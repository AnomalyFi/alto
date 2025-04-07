use std::{collections::HashMap, hash::Hash, time::{Duration, SystemTime}};

use bytes::{BufMut, Bytes};
use commonware_cryptography::{ed25519::PublicKey, sha256, Digest, Hasher, Sha256};
use commonware_p2p::{utils::requester, Receiver, Recipients, Sender};
use commonware_runtime::{Blob, Clock, Handle, Metrics, Spawner, Storage};
use commonware_resolver::{p2p, Resolver};
use commonware_storage::{
    archive::{self, translator::TwoCap, Archive, Identifier}, 
    journal::{self, variable::Journal},
};
use commonware_utils::SystemTimeExt;
use futures::{channel::{mpsc, oneshot}, SinkExt, StreamExt};
use commonware_macros::select;
use governor::Quota;
use rand::Rng;
use tracing::{debug, warn, info};
use governor::clock::Clock as GClock;
use super::{handler::{Handler, self}, key::{self, MultiIndex, Value}, ingress, coordinator::Coordinator, archive::Wrapped};
use crate::{actors::net, maybe_delay_between};
use alto_types::{signed_tx::SignedTx, tx::Tx, Batch};

pub enum Message<H: Hasher> {
    // mark batch as accepted by the netowrk through the broadcast protocol
    BatchAcknowledged {
        digest: H::Digest,
        response: oneshot::Sender<bool>
    },
    // from rpc or websocket
    SubmitTxs {
        payload: Vec<SignedTx<H>>,
        response: oneshot::Sender<Vec<bool>>
    },
    BatchConsumed {
        digests: Vec<H::Digest>,
        block_number: u64,
        response: oneshot::Sender<bool>,
    },
    // proposer consume batches to produce a block
    ConsumeBatches {
        response: oneshot::Sender<Vec<Batch<H>>>
    },
    GetTx {
        digest: H::Digest,
        response: oneshot::Sender<Option<SignedTx<H>>>
    },
    GetBatch {
        digest: H::Digest,
        response: oneshot::Sender<Option<Batch<H>>>
    },
    GetBatchContainTx {
        digest: H::Digest,
        response: oneshot::Sender<Option<Batch<H>>>
    }
}

#[derive(Clone)]
pub struct Mailbox<H: Hasher> {
    sender: mpsc::Sender<Message<H>>
}

impl<H: Hasher> Mailbox<H> {
    pub fn new(sender: mpsc::Sender<Message<H>>) -> Self {
        Self {
            sender
        }
    }

    pub async fn acknowledge_batch(&mut self, digest: H::Digest) -> bool {
        let (response, receiver) = oneshot::channel();
        self.sender
            .send(Message::BatchAcknowledged { digest, response})
            .await
            .expect("failed to acknowledge batch");

        receiver.await.expect("failed to receive batch acknowledge")
    }

    pub async fn submit_txs(&mut self, payload: Vec<SignedTx<H>>) -> Vec<bool> {
        let (response, receiver) = oneshot::channel();
        self.sender
            .send(Message::SubmitTxs { payload, response })
            .await
            .expect("failed to issue tx");

        receiver.await.expect("failed to receive tx issue status")
    }

    pub async fn consume_batches(&mut self) -> Vec<Batch<H>> {
        let (response, receiver) = oneshot::channel();
        self.sender
            .send(Message::ConsumeBatches { response })
            .await
            .expect("failed to consume batches");

        receiver.await.expect("failed to receive batches")
    }

    pub async fn consumed_batches(&mut self, digests: Vec<H::Digest>, block_number: u64) -> bool {
        let (response, receiver) = oneshot::channel();
        self.sender
            .send(Message::BatchConsumed { digests, block_number, response })
            .await
            .expect("failed to mark batches as consumed");

        receiver.await.expect("failed to mark batches as consumed")
    }

    pub async fn get_tx(&mut self, digest: H::Digest) -> Option<SignedTx<H>> {
        let (response, receiver) = oneshot::channel();
        self.sender
            .send(Message::GetTx { digest, response })
            .await
            .expect("failed to get tx");

        receiver.await.expect("failed to receive tx")
    }

    pub async fn get_batch(&mut self, digest: H::Digest) -> Option<Batch<H>> {
        let (response, receiver) = oneshot::channel();
        self.sender
            .send(Message::GetBatch { digest, response })
            .await
            .expect("failed to get batch");

        receiver.await.expect("failed to receive batch")
    }

    pub async fn get_batch_contain_tx(&mut self, digest: H::Digest) -> Option<Batch<H>> {
        let (response, receiver) = oneshot::channel();
        self.sender
            .send(Message::GetBatchContainTx { digest, response })
            .await
            .expect("failed to get batch");

        receiver.await.expect("failed to receive batch")
    }
}

pub struct Config {
    pub batch_propose_interval: Duration,
    pub batch_size_limit: u64,
    pub backfill_quota: Quota,
    pub mailbox_size: usize,
    pub public_key: PublicKey,
    pub partition_prefix: String,
    pub block_height: u64,
}

pub struct Mempool<
    B: Blob,
    R: Rng + Clock + GClock + Spawner + Metrics + Storage<B>, 
    H: Hasher
> {
    context: R,

    public_key: PublicKey,


    batches: HashMap<H::Digest, Batch<H>>,
    
    acknowledged: Vec<H::Digest>,

    //TODO: replace the following two
    accepted: Archive<TwoCap, H::Digest, B, R>,
    consumed: Archive<TwoCap, H::Digest, B, R>,

    txs: Vec<SignedTx<H>>,

    mailbox: mpsc::Receiver<Message<H>>,
    mailbox_size: usize,

    block_height_seen: u64,

    batch_propose_interval: Duration,
    batch_size_limit: u64,
    backfill_quota: Quota,
}

impl<
    B: Blob,
    R: Rng + Clock + GClock + Spawner + Metrics + Storage<B>, 
    H: Hasher,
> Mempool<B, R, H> {
    pub async fn init(context: R, cfg: Config) -> (Self, Mailbox<H>) {
        let accepted_journal = Journal::init(
            context.with_label("accepted_journal"), 
            journal::variable::Config {
                partition: format!("{}-acceptances", cfg.partition_prefix),
            })
            .await
            .expect("Failed to initialize accepted journal");
        let accepted_archive = Archive::init(
        context.with_label("accepted_archive"),
            accepted_journal, 
            archive::Config {
                translator: TwoCap,
                section_mask: 0xffff_ffff_fff0_0000u64,
                pending_writes: 0,
                replay_concurrency: 4,
                compression: Some(3),
            })
            .await
            .expect("Failed to initialize accepted archive");
            
        let consumed_journal = Journal::init(
            context.with_label("consumed_journal"), 
            journal::variable::Config {
                partition: format!("{}-consumptions", cfg.partition_prefix),
            })
            .await
            .expect("Failed to initialize consumed journal");
        let consumed_archive = Archive::init(
        context.with_label("consumed_archive"),
            consumed_journal, 
            archive::Config {
                translator: TwoCap,
                section_mask: 0xffff_ffff_fff0_0000u64,
                pending_writes: 0,
                replay_concurrency: 4,
                compression: Some(3),
            })
            .await
            .expect("Failed to initialize consumed archive");

        let (sender, receiver) = mpsc::channel(1024);
        (Self {
            context,
            public_key: cfg.public_key,
            batches: HashMap::new(),
            acknowledged: Vec::new(),
            accepted: accepted_archive,
            consumed: consumed_archive,

            txs: vec![],

            block_height_seen: cfg.block_height,

            mailbox: receiver,
            mailbox_size: cfg.mailbox_size,

            batch_propose_interval: cfg.batch_propose_interval,
            batch_size_limit: cfg.batch_size_limit,
            backfill_quota: cfg.backfill_quota,
        }, Mailbox::new(sender))
    }

    pub fn start(
        mut self,
        batch_network: (
            impl Sender<PublicKey = PublicKey>,
            impl Receiver<PublicKey = PublicKey>,
        ),
        backfill_network: (
            impl Sender<PublicKey = PublicKey>,
            impl Receiver<PublicKey = PublicKey>,
        ), 
        coordinator: Coordinator<PublicKey>,
        app_mailbox: ingress::Mailbox<H, PublicKey>
    ) -> Handle<()> {
        self.context.spawn_ref()(self.run(batch_network, backfill_network, coordinator, app_mailbox))
    }

    pub async fn run(
        mut self,
        mut batch_network: (
            impl Sender<PublicKey = PublicKey>,
            impl Receiver<PublicKey = PublicKey>,
        ), 
        backfill_network: (
            impl Sender<PublicKey = PublicKey>,
            impl Receiver<PublicKey = PublicKey>,
        ), 
        coordinator: Coordinator<PublicKey>,
        mut app_mailbox: ingress::Mailbox<H, PublicKey>
    ) {
        let (handler_sender, mut handler_receiver) = mpsc::channel(self.mailbox_size);
        let handler = Handler::<H>::new(handler_sender);
        let (resolver_engine, mut resolver) = p2p::Engine::new(
            self.context.with_label("resolver"),
            p2p::Config {
                coordinator: coordinator,
                consumer: handler.clone(),
                producer: handler,
                mailbox_size: self.mailbox_size,
                requester_config: requester::Config {
                    public_key: self.public_key.clone(),
                    rate_limit: self.backfill_quota,
                    initial: Duration::from_secs(1),
                    timeout: Duration::from_secs(2),
                },
                fetch_retry_timeout: Duration::from_millis(100), // prevent busy loop
                priority_requests: false,
                priority_responses: false,
            },
        );
        resolver_engine.start(backfill_network);

        let mut waiters: HashMap<H::Digest, Vec<oneshot::Sender<Option<Batch<H>>>>> = HashMap::new();
        let mut propose_timeout = self.context.current() + self.batch_propose_interval;
        let accepted = Wrapped::new(self.accepted);
        let consumed = Wrapped::new(self.consumed);
        loop {
            // Clear dead waiters
            waiters.retain(|_, waiters| {
                waiters.retain(|waiter| !waiter.is_canceled());
                !waiters.is_empty()
            });

            select! {
                mailbox_message = self.mailbox.next() => {
                    let Some(message) = mailbox_message else {
                        info!("Mailbox closed, terminating...");
                        return;
                    };
                    match message {
                        Message::SubmitTxs { payload, response } => {
                            // TODO: add timestamp verification here

                            let mut result = Vec::with_capacity(payload.len());
                            for tx in payload.into_iter() {
                                if !tx.validate() {
                                    result.push(false);
                                    continue
                                }
                                self.txs.push(tx);
                                result.push(true);
                            }
                            let _ = response.send(result);
                        },
                        // batch ackowledged by the network 
                        Message::BatchAcknowledged { digest, response } => {
                            debug!("batch accepted by the network: {}", digest);

                            self.acknowledged.push(digest);
                            if let Some(batch) = self.batches.get_mut(&digest) {
                                accepted.put(self.block_height_seen, digest, batch.serialize().into()).await.expect("unable to store accepted batch");
                            } else {
                                panic!("batch not found: {}", digest);
                            }
                            let _ = response.send(true);
                        },
                        Message::ConsumeBatches { response } => { 
                            // we do not remove anything from the mempool state as the digests/batches may not be consumed
                            let batches = self.acknowledged.iter().filter_map(|digest| {
                                let Some(batch) = self.batches.get(&digest) else {
                                    // shouldn't happen
                                    panic!("batch not found: {}", digest);
                                };
                                Some(batch.clone())
                            }).collect();
                            let _ = response.send(batches);
                        },
                        // received when a block is finalized, i.e. finalization message is received, 
                        // remove consumed batches from buffer & put them in consumed archive
                        Message:: BatchConsumed { digests, block_number, response } => {
                            // update the seen height
                            self.block_height_seen = block_number;

                            let consumed_keys: Vec<H::Digest> = self.batches.iter()
                                .filter_map(|(digest, batch)| {
                                    if digests.contains(&batch.digest) {
                                        Some(digest.clone())
                                    } else {
                                        None
                                    }
                                })
                                .collect();
                            let consumed_keys_len = consumed_keys.len();

                            // not all provided keys are consumed, there must be state corruption, panic immediately
                            if consumed_keys_len != digests.len() {
                                panic!("not all provided batch digests consumed, provided={:?}, consumed={:?}", digests, consumed_keys);
                            }

                            // remove digests and batches
                            let consumed_batches: Vec<Batch<H>> = consumed_keys.into_iter()
                                .filter_map(|key| self.batches.remove(&key))
                                .collect();

                            self.acknowledged.retain(|digest| !digests.contains(digest));

                            for batch in consumed_batches.iter() {
                                consumed.put(block_number, batch.digest, batch.serialize().into()).await.expect("Failed to insert accepted batch");
                            }


                            let _ = response.send(true);
                        },
                        // for validators, this should be only called when they receive 
                        // 1. a digest from broadcast primitive
                        // 2. a block with a list of references of batches
                        // both of the above should only happen at their verification stage
                        Message::GetBatch { digest, response } => { 
                            // fetch in buffer, i.e. accepted
                            if let Some(batch) = self.batches.get(&digest).cloned() {
                                let _ = response.send(Some(batch));
                                continue;
                            };
                            // fetch in consumed
                            let consumed = consumed.get(Identifier::Key(&digest))
                                .await 
                                .expect("Failed to get consumed batch");
                            if let Some(consumed) = consumed {
                                let consumed = Batch::deserialize(&consumed).expect("unable to deserialize batch");
                                let _ = response.send(Some(consumed));
                                continue;
                            }
                            
                            // not found in the above, request from other peers
                            resolver.fetch(MultiIndex::new(Value::Digest(digest.into()))).await;
                            waiters.entry(digest).or_default().push(response);
                        },
                        Message::GetBatchContainTx { digest, response } => {
                            // TODO: optimize this naive way of seaching
                            let pair = self.batches.iter().find(|(_, batch)| batch.contain_tx(&digest));
                            if let Some((_, batch)) = pair {
                                let _ = response.send(Some(batch.clone()));
                            } else {
                                let _ = response.send(None);
                            }
                        },
                        Message::GetTx { digest, response } => {
                            // TODO: optimize this naive way of seaching
                            let pair = self.batches.iter().find(|(_, batch)| batch.contain_tx(&digest));
                            if let Some((_, batch)) = pair {
                                let _ = response.send(batch.tx(&digest));
                            } else {
                                let _ = response.send(None);
                            }
                        },
                    }
                },
                // propose a batch in a given interval
                _ = self.context.sleep_until(propose_timeout) => {
                    let mut size = 0;
                    let mut txs_cnt = 0;
                    for tx in self.txs.iter() {
                        size += tx.size() as u64;
                        txs_cnt += 1;

                        if size > self.batch_size_limit {
                            break;
                        }
                    }

                    if txs_cnt == 0 {
                        propose_timeout = self.context.current() + self.batch_propose_interval;
                        continue;
                    }

                    let batch = Batch::new(self.txs.drain(..txs_cnt).collect(), self.context.current());
                    self.batches.insert(batch.digest, batch.clone());

                    debug!("broadcasting batch & digest={}, peer={}", batch.digest, self.public_key.clone());
                    maybe_delay_between! {
                        self.context,
                        app_mailbox.broadcast(batch.digest).await;
                        batch_network.0.send(Recipients::All, batch.serialize().into(), true).await.expect("failed to broadcast batch");
                    }

                    // reset the timeout
                    propose_timeout = self.context.current() + self.batch_propose_interval;
                },
                batch_message = batch_network.1.recv() => {
                    let (sender, message) = batch_message.expect("Batch broadcast closed");
                    let Ok(batch) = Batch::deserialize(&message) else {
                        warn!(?sender, "failed to deserialize batch");
                        continue;
                    };

                    debug!(?sender, digest=?batch.digest, receiver=?self.public_key, "received batch");
                    let _ = self.batches.entry(batch.digest).or_insert(batch);
                },

                // handle batch request, a validator will issue batches it has
                handler_message = handler_receiver.next() => {
                    let message = handler_message.expect("Handler closed");
                    match message {
                        handler::Message::Produce { key, response } => {
                            match key.to_value() {
                                key::Value::Digest(digest) => {
                                    if let Some(batch) = self.batches.get(&digest).cloned() {
                                        let _ = response.send(batch.serialize().into());
                                        continue;
                                    };

                                    // TODO: replace with new key-value db
                                    // let consumed = consumed.get(Identifier::Key(&H::Digest::from(digest)))
                                    //     .await 
                                    //     .expect("Failed to get accepted batch");
                                    // if let Some(consumed) = consumed {
                                    //     let _ = response.send(consumed);
                                    //     continue;
                                    // }
                                    debug!(?digest, "missing batch");
                                }
                            }         
                        },
                        handler::Message::Deliver { key, value, response } => {
                            match key.to_value() {
                                key::Value::Digest(digest) => {
                                    let batch = Batch::deserialize(&value).expect("Failed to deserialize batch");
                                    if batch.digest != digest {
                                        let _ = response.send(false);
                                        continue;
                                    }

                                    if let Some(waiters) = waiters.remove(&batch.digest) {
                                        debug!(?batch.digest, ?self.public_key, "waiters resolved via batch");
                                        for waiter in waiters {
                                            let _ = waiter.send(Some(batch.clone()));
                                        }
                                    }

                                    debug!(?batch.digest, "receive a batch from resolver");
                                    // add received batch to buffer if not exists
                                    self.batches.entry(batch.digest).or_insert(batch);

                                    let _ = response.send(true);
                                }
                            }         
                        },
                    }
                }
            }
        }
    }
}
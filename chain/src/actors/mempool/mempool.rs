use std::{alloc::System, collections::HashMap, time::{Duration, SystemTime}};

use bytes::{BufMut, Bytes};
use commonware_cryptography::{bls12381::primitives::group::Public, ed25519::PublicKey, sha256, Digest, Hasher, Sha256};
use commonware_p2p::{utils::requester, Receiver, Recipients, Sender};
use commonware_runtime::{Blob, Clock, Handle, Metrics, Spawner, Storage};
use commonware_resolver::{p2p, Resolver};
use commonware_storage::{
    archive::{self, translator::TwoCap, Archive, Identifier}, 
    journal::{self, variable::Journal},
};
use commonware_utils::{Array, SystemTimeExt};
use futures::{channel::{mpsc, oneshot}, SinkExt, StreamExt};
use commonware_macros::select;
use governor::Quota;
use rand::Rng;
use tracing::{debug, warn};
use governor::clock::Clock as GClock;
use tracing_subscriber::fmt::time;
use super::{actor, handler::{Handler, self}, key::{self, MultiIndex, Value}, ingress, coordinator::Coordinator, archive::Wrapped};

#[derive(Clone)]
pub struct Batch<D: Digest>  {
    pub timestamp: SystemTime,
    // mark if the batch is accepted by the network, i.e. the network has received & verified the batch 
    pub accepted: bool, 

    pub txs: Vec<RawTransaction<D>>,
    pub digest: D,
}

impl<D: Digest> Batch<D> 
    where Sha256: Hasher<Digest = D>
{
    fn compute_digest(txs: &Vec<RawTransaction<D>>) -> D {
        let mut hasher = Sha256::new();

        for tx in txs.into_iter() {
            hasher.update(tx.raw.as_ref());
        }

        hasher.finalize()
    }

    pub fn new(txs: Vec<RawTransaction<D>>, timestamp: SystemTime) -> Self {
        let digest = Self::compute_digest(&txs);

        Self {
            txs,
            digest,
            accepted: false,
            timestamp 
        }
    }

    pub fn serialize(&self) -> Vec<u8> {
        let mut bytes = Vec::new();
        bytes.put_u64(self.timestamp.epoch_millis());
        bytes.put_u64(self.txs.len() as u64);
        for tx in self.txs.iter() {
            bytes.put_u64(tx.size());
            bytes.extend_from_slice(&tx.raw);
        }
        bytes
    }

    pub fn deserialize(mut bytes: &[u8]) -> Option<Self> {
        use bytes::Buf;
        // We expect at least 18 bytes for the header
        if bytes.remaining() < 18 {
            return None;
        }
        let timestamp = bytes.get_u64();
        let timestamp = SystemTime::UNIX_EPOCH + Duration::from_millis(timestamp);

        let tx_count = bytes.get_u64();
        let mut txs = Vec::with_capacity(tx_count as usize);
        for _ in 0..tx_count {
            // For each transaction, first read the size (u64).
            if bytes.remaining() < 8 {
                return None;
            }
            let tx_size = bytes.get_u64() as usize;
            // Ensure there are enough bytes left.
            if bytes.remaining() < tx_size {
                return None;
            }
            // Extract tx_size bytes.
            let tx_bytes = bytes.copy_to_bytes(tx_size);
            txs.push(RawTransaction::new(tx_bytes));
        }
        // Compute the digest from the transactions.
        let digest = Self::compute_digest(&txs);
        // Since serialize did not include accepted and timestamp, we set accepted to false
        // and set timestamp to the current time.
        Some(Self {
            timestamp,
            accepted: false,
            txs,
            digest,
        })
    }

    pub fn contain_tx(&self, digest: &D) -> bool {
        self.txs.iter().any(|tx| &tx.digest == digest) 
    }

    pub fn tx(&self, digest: &D) -> Option<RawTransaction<D>> {
        self.txs.iter().find(|tx| &tx.digest == digest).cloned()
    }
}

#[derive(Clone)]
pub struct RawTransaction<D: Digest> {
    pub raw: Bytes,

    pub digest: D
}

impl<D: Digest> RawTransaction<D> 
    where Sha256: Hasher<Digest = D>
{
    fn compute_digest(raw: &Bytes) -> D {
        let mut hasher = Sha256::new();
        hasher.update(&raw);
        hasher.finalize()
    }

    pub fn new(raw: Bytes) -> Self {
        let digest = Self::compute_digest(&raw);
        Self {
            raw,
            digest
        }
    }

    pub fn validate(&self) -> bool {
        // TODO: implement validate here
        true
    }

    pub fn size(&self) -> u64 {
        self.raw.len() as u64
    }
}

pub enum Message<D: Digest> {
    // mark batch as accepted by the netowrk through the broadcast protocol
    BatchAcknowledged {
        digest: D,
        response: oneshot::Sender<bool>
    },
    // from rpc or websocket
    SubmitTx {
        payload: RawTransaction<D>,
        response: oneshot::Sender<bool>
    },
    BatchConsumed {
        digests: Vec<D>,
        block_number: u64,
    },
    // proposer consume batches to produce a block
    ConsumeBatches {
        response: oneshot::Sender<Vec<Batch<D>>>
    },
    GetTx {
        digest: D,
        response: oneshot::Sender<Option<RawTransaction<D>>>
    },
    GetBatch {
        digest: D,
        response: oneshot::Sender<Option<Batch<D>>>
    },
    GetBatchContainTx {
        digest: D,
        response: oneshot::Sender<Option<Batch<D>>>
    }
}

#[derive(Clone)]
pub struct Mailbox<D: Digest> {
    sender: mpsc::Sender<Message<D>>
}

impl<D: Digest> Mailbox<D> {
    pub fn new(sender: mpsc::Sender<Message<D>>) -> Self {
        Self {
            sender
        }
    }

    pub async fn acknowledge_batch(&mut self, digest: D) -> bool {
        let (response, receiver) = oneshot::channel();
        self.sender
            .send(Message::BatchAcknowledged { digest, response})
            .await
            .expect("failed to acknowledge batch");

        receiver.await.expect("failed to receive batch acknowledge")
    }

    pub async fn issue_tx(&mut self, tx: RawTransaction<D>) -> bool {
        let (response, receiver) = oneshot::channel();
        self.sender
            .send(Message::SubmitTx { payload: tx, response })
            .await
            .expect("failed to issue tx");

        receiver.await.expect("failed to receive tx issue status")
    }

    pub async fn consume_batches(&mut self) -> Vec<Batch<D>> {
        let (response, receiver) = oneshot::channel();
        self.sender
            .send(Message::ConsumeBatches { response })
            .await
            .expect("failed to consume batches");

        receiver.await.expect("failed to receive batches")
    }

    pub async fn consumed_batches(&mut self, digests: Vec<D>, block_number: u64) -> Vec<Batch<D>> {
        let (_, receiver) = oneshot::channel();
        self.sender
            .send(Message::BatchConsumed { digests, block_number})
            .await
            .expect("failed to mark batches as consumed");

        receiver.await.expect("failed to mark batches as consumed")
    }

    pub async fn get_tx(&mut self, digest: D) -> Option<RawTransaction<D>> {
        let (response, receiver) = oneshot::channel();
        self.sender
            .send(Message::GetTx { digest, response })
            .await
            .expect("failed to get tx");

        receiver.await.expect("failed to receive tx")
    }

    pub async fn get_batch(&mut self, digest: D) -> Option<Batch<D>> {
        let (response, receiver) = oneshot::channel();
        self.sender
            .send(Message::GetBatch { digest, response })
            .await
            .expect("failed to get batch");

        receiver.await.expect("failed to receive batch")
    }

    pub async fn get_batch_contain_tx(&mut self, digest: D) -> Option<Batch<D>> {
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
    D: Digest + Into<sha256::Digest> + From<sha256::Digest>
> {
    context: R,

    public_key: PublicKey,

    batches: HashMap<D, Batch<D>>,
    accepted: Archive<TwoCap, D, B, R>,
    consumed: Archive<TwoCap, D, B, R>,

    txs: Vec<RawTransaction<D>>,

    mailbox: mpsc::Receiver<Message<D>>,
    mailbox_size: usize,

    block_height_seen: u64,

    batch_propose_interval: Duration,
    batch_size_limit: u64,
    backfill_quota: Quota,
}

impl<
    B: Blob,
    R: Rng + Clock + GClock + Spawner + Metrics + Storage<B>, 
    D: Digest + Into<sha256::Digest> + From<sha256::Digest>
> Mempool<B, R, D> 
    where 
        Sha256: Hasher<Digest = D>,
{
    pub async fn init(context: R, cfg: Config) -> (Self, Mailbox<D>) {
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
        app_mailbox: ingress::Mailbox<D, PublicKey>
    ) -> Handle<()> {
        self.context.spawn_ref()(self.run(batch_network, backfill_network, coordinator, app_mailbox))
    }

    async fn run(
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
        mut app_mailbox: ingress::Mailbox<D, PublicKey>
    ) {
        let (handler_sender, mut handler_receiver) = mpsc::channel(self.mailbox_size);
        let handler = Handler::new(handler_sender);
        let (resolver_engine, mut resolver) = p2p::Engine::new(
            self.context.with_label("resolver"),
            p2p::Config {
                coordinator: coordinator,
                consumer: handler.clone(),
                producer: handler,
                mailbox_size: self.mailbox_size,
                requester_config: requester::Config {
                    public_key: self.public_key,
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

        let mut waiters: HashMap<D, Vec<oneshot::Sender<Option<Batch<D>>>>> = HashMap::new();
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
                    // let message = mailbox_message.expect("Mailbox closed");
                    let Some(message) = mailbox_message else {
                        // TODO: revisit this branch, it was .expect("Mailbox closed") and will panic after unit test is finished
                        debug!("Mailbox closed, terminating...");
                        return;
                    };
                    match message {
                        Message::SubmitTx { payload, response } => {
                            if !payload.validate() {
                                let _ = response.send(false);
                                return;    
                            }

                            self.txs.push(payload);
                            let _ = response.send(true);
                        },
                        // batch ackowledged by the network, put in the accepted archive and mark batch as accepted
                        Message::BatchAcknowledged { digest, response } => {
                            if let Some(batch) = self.batches.get_mut(&digest) {
                                accepted.put(self.block_height_seen, digest, batch.serialize().into()).await.expect("unable to store accepted batch");
                                batch.accepted = true;
                                let _ = response.send(true);
                                debug!("batch accepted by the network: {}", digest);
                            } else {
                                let _ = response.send(false);
                            }
                        },
                        Message::ConsumeBatches { response } => { 
                            let batches = self.batches.iter().filter(|(_, batch)| batch.accepted).map(|(_, batch)| batch.clone()).collect();
                            let _ = response.send(batches);
                        },
                        // received when a block is finalized, i.e. finalization message is received, 
                        // remove consumed batches from buffer & put them in consumed archive
                        Message:: BatchConsumed { digests, block_number } => {
                            // update the seen height
                            self.block_height_seen = block_number;

                            let consumed_keys: Vec<D> = self.batches.iter()
                                .filter_map(|(digest, batch)| {
                                    if digests.contains(&batch.digest) {
                                        Some(digest.clone())
                                    } else {
                                        None
                                    }
                                })
                                .collect();

                            let consumed_batches: Vec<Batch<D>> = consumed_keys.into_iter()
                                .filter_map(|key| self.batches.remove(&key))
                                .collect();

                            for batch in consumed_batches.iter() {
                                consumed.put(block_number, batch.digest, batch.serialize().into()).await.expect("Failed to insert accepted batch");
                            }
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
                        size += tx.size();
                        txs_cnt += 1;

                        if size > self.batch_size_limit {
                            break;
                        }
                    }

                    if txs_cnt == 0 {
                        propose_timeout = self.context.current() + self.batch_propose_interval;
                        continue;
                    }

                    // 1. send raw batch over p2p
                    let batch = Batch::new(self.txs.drain(..txs_cnt).collect(), self.context.current());
                    self.batches.insert(batch.digest, batch.clone());
                    batch_network.0.send(Recipients::All, batch.serialize().into(), true).await.expect("failed to broadcast batch");
                    // 2. send batch digest over broadcast layer
                    app_mailbox.broadcast(batch.digest).await;

                    // reset the timeout
                    propose_timeout = self.context.current() + self.batch_propose_interval;
                },
                batch_message = batch_network.1.recv() => {
                    let (sender, message) = batch_message.expect("Batch broadcast closed");
                    let Some(batch) = Batch::deserialize(&message) else {
                        warn!(?sender, "failed to deserialize batch");
                        continue;
                    };

                    debug!(?sender, digest=?batch.digest, "received batch");
                    let _ = self.batches.entry(batch.digest).or_insert(batch);
                },

                // handle batch request, a validator will issue batches it has
                handler_message = handler_receiver.next() => {
                    let message = handler_message.expect("Handler closed");
                    match message {
                        handler::Message::Produce { key, response } => {
                            match key.to_value() {
                                key::Value::Digest(digest) => {
                                    if let Some(batch) = self.batches.get(&D::from(digest)).cloned() {
                                        let _ = response.send(batch.serialize().into());
                                        continue;
                                    };

                                    let consumed = consumed.get(Identifier::Key(&D::from(digest)))
                                        .await 
                                        .expect("Failed to get accepted batch");
                                    if let Some(consumed) = consumed {
                                        let _ = response.send(consumed);
                                        continue;
                                    }
                                }
                            }         
                        },
                        handler::Message::Deliver { key, value, response } => {
                            match key.to_value() {
                                key::Value::Digest(digest) => {
                                    let batch = Batch::deserialize(&value).expect("Failed to deserialize batch");
                                    if batch.digest.into() != digest {
                                        let _ = response.send(false);
                                        continue;
                                    }

                                    if let Some(waiters) = waiters.remove(&batch.digest) {
                                        debug!(?batch.digest, "waiters resolved via batch");
                                        for waiter in waiters {
                                            let _ = waiter.send(Some(batch.clone()));
                                        }
                                    }

                                    debug!(?batch.digest, "receive a batch from resolver");
                                    // add received batch to buffer
                                    self.batches.insert(batch.digest, batch);
                                }
                            }         
                        },
                    }
                }
            }
        }
    }
}
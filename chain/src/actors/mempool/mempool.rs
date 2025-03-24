use std::{collections::HashMap, time::{Duration, SystemTime}};

use bytes::{BufMut, Bytes};
use commonware_cryptography::{bls12381::primitives::group::Public, ed25519::PublicKey, sha256::{self}, Digest, Hasher, Sha256};
use commonware_p2p::{Receiver, Recipients, Sender};
use commonware_runtime::{Clock, Handle, Spawner};
use futures::{channel::{mpsc, oneshot}, SinkExt, StreamExt};
use commonware_macros::select;
use tracing::{debug, warn};

use super::{actor, ingress};

#[derive(Clone)]
pub struct Batch<D: Digest>  {
    pub txs: Vec<RawTransaction<D>>,

    pub digest: D,
    // mark if the batch is accepted by the network, i.e. the network has received & verified the batch 
    pub accepted: bool, 
    pub timestamp: SystemTime,
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
        bytes.put_u64(self.txs.len() as u64);
        for tx in self.txs.iter() {
            bytes.put_u64(tx.size());
            bytes.extend_from_slice(&tx.raw);
        }
        bytes
    }

    pub fn deserialize(mut bytes: &[u8]) -> Option<Self> {
        use bytes::Buf;
        // We expect at least 8 bytes for the number of transactions.
        if bytes.remaining() < 8 {
            return None;
        }
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
            txs,
            digest,
            accepted: false,
            timestamp: SystemTime::now(),
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
}

pub struct Mempool<R: Clock + Spawner, D: Digest> {
    cfg: Config,
    context: R,

    batches: HashMap<D, Batch<D>>,
    txs: Vec<RawTransaction<D>>,

    mailbox: mpsc::Receiver<Message<D>>,
}

impl<R: Clock + Spawner, D: Digest> Mempool<R, D> 
    where Sha256: Hasher<Digest = D>,
{
    pub fn new(context: R, cfg: Config) -> (Self, Mailbox<D>) {
        let (sender, receiver) = mpsc::channel(1024);
        (Self {
            cfg, 
            context,
            mailbox: receiver,
            txs: vec![],
            batches: HashMap::new(),
        }, Mailbox::new(sender))
    }

    pub fn start(
        mut self,
        batch_network: (
            impl Sender<PublicKey = PublicKey>,
            impl Receiver<PublicKey = PublicKey>,
        ),
        mut app_mailbox: ingress::Mailbox<D, PublicKey>
    ) -> Handle<()> {
        self.context.spawn_ref()(self.run(batch_network, app_mailbox))
    }

    async fn run(
        mut self,
        mut batch_network: (
            impl Sender<PublicKey = PublicKey>,
            impl Receiver<PublicKey = PublicKey>,
        ), 
        mut app_mailbox: ingress::Mailbox<D, PublicKey>
    ) {
        let mut propose_timeout = self.context.current() + self.cfg.batch_propose_interval;
        loop {
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
                        Message::BatchAcknowledged { digest, response } => {
                            if let Some(batch) = self.batches.get_mut(&digest) {
                                batch.accepted = true;
                                let _ = response.send(true);
                                debug!("batch accepted by the network: {}", digest);
                            } else {
                                let _ = response.send(false);
                            }
                        },
                        Message::GetBatch { digest, response } => { 
                            let batch = self.batches.get(&digest).cloned();
                            let _ = response.send(batch);
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
                _ = self.context.sleep_until(propose_timeout) => {
                    let mut size = 0;
                    let mut txs_cnt = 0;
                    for tx in self.txs.iter() {
                        size += tx.size();
                        txs_cnt += 1;

                        if size > self.cfg.batch_size_limit {
                            break;
                        }
                    }

                    if txs_cnt == 0 {
                        propose_timeout = self.context.current() + self.cfg.batch_propose_interval;
                        continue;
                    }

                    // 1. send raw batch over p2p
                    let batch = Batch::new(self.txs.drain(..txs_cnt).collect(), self.context.current());
                    self.batches.insert(batch.digest, batch.clone());
                    batch_network.0.send(Recipients::All, batch.serialize().into(), true).await.expect("failed to broadcast batch");
                    // 2. send batch digest over broadcast layer
                    app_mailbox.broadcast(batch.digest).await;

                    // reset the timeout
                    propose_timeout = self.context.current() + self.cfg.batch_propose_interval;
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
            }
        }
    }
}
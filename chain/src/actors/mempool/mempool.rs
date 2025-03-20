use std::{collections::HashMap, time::{Duration, SystemTime}};

use bytes::{BufMut, Bytes};
use commonware_cryptography::{ed25519::PublicKey, sha256::Digest, Hasher, Sha256};
use commonware_p2p::{Receiver, Recipients, Sender};
use commonware_runtime::{Clock, Handle, Spawner};
use futures::{channel::{mpsc, oneshot}, SinkExt, StreamExt};
use commonware_macros::select;
use tokio::time;
use tracing::{debug, warn};

#[derive(Clone)]
pub struct Batch  {
    pub txs: Vec<RawTransaction>,

    pub digest: Digest,
    // mark if the batch is accepted by the network, i.e. the network has received & verified the batch 
    pub accepted: bool, 
    pub timestamp: SystemTime,
}

impl Batch {
    fn compute_digest(txs: &Vec<RawTransaction>) -> Digest {
        let mut hasher = Sha256::new();
        for tx in txs.into_iter() {
            hasher.update(tx.raw.as_ref());
        }

        hasher.finalize()
    }

    pub fn new(txs: Vec<RawTransaction>, timestamp: SystemTime) -> Self {
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

    pub fn contain_tx(&self, digest: &Digest) -> bool {
        self.txs.iter().any(|tx| &tx.digest == digest) 
    }

    pub fn tx(&self, digest: &Digest) -> Option<RawTransaction> {
        self.txs.iter().find(|tx| &tx.digest == digest).cloned()
    }
}

#[derive(Clone)]
pub struct RawTransaction {
    pub raw: Bytes,

    pub digest: Digest
}

impl RawTransaction {
    fn compute_digest(raw: &Bytes) -> Digest {
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

pub enum Message {
    // mark batch as accepted by the netowrk through the broadcast protocol
    BatchAccepted {
        digest: Digest,
    },
    // from rpc or websocket
    SubmitTx {
        payload: RawTransaction,
        response: oneshot::Sender<bool>
    },
    GetTx {
        digest: Digest,
        response: oneshot::Sender<Option<RawTransaction>>
    },
}

#[derive(Clone)]
pub struct Mailbox {
    sender: mpsc::Sender<Message>
}

impl Mailbox {
    pub fn new(sender: mpsc::Sender<Message>) -> Self {
        Self {
            sender
        }
    }

    pub async fn issue_tx(&mut self, tx: RawTransaction) -> bool {
        let (response, receiver) = oneshot::channel();
        self.sender
            .send(Message::SubmitTx { payload: tx, response })
            .await
            .expect("failed to issue tx");

        receiver.await.expect("failed to receive tx issue status")
    }

    pub async fn get_tx(&mut self, digest: Digest) -> Option<RawTransaction> {
        let (response, receiver) = oneshot::channel();
        self.sender
            .send(Message::GetTx { digest, response })
            .await
            .expect("failed to get tx");

        receiver.await.expect("failed to receive tx")
    }
}

pub struct Config {
    pub batch_propose_interval: Duration,
    pub batch_size_limit: u64,
}

pub struct Mempool<R: Clock + Spawner> {
    cfg: Config,
    context: R,

    batches: HashMap<Digest, Batch>,
    txs: Vec<RawTransaction>,

    mailbox: mpsc::Receiver<Message>
}

impl<R: Clock + Spawner> Mempool<R> {
    pub fn new(context: R, cfg: Config) -> (Self, Mailbox) {
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
        )
    ) -> Handle<()> {
        self.context.spawn_ref()(self.run(batch_network))
    }

    async fn run(
        mut self,
        mut batch_network: (
            impl Sender<PublicKey = PublicKey>,
            impl Receiver<PublicKey = PublicKey>,
        ), 
    ) {

        let mut batch_propose_interval = time::interval(self.cfg.batch_propose_interval);
        loop {
            select! {
                mailbox_message = self.mailbox.next() => {
                    let message = mailbox_message.expect("Mailbox closed");
                    match message {
                        Message::SubmitTx { payload, response } => {
                            if !payload.validate() {
                                let _ = response.send(false);
                                return;    
                            }

                            self.txs.push(payload);
                            let _ = response.send(true);
                        },
                        Message::BatchAccepted { digest } => {
                            if let Some(batch) = self.batches.get_mut(&digest) {
                                batch.accepted = true;
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
                _ = batch_propose_interval.tick() => {
                    let mut size = 0;
                    let mut txs_cnt = 0;
                    for tx in self.txs.iter() {
                        size += tx.size();
                        txs_cnt += 1;

                        if size > self.cfg.batch_size_limit {
                            break;
                        }
                    }

                    let batch = Batch::new(self.txs.drain(..txs_cnt).collect(), self.context.current());
                    self.batches.insert(batch.digest, batch.clone());
                    batch_network.0.send(Recipients::All, batch.serialize().into(), true).await.expect("failed to broadcast batch");
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
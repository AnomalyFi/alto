use commonware_broadcast::{linked::Prover, Collector as Z, Proof, };
use commonware_cryptography::{bls12381::primitives::group, Digest, Scheme};
use futures::{
    channel::{mpsc, oneshot},
    SinkExt, StreamExt,
};
use std::{
    cmp::max,
    collections::{BTreeMap, HashMap},
};
use tracing::error;

use super::mempool;

enum Message<C: Scheme, D: Digest> {
    Acknowledged(Proof, D),
    _Phantom(C::PublicKey),    
}

pub struct Collector<C: Scheme, D: Digest> {
    mailbox: mpsc::Receiver<Message<C, D>>,

    // Application namespace
    namespace: Vec<u8>,

    // Public key of the group
    public: group::Public,
}

impl<C: Scheme, D: Digest> Collector<C, D> {
    pub fn new(namespace: &[u8], public: group::Public) -> (Self, Mailbox<C, D>) {
        let (sender, receiver) = mpsc::channel(1024);
        (
            Collector {
                mailbox: receiver,
                namespace: namespace.to_vec(),
                public,
            },
            Mailbox { sender },
        )
    }

    pub async fn run(mut self, mut mempool: mempool::Mailbox<D>) {
        while let Some(msg) = self.mailbox.next().await {
            match msg {
                Message::Acknowledged(proof, payload) => {
                    // Check proof.
                    // The prover checks the validity of the threshold signature when deserializing
                    let prover = Prover::<C, D>::new(&self.namespace, self.public);
                    let _ = match prover.deserialize_threshold(proof) {
                        Some((context, _payload, _epoch, _threshold)) => context,
                        None => {
                            error!("invalid proof");
                            continue;
                        }
                    };

                    // Acknowledge batch in mempool, mark the batch as ready for pickup
                    let acknowledge = mempool.acknowledge_batch(payload).await;
                    if !acknowledge {
                        error!("unable to acknowledge batch {}", payload)
                    }
                }
                _ => unreachable!()
            }
        }
    }
}

#[derive(Clone)]
pub struct Mailbox<C: Scheme, D: Digest> {
    sender: mpsc::Sender<Message<C, D>>,
}

impl<C: Scheme, D: Digest> Z for Mailbox<C, D> {
    type Digest = D;
    async fn acknowledged(&mut self, proof: Proof, payload: Self::Digest) {
        self.sender
            .send(Message::Acknowledged(proof, payload))
            .await
            .expect("Failed to send acknowledged");
    }
}
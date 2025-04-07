use commonware_broadcast::{linked::Prover, Collector as Z, Proof, };
use commonware_cryptography::{bls12381::primitives::group, Digest, Hasher, Scheme};
use futures::{
    channel::mpsc,
    SinkExt, StreamExt,
};
use tracing::error;

use super::mempool;

enum Message<C: Scheme, H: Hasher> {
    Acknowledged(Proof, H::Digest),
    _Phantom(C::PublicKey),    
}

pub struct Collector<C: Scheme, H: Hasher> {
    mailbox: mpsc::Receiver<Message<C, H>>,

    // Application namespace
    namespace: Vec<u8>,

    // Public key of the group
    public: group::Public,
}

impl<C: Scheme, H: Hasher> Collector<C, H> {
    pub fn new(namespace: &[u8], public: group::Public) -> (Self, Mailbox<C, H>) {
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

    pub async fn run(mut self, mut mempool: mempool::Mailbox<H>) {
        while let Some(msg) = self.mailbox.next().await {
            match msg {
                Message::Acknowledged(proof, payload) => {
                    // Check proof.
                    // The prover checks the validity of the threshold signature when deserializing
                    let prover = Prover::<C, H::Digest>::new(&self.namespace, self.public);
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
pub struct Mailbox<C: Scheme, H: Hasher> {
    sender: mpsc::Sender<Message<C, H>>,
}

impl<C: Scheme, H: Hasher> Z for Mailbox<C, H> {
    type Digest = H::Digest;
    async fn acknowledged(&mut self, proof: Proof, payload: Self::Digest) {
        self.sender
            .send(Message::Acknowledged(proof, payload))
            .await
            .expect("Failed to send acknowledged");
    }
}
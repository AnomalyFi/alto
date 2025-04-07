use super::key::MultiIndex;
use bytes::Bytes;
use commonware_cryptography::{Digest, Hasher};
use commonware_resolver::{p2p::Producer, Consumer};
use futures::{
    channel::{mpsc, oneshot},
    SinkExt,
};
use tracing::warn;

pub enum Message<H: Hasher> {
    Deliver {
        key: MultiIndex<H::Digest>,
        value: Bytes,
        response: oneshot::Sender<bool>,
    },
    Produce {
        key: MultiIndex<H::Digest>,
        response: oneshot::Sender<Bytes>,
    },
}

/// Mailbox for resolver
#[derive(Clone)]
pub struct Handler<H: Hasher> {
    sender: mpsc::Sender<Message<H>>,
}

impl<H: Hasher> Handler<H> {
    pub(super) fn new(sender: mpsc::Sender<Message<H>>) -> Self {
        Self { sender }
    }
}

impl<H: Hasher> Consumer for Handler<H> {
    type Key = MultiIndex<H::Digest>;
    type Value = Bytes;
    type Failure = ();

    async fn deliver(&mut self, key: Self::Key, value: Self::Value) -> bool {
        let (response, receiver) = oneshot::channel();
        self.sender
            .send(Message::Deliver {
                key,
                value,
                response,
            })
            .await
            .expect("Failed to send deliver");
        receiver.await.expect("Failed to receive deliver")
    }

    async fn failed(&mut self, key: Self::Key, failture: Self::Failure) {
        warn!(?key, ?failture, "failed at consumer");
    }
}

impl<H: Hasher> Producer for Handler<H> {
    type Key = MultiIndex<H::Digest>;

    async fn produce(&mut self, key: Self::Key) -> oneshot::Receiver<Bytes> {
        let (response, receiver) = oneshot::channel();
        self.sender
            .send(Message::Produce { key, response })
            .await
            .expect("Failed to send produce");
        receiver
    }
}

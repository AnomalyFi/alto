use std::hash::Hash;

use commonware_utils::Array;
use commonware_cryptography::{Digest, Hasher};
use commonware_broadcast::{linked::Context, Application as A};
use futures::{ channel::{mpsc, oneshot}, SinkExt};

pub struct Payload {
    #[allow(dead_code)]
    data: Vec<u8>
}

pub enum Message<H: Hasher, P: Array> {
    Broadcast(H::Digest),
    Verify(Context<P>, H::Digest, oneshot::Sender<bool>),
}

#[derive(Clone)]
pub struct Mailbox<H: Hasher, P: Array> {
    sender: mpsc::Sender<Message<H, P>>,
}

impl<H: Hasher, P: Array> Mailbox<H, P> {
    pub(super) fn new(sender: mpsc::Sender<Message<H, P>>) -> Self {
        Self {
            sender
        }
    }

    pub async fn broadcast(&mut self, payload: H::Digest) {
        let _ = self.sender.send(Message::Broadcast(payload)).await;
    }
}

impl<H: Hasher, P: Array> A for Mailbox<H, P> {
    type Context = Context<P>;
    type Digest = H::Digest;

    async fn verify(
        &mut self,
        context: Self::Context,
        payload: Self::Digest,
    ) -> oneshot::Receiver<bool> {
        let (sender, receiver) = oneshot::channel();
        let _ = self
            .sender
            .send(Message::Verify(context, payload, sender))
            .await;
        receiver
    }
}
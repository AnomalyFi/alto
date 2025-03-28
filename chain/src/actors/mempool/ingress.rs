use commonware_utils::Array;
use commonware_cryptography::Digest;
use commonware_broadcast::{linked::Context, Application as A};
use futures::{ channel::{mpsc, oneshot}, SinkExt};

pub struct Payload {
    #[allow(dead_code)]
    data: Vec<u8>
}

pub enum Message<D: Digest, P: Array> {
    Broadcast(D),
    Verify(Context<P>, D, oneshot::Sender<bool>),
}

#[derive(Clone)]
pub struct Mailbox<D: Digest, P: Array> {
    sender: mpsc::Sender<Message<D, P>>,
}

impl<D: Digest, P: Array> Mailbox<D, P> {
    pub(super) fn new(sender: mpsc::Sender<Message<D, P>>) -> Self {
        Self {
            sender
        }
    }

    pub async fn broadcast(&mut self, payload: D) {
        let _ = self.sender.send(Message::Broadcast(payload)).await;
    }
}

impl<D: Digest, P: Array> A for Mailbox<D, P> {
    type Context = Context<P>;
    type Digest = D;

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
use super:: ingress::{Mailbox, Message};
use commonware_broadcast::{linked::Context, Application as A, Broadcaster};
use commonware_cryptography::Digest;
use commonware_utils::Array;
use futures::{
    channel::{mpsc, oneshot},
    SinkExt, StreamExt,
};
use tracing::error;


pub struct Actor<D: Digest, P: Array> {
    mailbox: mpsc::Receiver<Message<D, P>>,
    // TODO: add a mempool structure here
}

impl<D: Digest, P: Array> Actor<D, P> {
    pub fn new() -> (Self, Mailbox<D, P>) {
        let (sender, receiver) = mpsc::channel(1024);
        (Actor { mailbox: receiver }, Mailbox::new(sender))
    }

    pub async fn run(mut self, mut engine: impl Broadcaster<Digest = D>) {
        // it passes msgs in the mailbox of the actor to the engine mailbox
        while let Some(msg) = self.mailbox.next().await {
            match msg {
                Message::Broadcast(payload) => {
                    let receiver = engine.broadcast(payload).await;
                    let result = receiver.await;
                    match result {
                        Ok(true) => {}
                        Ok(false) => {
                            error!("broadcast returned false")
                        }
                        Err(_) => {
                            error!("broadcast dropped")
                        }
                    }
                }
                Message::Verify(_context, _payload, sender) => {
                    // TODO: add handler here to process batch received
                    let result = sender.send(true);
                    if result.is_err() {
                        error!("verify dropped");
                    }
                }
            }
        }
    }

    // TODO: implement handler for data received
}

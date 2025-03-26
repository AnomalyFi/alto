use super::{ ingress::{Mailbox, Message}, mempool};
use commonware_broadcast::Broadcaster;
use commonware_cryptography::Digest;
use commonware_utils::Array;
use futures::{
    channel::mpsc,
    StreamExt,
};
use tracing::{error, warn, debug};


pub struct Actor<D: Digest, P: Array> {
    mailbox: mpsc::Receiver<Message<D, P>>,
    // TODO: add a mempool structure here
}

impl<D: Digest, P: Array> Actor<D, P> {
    pub fn new() -> (Self, Mailbox<D, P>) {
        let (sender, receiver) = mpsc::channel(1024);
        (Actor { mailbox: receiver }, Mailbox::new(sender))
    }

    pub async fn run(mut self, 
        mut engine: impl Broadcaster<Digest = D>,
        mut mempool: mempool::Mailbox<D>
    ) {
        // it passes msgs in the mailbox of the actor to the engine mailbox
        while let Some(msg) = self.mailbox.next().await {
            match msg {
                Message::Broadcast(payload) => {
                    debug!("broadcasting batch {}", payload);
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
                    debug!(digest=?_payload, "incoming verification request");
                    let Some(_) = mempool.get_batch(_payload).await else {
                        warn!(?_payload, "batch not exists");
                        let _ = sender.send(false);
                        continue;
                    };
                    debug!("issue verfication to batch={}", _payload);
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

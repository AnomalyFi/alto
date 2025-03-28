use futures::{channel::mpsc, StreamExt};
use rand::Rng;
use commonware_runtime::{Clock, Handle, Metrics, Spawner};

use super::ingress::{Mailbox, Message};
use tracing::{debug};

pub struct Actor {
    mailbox: mpsc::Receiver<Message>
}

impl Actor {
    pub fn new() -> (Self, Mailbox) {
        let (sender, receiver) = mpsc::channel(1024);

        (
            Actor {
                mailbox: receiver,   
            },
            Mailbox::new(sender)
        )
    }

    pub async fn run(mut self) {
        while let Some(msg) = self.mailbox.next().await {
            match msg {
                Message::TestMsg { payload, response } => {
                    debug!(?payload, "received test message");
                    let _ = response.send(format!("echo: {}", payload));
                },
                _ => unimplemented!()
            } 
        }
    }
}
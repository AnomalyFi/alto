use bytes::Bytes;
use commonware_cryptography::sha256::Digest;
use futures::{channel::{mpsc, oneshot}, SinkExt};

pub enum Message {
    SubmitTx {
        payload: Bytes,
        response: oneshot::Sender<Digest>
    },
    TestMsg {
        payload: String,
        response: oneshot::Sender<String>
    }
}

#[derive(Clone)]
pub struct Mailbox {
    sender: mpsc::Sender<Message>
}

impl Mailbox {
    pub(super) fn new(sender: mpsc::Sender<Message>) -> Self {
        Self { sender }
    }

    pub async fn test(&mut self, payload: String) -> String {
        let (response, receiver) = oneshot::channel();

        self.sender
            .send(Message::TestMsg { payload, response: response })
            .await
            .expect("failed to send test msg");
        
        receiver.await.expect("failed to receive test msg")
    }
}
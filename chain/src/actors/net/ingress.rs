use std::vec;

use alto_types::Block;
use bytes::{Buf, BufMut, Bytes};
use commonware_cryptography::sha256::Digest;
use futures::{channel::{mpsc, oneshot}, stream::SelectNextSome, SinkExt};

// message from other services
#[derive(Debug)]
pub enum Message {
    PublishBlock {
        block: Block,
    },
}

impl Message {
    pub fn serialize(&self) -> Vec<u8> {
        match self {
            Self::PublishBlock { block } => {
                let raw_block = block.serialize();
                let mut raw = Vec::with_capacity(1 + raw_block.len());
                raw.push(0);
                raw.extend_from_slice(&raw_block);
                raw
            }
        }
    } 

    pub fn deserialize(raw: &[u8]) -> Result<Message, String> {
        if raw.is_empty() {
            return Err("Empty payload provided".into());
        }
        // The first byte indicates the message type.
        match raw[0] {
            0 => {
                let Some(block) = Block::deserialize(&raw[1..]) else {
                    return Err(format!("unable to deserialize into block"));
                };
                Ok(Message::PublishBlock { block })
            }
            other => Err(format!("Unsupported message type: {}", other)),
        }
    }
}

#[derive(Clone)]
pub struct Mailbox {
    sender: mpsc::Sender<Message>
}

impl Mailbox {
    pub fn new(sender: mpsc::Sender<Message>) -> Self {
        Self { sender }
    }

    pub async fn broadcast_block(&mut self, block: Block) {
        let _ = self.sender.send(Message::PublishBlock { block }).await;
    }
}
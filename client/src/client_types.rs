use bytes::{BufMut, Bytes};
use commonware_cryptography::sha256::Digest;
use futures::channel::oneshot;
use alto_types::Block;

/// Messages sent from client
pub enum ClientMessage {
    WSMessage(WebsocketClientMessage),
    RpcMessage(ClientRpcMessage)
}

/// Websocket Message sent from client
#[derive(Debug)]
#[repr(u8)]
pub enum WebsocketClientMessage {
    RegisterBlock = 0,
    RegisterTx,

    SubmitTxs(Vec<Bytes>)
}

impl WebsocketClientMessage {
    // TODO: cache the serialization result
    pub fn serialize(&self) -> Vec<u8> {
        match self {
            Self::RegisterBlock => vec![0],
            Self::RegisterTx => vec![1],
            Self::SubmitTxs(txs) => {
                let mut raw = vec![2]; // first byte indicates the message type
                raw.put_u64(txs.len() as u64);
                for tx in txs.into_iter() {
                    raw.put_u64(tx.len() as u64);
                    raw.put(tx.clone());
                }
                raw
            }
        }
    }

    pub fn deserialize(raw: &[u8]) -> Result<WebsocketClientMessage, String> {
        use bytes::Buf;

        if raw.is_empty() {
            return Err("empty payload provided".into());
        }

        // Create a mutable buffer view over the input slice
        let mut buf = raw;

        // Read the message type byte.
        let msg_type = buf.get_u8();
        match msg_type {
            0 => Ok(WebsocketClientMessage::RegisterBlock),
            1 => Ok(WebsocketClientMessage::RegisterTx),
            2 => {
                // Ensure there are enough bytes for the number of transactions.
                if buf.remaining() < 8 {
                    return Err("payload too short for number of transactions".into());
                }
                let num_txs = buf.get_u64();

                let mut txs = Vec::with_capacity(num_txs as usize);
                // Loop over each transaction.
                for _ in 0..num_txs {
                    if buf.remaining() < 8 {
                        return Err("payload too short for transaction length".into());
                    }
                    let tx_len = buf.get_u64() as usize;
                    if buf.remaining() < tx_len {
                        return Err("payload too short for transaction data".into());
                    }
                    // Extract the transaction bytes.
                    let tx_data = buf.copy_to_bytes(tx_len);
                    txs.push(tx_data);
                }
                Ok(WebsocketClientMessage::SubmitTxs(txs))
            }
            other => Err(format!("unsupported message type: {}", other)),
        }
    }
}

#[derive(Debug)]
pub enum ClientRpcMessage {
    // for rpc
    SubmitTx {
        payload: Bytes,
    },
    GetBlockHeight {
        response: oneshot::Sender<u64>
    },
    GetBlock {
        height: u64,
    },
}

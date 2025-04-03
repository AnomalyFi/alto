use std::error::Error;
use commonware_cryptography::Sha256;
use reqwest::{Client, Url};
use super::client_types::{ClientRpcMessageResp, ClientRpcMessage};
use bytes::Bytes;
use serde::Deserialize;
use alto_types::tx::{Tx};
pub const WEBSOCKET_PREFIX:  &'static str = "/ws";
pub const RPC_PREFIX: &'static str = "/api";
// TODO: Update the below endpoints when we know what they are
pub const PATH_SUBMIT_TX: &'static str = "/mempool/submit";
pub const PATH_GET_BLOCK: &'static str = "/api/get_block";
pub const PATH_GET_BLOCK_HEIGHT: &'static str = "/api/get_block_height";

#[derive(Debug)]
pub struct JSONRPCClient {
    http_client: Client,
    base_url: String,
    chain_id: String,
}

impl JSONRPCClient {
    pub fn new(mut uri: String, chain_id: String) -> Self {
        if uri.ends_with('/') {
            uri.pop();
        }
        let final_url = format!("{}/jsonrpc", uri);

        Self {
            http_client: Client::new(),
            base_url: final_url,
            chain_id,
        }
    }
    //todo implement methods needed to communicate with server
    pub async fn submit_tx(&self, mut tx: Tx<Sha256>) -> Result<ClientRpcMessageResp, Box<dyn Error>> {
        let encoded_tx_bytes = tx.encode();
        let mut submit_request = ClientRpcMessage::SubmitTx {
            payload: encoded_tx_bytes.into(),
        };

        let full_url = Url::parse(&self.base_url)
            .and_then(|base| base.join(PATH_SUBMIT_TX))
            .expect("Invalid base_url or path for submit tx");

            todo!()
        // self.send_request(full_url.to_string(), submit_request.into()).await
    }

    pub async fn get_block(&self, height: u64) -> Result<ClientRpcMessageResp, Box<dyn Error>> {
        let mut get_block_req = ClientRpcMessage::GetBlock {
            height
        };

        let full_url = Url::parse(&self.base_url)
            .and_then(|base| base.join(PATH_GET_BLOCK))
            .expect("Invalid base_url or path for get block");

            todo!()
        // self.send_request(full_url.to_string(), get_block_req.into()).await
    }

    pub async fn get_block_height(&self) -> Result<ClientRpcMessageResp, Box<dyn Error>> {
        let mut get_block_height_req = ClientRpcMessage::GetBlockHeight {};

        let full_url = Url::parse(&self.base_url)
            .and_then(|base| base.join(PATH_GET_BLOCK_HEIGHT))
            .expect("Invalid base_url or path for get block");

            todo!()
        // self.send_request(full_url.to_string(), get_block_height_req.into()).await
    }

    async fn send_request<Resp>(&self, uri: String, data: Vec<u8>) -> Result<Resp, Box<dyn Error>> {
        let resp = self.http_client.post(uri)
            .body(data)
            .send()
            .await?;
        todo!()
        // Ok(resp)
    }

}

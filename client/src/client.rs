use reqwest::Client;
use crate::client_types::ClientRpcMessage;
use bytes::Bytes;
use serde::Deserialize;
use alto_types::tx::Tx;
pub const WEBSOCKET_PREFIX:  &'static str = "/ws";
pub const RPC_PREFIX: &'static str = "/api";
pub const PATH_SUBMIT_TX: &'static str = "/mempool/submit";

//todo add more const once we have server

//todo define all structs needed from server

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
    pub fn submit_tx(&self, tx: Tx) -> Vec<u8> {
        //todo safety checks
        todo!()
    }

    pub fn get_block(&self, data: Vec<u8>) -> u64{
        todo!()
    }

    pub fn get_block_height(&self, data: Vec<u8>) {
        todo!()
    }

}

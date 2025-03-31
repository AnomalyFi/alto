use std::{
   collections::HashMap, io, ops::Deref, sync::{Arc, RwLock}
};
use alto_types::Block;
use axum::response::IntoResponse;
use axum::{
    routing::get,
    extract::{Path, State, ws::{
        WebSocket, WebSocketUpgrade, Message as WSMessage
    }},
};

use bytes::Bytes;
use commonware_cryptography::{sha256, Digest};
use commonware_runtime::{Clock, Handle, Metrics, Spawner};
use futures::{channel::{mpsc, oneshot}, lock::Mutex, SinkExt, StreamExt};
use rand::Rng;
use serde::Deserialize;
use tokio::net::TcpListener;
use tokio_tungstenite::tungstenite::client;
use tracing::{debug, event, warn, Level, error};

use super::ingress;

pub struct RouterConfig {
    pub port: i32,
    pub mailbox: ingress::Mailbox
}

#[derive(Deserialize)]
pub struct DummyTransaction {
    #[serde(with = "serde_bytes")]
    pub payload: Vec<u8>,
}

#[derive(Debug, Clone)]
pub enum RouterMessage {
    Block {
        block: Block
    }
}

pub struct Mailbox {
    sender: mpsc::Sender<RouterMessage>
}

impl Mailbox {
    pub async fn broadcast_block(&mut self, block: Block) {
        self.sender.send(RouterMessage::Block { block })
        .await
        .expect("failed to broadcast block")
    }
}

type ClientSender = mpsc::Sender<RouterMessage>;

type ClientID = String;
type Clients = Arc<RwLock<HashMap<ClientID, ClientSender>>>;

type SharedState<R: Rng + Spawner + Metrics + Clock> = Arc<RwLock<AppState<R>>>;

#[derive(Clone)]
struct AppState<R: Rng + Spawner + Metrics + Clock>   {
    context: R,
    mailbox: ingress::Mailbox,
    clients: Clients
}

pub struct Router<R: Rng + Spawner + Metrics + Clock> {
    context: R,
    port: i32,
    listener: Option<TcpListener>,
    pub router: Option<axum::Router>,
    is_active: bool,

    state: SharedState<R>
}

impl<R: Rng + Spawner + Metrics + Clock> Router<R> {
    pub const WEBSOCKET_PREFIX:  &'static str = "/ws";
    pub const RPC_PREFIX: &'static str = "/api";
    pub const PATH_SUBMIT_TX: &'static str = "/mempool/submit";

    pub fn new(context: R, cfg: RouterConfig) -> (Self, Mailbox) {
        if cfg.port == 0 {
            panic!("Invalid port number");
        }

        let state = AppState::<R> {
            context: context.with_label("app_state"),
            mailbox: cfg.mailbox,
            clients: Arc::new(RwLock::new(HashMap::new()))
        };
        let state = Arc::new(RwLock::new(state));
        let (sender, mut receiver) = mpsc::channel::<RouterMessage>(1024);

        let receiver_state = state.clone();

        context.with_label("receiver").spawn(async move |_| {
            println!("starting receiving mailbox messages");
            while let Some(msg) = receiver.next().await {
                println!("received message from mailbox: {:?}", msg);
                let guard = receiver_state.write().unwrap().clients.clone();
                let clients = guard.write().unwrap().clone();
                println!("clients connected: {:?}", clients.keys());
                for (client_id, mut sender)  in clients.into_iter() {
                    debug!(?client_id, ?msg, "broadcasting message to client");
                    print!("broadcasting message to client: {}", client_id);
                    let _ = sender.send(msg.clone()).await;
                }
                println!("finishing up broadcasting")
            }
        });


        let mut router = Router {
            context,
            port: cfg.port,
            listener: None,
            router: None,
            is_active: false,
            state
        };
        router.init_router();

        (
            router,
            Mailbox { sender }
        )
    }

    pub async fn start(mut self) -> Handle<()> {
        self.context.spawn_ref()(self.run())
    }

    pub fn stop(&self) {
        if !self.is_active {
            return
        }

        event!(Level::INFO, "stopped router service");
    }

    async fn init_listener(&mut self) -> io::Result<TcpListener> {
        let listener = TcpListener::bind(format!("127.0.0.1:{}", self.port)).await?;
        Ok(listener)
    }

    async fn ws_handler(
        ws: WebSocketUpgrade,
        State(state): State<SharedState<R>>,
    ) -> impl IntoResponse {
        let client_id = uuid::Uuid::new_v4().to_string();
        ws.on_upgrade(move |socket| Self::handle_socket(socket, client_id, state))
    }

    async fn handle_socket(mut socket: WebSocket, client_id: ClientID, state: SharedState<R>) {
        let (mut socket_sender, mut socket_receiver) = socket.split();

        let (tx, mut rx) = mpsc::channel::<RouterMessage>(1024);

        // Insert the sender into the shared state
        {
            let mut state = state.write().unwrap();
            state.clients.write().unwrap().insert(client_id.clone(), tx);
            print!("inserting client {}\n", client_id);

            state.context.with_label(format!("client-{}", client_id).deref()).spawn(async move |_| {
                println!("starting client rx listener");
                while let Some(msg) = rx.next().await {
                    print!("received message from client receiver chan: {:?}", msg);
                    match encode_router_message(msg) {
                        Ok(raw) => {
                            socket_sender.send(WSMessage::Binary(Bytes::from(raw))).await.unwrap();
                        },
                        Err(err) => {
                            warn!(?err, "received unsupported message");
                            print!("received unsupporated message: {}", err)
                        }
                    }
                }  
            });
        }


        while let Some(msg) = socket_receiver.next().await {
            match msg {
                Ok(WSMessage::Text(text)) => {
                    debug!(?text, "receiving text");
                }
                Ok(WSMessage::Binary(bin)) => {
                    match decode_router_message(bin.into()) {
                        Ok(msg) => {
                            debug!(?msg, "received msg from client")
                        },
                        Err(err) => {
                            // TODO: possibly terminate the connection as malicious message is sent?
                            error!(?err, "received unsupported message")
                        }
                    }
                }
                Ok(WSMessage::Close(_)) => break,
                _ => {}
            }
        }

        {
            let mut state = state.write().unwrap();
            state.clients.write().unwrap().remove(&client_id);
            print!("removing client {}\n", client_id);
        }
    }

    async fn handle_submit_tx(
        State(state): State<SharedState<R>>,
        payload: String,
    ) -> impl IntoResponse {
        let mut mailbox = state.write().unwrap().mailbox.clone();
        mailbox.test(payload).await
    }


    fn init_router(&mut self) {
        let router = axum::Router::new()
            .route(
                Self::PATH_SUBMIT_TX,
                get(Self::handle_submit_tx).with_state(Arc::clone(&self.state))
            )
            .route(
                Self::WEBSOCKET_PREFIX, 
                get(Self::ws_handler).with_state(Arc::clone(&self.state))
            );
        self.router = Some(router)
    }

    async fn serve(&mut self) -> Result<(), Box<dyn std::error::Error>> {
        let listener = self.listener.take().ok_or("serve failed because listener is None");
        let router = self.router.take().ok_or("serve failed because router is None");
        axum::serve(listener.unwrap(), router.unwrap()).await?;
        Ok(())
    }

    async fn run(mut self) {
        event!(Level::INFO, "starting router service");

        println!("init listener");
        let listener_res = self.init_listener();
        match listener_res.await {
            Ok(value) => self.listener = Some(value),
            Err(error) => {
                println!("Error during listener init: {}", error);
                return
            },
        }

        println!("init router & serve");
        self.init_router();
        self.serve().await.unwrap();
        self.is_active = true;

        event!(Level::INFO, "server stopping...");

    }
}

pub enum RouterMessageType {
    Block = 1,
}

impl TryFrom<u8> for RouterMessageType {
    type Error = ();
    fn try_from(value: u8) -> Result<Self, Self::Error> {
        match value {
            x if x == RouterMessageType::Block as u8 => Ok(RouterMessageType::Block),
            _ => Err(())
        } 
    }
}

pub fn encode_router_message(msg: RouterMessage) -> Result<Vec<u8>, String> {
    match msg {
        RouterMessage::Block { block } => {
            let mut raw = block.serialize();
            raw.insert(0, RouterMessageType::Block as u8);
            Ok(raw)
        }
    }
}

pub fn decode_router_message(raw: Vec<u8>) -> Result<RouterMessage, String> {
    if raw.len() == 0 {
        return Err(format!("zero len raw message provided"))
    }

    let msg_type = RouterMessageType::try_from(raw[0]).unwrap();
    match msg_type {
        RouterMessageType::Block => {
            let Some(block) = Block::deserialize(&raw[1..]) else {
                return Err(format!("unable to deserialize block"))
            };

            Ok(RouterMessage::Block { block })
        }
    }
}

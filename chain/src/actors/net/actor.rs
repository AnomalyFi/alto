use std::{
   collections::{HashMap, HashSet}, io, ops::Deref, sync::{Arc, RwLock}
};
use alto_client::Client;
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
use tracing::{debug, event, Level, error};
use tracing_subscriber::fmt::format;

use super::ingress::{Mailbox, Message, WebsocketClientMessage};

#[derive(Deserialize)]
pub struct DummyTransaction {
    #[serde(with = "serde_bytes")]
    pub payload: Vec<u8>,
}

type ClientSender = mpsc::Sender<Arc<Message>>;
type ClientID = String;
type Clients = Arc<RwLock<HashMap<ClientID, ClientSender>>>;

type SharedState<R: Rng + Spawner + Metrics + Clock> = Arc<RwLock<AppState<R>>>;

#[derive(Clone)]
struct AppState<R: Rng + Spawner + Metrics + Clock>   {
    context: R,
    mailbox: Mailbox,
    clients: Clients,
    block_listeners: Arc<RwLock<HashSet<ClientID>>>,
    tx_listeners: Arc<RwLock<HashSet<ClientID>>>
}

pub struct Config {
    pub port: i32,
}

pub struct Actor<R: Rng + Spawner + Metrics + Clock> {
    context: R,
    port: i32,
    listener: Option<TcpListener>,
    pub router: Option<axum::Router>,
    is_active: bool,

    state: SharedState<R>
}

impl<R: Rng + Spawner + Metrics + Clock> Actor<R> {
    pub const WEBSOCKET_PREFIX:  &'static str = "/ws";
    pub const RPC_PREFIX: &'static str = "/api";
    pub const PATH_SUBMIT_TX: &'static str = "/mempool/submit";

    pub fn new(context: R, cfg: Config) -> (Self, Mailbox) {
        if cfg.port == 0 {
            panic!("Invalid port number");
        }

        let (sender, mut receiver) = mpsc::channel(1024);
        let mailbox = Mailbox::new(sender);

        let state = AppState::<R> {
            context: context.with_label("app_state"),
            mailbox: mailbox.clone(),
            clients: Arc::new(RwLock::new(HashMap::new())),
            block_listeners: Arc::new(RwLock::new(HashSet::new())),
            tx_listeners: Arc::new(RwLock::new(HashSet::new())),
        };
        let state = Arc::new(RwLock::new(state));
        let receiver_state = state.clone();

        context.with_label("receiver").spawn(async move |_| {
            println!("starting receiving mailbox messages");
            while let Some(msg) = receiver.next().await {
                Self::handle_message(receiver_state.clone(), msg).await;
            }
        });


        let mut router = Actor {
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
            mailbox
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

    /// handles messages from other services within a node such as block messages 
    async fn handle_message(state: SharedState<R>, msg: Message) {
        println!("handling msg {:?}", msg);
        let block_listeners = state.read().unwrap().block_listeners.clone();
        let clients = state.read().unwrap().clients.clone();

        match msg {
            Message::PublishBlock { block } => {
                let msg = Arc::new(Message::PublishBlock { block });
                let listeners: Vec<_> = {
                    block_listeners.read().unwrap().iter().cloned().collect()
                };

                for listener in listeners {
                    if let Some(tx) = {
                        let guard = clients.read().unwrap();
                        guard.get(&listener).cloned()
                    } {
                        let _ = tx.clone().send(msg.clone()).await;
                    }
                }
            }
        }
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

        let (tx, mut rx) = mpsc::channel::<Arc<Message>>(1024);

        // Insert the sender into the shared state
        {
            let state = state.write().unwrap();
            state.clients.write().unwrap().insert(client_id.clone(), tx);
            print!("inserting client {}\n", client_id);

            state.context.with_label(format!("client-{}", client_id).deref()).spawn(async move |_| {
                println!("starting client rx listener");
                while let Some(msg) = rx.next().await {
                    print!("received message from client receiver chan: {:?}", msg);
                    let raw = msg.serialize();
                    socket_sender.send(WSMessage::Binary(Bytes::from(raw))).await.unwrap();
                }  
            });
        }


        while let Some(msg) = socket_receiver.next().await {
            match msg {
                Ok(WSMessage::Text(text)) => {
                    debug!(?text, "receiving text");
                }
                Ok(WSMessage::Binary(bin)) => {
                    match WebsocketClientMessage::deserialize(bin.deref()) {
                        Ok(msg) => {
                            debug!(?msg, "received msg from client");
                            let state = state.write().unwrap();
                            match msg {
                                WebsocketClientMessage::RegisterBlock => {
                                    println!("adding block listener {}", client_id);
                                    state.block_listeners.write().unwrap().insert(client_id.clone());
                                }, 
                                WebsocketClientMessage::RegisterTx => {
                                    state.tx_listeners.write().unwrap().insert(client_id.clone());
                                }, 
                                WebsocketClientMessage::SubmitTxs(txs) => {
                                    unimplemented!()
                                }
                            }
                        },
                        Err(err) => {
                            // TODO: possibly terminate the connection as malicious message is sent?
                            error!(?err, "received unsupported message")
                        }
                    }
                }
                Ok(WSMessage::Close(_)) => {
                    let state = state.write().unwrap();
                    state.clients.write().unwrap().remove(&client_id);
                    state.block_listeners.write().unwrap().remove(&client_id);
                    state.tx_listeners.write().unwrap().remove(&client_id);
                    break;
                }
                _ => {}
            }
        }

        {
            let state = state.write().unwrap();
            state.clients.write().unwrap().remove(&client_id);
            print!("removing client {}\n", client_id);
        }
    }

    async fn handle_submit_tx(
        State(state): State<SharedState<R>>,
        payload: Bytes,
    ) -> impl IntoResponse {
        // TODO: send to mempool mailbox
        format!("submitted")
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
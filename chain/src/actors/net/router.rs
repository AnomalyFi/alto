use std::{
   io,
   sync::{Arc, RwLock},
};
use axum::response::IntoResponse;
use axum::{
    routing::get,
    extract::{Path, State},
};

use bytes::Bytes;
use commonware_cryptography::{sha256, Digest};
use commonware_runtime::{Clock, Handle, Metrics, Spawner};
use rand::Rng;
use serde::Deserialize;
use tokio::net::TcpListener;
use tracing::{event, Level};

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

type SharedState = Arc<RwLock<AppState>>;

#[derive(Clone)]
struct AppState  {
    mailbox: ingress::Mailbox
}

pub struct Router<R: Rng + Spawner + Metrics + Clock> {
    context: R,
    port: i32,
    listener: Option<TcpListener>,
    pub router: Option<axum::Router>,
    is_active: bool,

    state: SharedState
}

impl<R: Rng + Spawner + Metrics + Clock> Router<R> {
    pub const PATH_SUBMIT_TX: &'static str = "/mempool/submit";

    pub fn new(context: R, cfg: RouterConfig) -> Self {
        if cfg.port == 0 {
            panic!("Invalid port number");
        }

        let state = AppState {
            mailbox: cfg.mailbox,
        };

        let mut router = Router {
            context,
            port: cfg.port,
            listener: None,
            router: None,
            is_active: false,

            state: Arc::new(RwLock::new(state))
        };
        router.init_router();

        router
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

    async fn handle_default() -> impl IntoResponse {
        "Hello world!"
    }

    async fn handle_submit_tx(
        State(state): State<SharedState>,
        payload: String,
    ) -> impl IntoResponse {
        let mut mailbox = state.write().unwrap().mailbox.clone();
        mailbox.test(payload).await
    }


    fn init_router(&mut self) {
        let router = axum::Router::new()
            .route(
                Router::<R>::PATH_SUBMIT_TX,
                get(Router::<R>::handle_submit_tx).with_state(Arc::clone(&self.state))
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

        let listener_res = self.init_listener();
        match listener_res.await {
            Ok(value) => self.listener = Some(value),
            Err(error) => {
                println!("Error during listener init: {}", error);
                return
            },
        }

        self.init_router();
        self.serve().await.unwrap();
        self.is_active = true;

        event!(Level::INFO, "finished starting router service");
    }
}

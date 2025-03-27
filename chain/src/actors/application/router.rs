use std::io;
use axum::response::IntoResponse;
use axum::routing::get;
use commonware_runtime::{Clock, Handle, Metrics, Spawner};
use rand::Rng;
use tokio::net::TcpListener;
use tracing::{event, Level};
use crate::actors::syncer;

pub struct RouterConfig {
    port: i32,
}

impl RouterConfig {
    pub fn default_config() -> RouterConfig {
        RouterConfig {
            port: 7844,
        }
    }
}

pub struct Router<R: Rng + Spawner + Metrics + Clock> {
    context: R,
    cfg: RouterConfig,
    listener: Option<TcpListener>,
    router: Option<axum::Router>,
    is_active: bool,
}

impl<R: Rng + Spawner + Metrics + Clock> Router<R> {
    const PATH_SUBMIT_BLOCK: &'static str = "/builder/submit";

    pub fn new(context: R, cfg: RouterConfig) -> Self {
        if cfg.port == 0 {
            panic!("Invalid port number");
        }

        Router {
            context,
            cfg,
            listener: None,
            router: None,
            is_active: false,
        }
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
        let listener = TcpListener::bind(format!("127.0.0.1:{}", self.cfg.port)).await?;
        Ok(listener)
    }

    async fn handle_default() -> impl IntoResponse {
        "Hello world!"
    }

    async fn handle_submit_block() -> impl IntoResponse {
        "Submit block"
    }

    fn init_router(&mut self) {
        let router = axum::Router::new()
            .route("/", get(Router::handle_default))
            .route(Router::PATH_SUBMIT_BLOCK, get(Router::handle_submit_block()));

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

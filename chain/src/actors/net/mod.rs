pub mod router;
pub mod ingress;
pub mod actor;

#[cfg(test)]
mod tests {
    use core::panic;
    use std::time::Duration;
    use alto_types::Block;
    use axum::{
        body::Body,
        http::{Request, StatusCode}
    };
    use commonware_cryptography::sha256;
    use commonware_macros::{test_async, test_traced};
    use commonware_runtime::{tokio::{self, Context, Executor}, Clock, Metrics, Runner, Spawner};
    use futures::{channel::mpsc, future::join_all, SinkExt, StreamExt};
    use tokio_tungstenite::{connect_async, tungstenite::Message as WsClientMessage};
    use tower::ServiceExt;
    use tracing_subscriber::field::debug;

    use crate::actors::net::router::decode_router_message;

    use super::{actor::Actor, ingress::Mailbox, router::{self, RouterMessage}};
    use tracing::debug;

    #[test_traced]
    fn test_msg() {
        let (runner, mut context) = Executor::init(tokio::Config::default());
        runner.start(async move {
            let (actor, mailbox) = Actor::new();

            context.with_label("net_actor").spawn(|_| {
                actor.run()
            });

            let (app, _) = router::Router::new(
                context.with_label("net_router"), 
                router::RouterConfig {
                    port: 7890,
                    mailbox
            });

            let Some(router) = app.router else {
                panic!("router not initalized");
            };

            // Construct a GET request.
            // Note: the handler expects a payload (a String). Since GET requests normally have no body,
            // you might decide to pass the payload as a query parameter or in the body if that's what you intend.
            // Here, we'll assume the payload is extracted from the request body.
            let payload = "test payload";
            let request = Request::builder()
                .method("GET")
                .uri("/mempool/submit")
                .body(Body::from(payload))
                .unwrap();

            // Send the request to the app.
            let response = router.oneshot(request).await.unwrap();

            // Check that the response status is OK.
            assert_eq!(response.status(), StatusCode::OK);
        })
    }
    
    #[test_traced]
    fn test_ws() {
        let (runner, mut context) = Executor::default();
        runner.start(async move {
            let (actor, actor_mailbox) = Actor::new();

            context.with_label("net_actor").spawn(|_| {
                actor.run()
            });

            let (app, mut router_mailbox) = router::Router::new(
                context.with_label("net_router"), 
                router::RouterConfig {
                    port: 7890,
                    mailbox: actor_mailbox
            });

            debug!("starting router");
            let app_handler = app.start().await;

            debug!("launching ws client");
            // instantiate websocket client listening block
            let url = format!("ws://127.0.0.1:7890/ws");
            let (ws_stream, response) = connect_async(url).await.expect("Failed to connect");
            assert_eq!(response.status(), StatusCode::SWITCHING_PROTOCOLS);

            let (_, mut read) = ws_stream.split();
            let client_handler = context.with_label("ws_client").spawn(async move |_| {
                while let Ok(msg) =  read.next().await.unwrap() {
                    match msg {
                        WsClientMessage::Binary(bin) => {
                            match decode_router_message(bin.into()).unwrap() {
                                RouterMessage::Block { block } => {
                                    debug!(?block, "received a block from server");
                                    return;
                                } }
                        } ,
                        _ => {
                            debug!("unknown message")
                        }
                    } 
                };
            });

            // send a dummy block
            debug!("mock sending dummy block from another service");
            let parent_digest = sha256::hash(&[0; 32]);
            let height = 0;
            let timestamp = 1;
            let block = Block::new(parent_digest, height, timestamp);
            router_mailbox.broadcast_block(block).await;

            context.sleep(Duration::from_millis(1000)).await;

            // join_all(vec![app_handler]).await;
        })
    }
}
pub mod actor;
pub mod ingress;

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
    use tokio_tungstenite::{connect_async, tungstenite::{client, Message as WsClientMessage}};
    use tower::ServiceExt;
    use tracing_subscriber::field::debug;

    use crate::actors::net::ingress::WebsocketClientMessage;

    use super::{actor::Actor, ingress::Message, actor::{self}};
    use tracing::debug;

    #[test_traced]
    fn test_msg() {
        let (runner, mut context) = Executor::init(tokio::Config::default());
        runner.start(async move {
            let (actor, mailbox) = Actor::new(context, actor::Config {
                port: 7890
            });

            let Some(router) = actor.router else {
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
            let (actor, mut mailbox) = Actor::new(context.with_label("router"), actor::Config {
                port: 7890
            });

            debug!("starting router");
            let app_handler = actor.start().await;

            debug!("launching ws client");
            // instantiate websocket client listening block
            let url = format!("ws://127.0.0.1:7890/ws");
            let (ws_stream, response) = connect_async(url).await.expect("Failed to connect");
            assert_eq!(response.status(), StatusCode::SWITCHING_PROTOCOLS);

            let (mut write, mut read) = ws_stream.split();
            // register block
            let _ = write.send(WsClientMessage::binary(WebsocketClientMessage::RegisterBlock.serialize())).await;

            // listening block
            let client_handler = context.with_label("ws_client").spawn(async move |_| {
                while let Ok(msg) =  read.next().await.unwrap() {
                    match msg {
                        WsClientMessage::Binary(bin) => {
                            let msg = Message::deserialize(&bin).unwrap();
                            match msg {
                                Message::PublishBlock { block } => {
                                    print!("received a block from server: {:?}", block);
                                    return;
                                }
                            }
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
            mailbox.broadcast_block(block).await;

            context.sleep(Duration::from_millis(1000)).await;

            join_all(vec![client_handler]).await;
        })
    }
}
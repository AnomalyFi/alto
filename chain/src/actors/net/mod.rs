pub use ingress::{Mailbox, Message};
pub use actor::{Actor, Config};

pub mod actor;
pub mod ingress;

#[cfg(test)]
mod tests {
    use core::panic;
    use std::{ops::Deref, str, time::Duration};
    use alto_types::{signed_tx::SignedTx, Block};
    use axum::{
        body::{to_bytes, Body},
        http::{Request, StatusCode}, Router
    };
    use commonware_cryptography::{sha256, Sha256};
    use commonware_macros::{test_async, test_traced};
    use commonware_runtime::{tokio::{self, Context, Executor}, Clock, Handle, Metrics, Runner, Spawner};
    use futures::{channel::mpsc, future::{join_all, try_join_all}, SinkExt, StreamExt};
    use tokio_tungstenite::{connect_async, tungstenite::{client, Message as WsClientMessage}};
    use tower::{ServiceExt};
    use alto_client::client_types::{WebsocketClientMessage};

    use crate::actors::{mempool::mempool};
    

    use super::{actor::Actor, ingress::Message, actor::{self}};
    use tracing::debug;

    fn spawn_mempool(context: Context) -> (Handle<()>, Router)  {
        let (mempool_sender, mut mempool_receiver) = mpsc::channel(1024);
        let mempool_mailbox: mempool::Mailbox<Sha256> = mempool::Mailbox::new(mempool_sender);
        let (actor, _) = Actor::new(context.with_label("router"), actor::Config {
            port: 7890,
            mempool: mempool_mailbox
        });

        let Some(router) = actor.router else {
            panic!("router not initalized");
        };

        let handler = context.with_label("mock_mempool").spawn(async move |_| {
            while let Some(msg) = mempool_receiver.next().await {
                match msg {
                    mempool::Message::SubmitTxs { payload, response } => {
                        print!("received txs from rpc: {:?}", payload);
                        let _  = response.send(vec![true; payload.len()]);
                        return;
                    },
                    _ => unreachable!()
                }
            }
        });

        (handler, router)
    }

    #[test_traced]
    fn test_submit_tx() {
        let (runner, context) = Executor::init(tokio::Config::default());
        runner.start(async move {
            let (mempool_handler, router) = spawn_mempool(context);
            // Construct a GET request.
            // Note: the handler expects a payload (a String). Since GET requests normally have no body,
            // you might decide to pass the payload as a query parameter or in the body if that's what you intend.
            // Here, we'll assume the payload is extracted from the request body.
            let tx = SignedTx::<Sha256>::random();
            let payload = tx.payload();
            let request = Request::builder()
                .method("GET")
                .uri("/mempool/submit")
                .body(Body::from(payload))
                .unwrap();

            // Send the request to the app.
            let response = router.oneshot(request).await.unwrap();

            // Check that the response status is OK.
            assert_eq!(response.status(), StatusCode::OK);
            let _ = try_join_all(vec![mempool_handler]).await;
        })
    }

    #[test_traced]
    fn test_submit_tx_wrong_format() {
        let (runner, context) = Executor::init(tokio::Config::default());
        runner.start(async move {
            let (mempool_handler, router) = spawn_mempool(context);
            // Construct a GET request.
            // Note: the handler expects a payload (a String). Since GET requests normally have no body,
            // you might decide to pass the payload as a query parameter or in the body if that's what you intend.
            // Here, we'll assume the payload is extracted from the request body.
            let tx = b"test-tx";
            let request = Request::builder()
                .method("GET")
                .uri("/mempool/submit")
                .body(Body::from(tx.to_vec()))
                .unwrap();

            // Send the request to the app.
            let response = router.oneshot(request).await.unwrap();

            // Check that the response status is OK.
            assert_eq!(response.status(), StatusCode::OK);
            let body = response.into_body();
            let body = to_bytes(body, 2*1024*1024).await.unwrap();
            let result = String::from_utf8(body.to_vec()).unwrap();
            print!("submission result {}\n", result);

            assert!(result.contains("failed to submit tx"));

            let _ = try_join_all(vec![mempool_handler]).await;
        })
    }

    
    #[test_traced]
    fn test_ws() {
        let (runner, mut context) = Executor::default();
        runner.start(async move {
            let (mempool_sender, mempool_receiver) = mpsc::channel(1024);
            let mempool_mailbox: mempool::Mailbox<Sha256> = mempool::Mailbox::new(mempool_sender);
            let (actor, mut mailbox) = Actor::new(context.with_label("router"), actor::Config {
                port: 7890,
                mempool: mempool_mailbox
            });

            println!("starting router");
            let app_handler = actor.start();

            println!("launching ws client");
            // instantiate websocket client listening block
            let url = format!("ws://127.0.0.1:7890/ws");
            let (ws_stream, response) = connect_async(url).await.expect("Failed to connect");
            assert_eq!(response.status(), StatusCode::SWITCHING_PROTOCOLS);

            let (mut write, mut read) = ws_stream.split();
            // register block
            let _ = write.send(WsClientMessage::binary(WebsocketClientMessage::<Sha256>::RegisterBlock.serialize())).await;

            // listening block
            let client_handler = context.with_label("ws_client").spawn(async move |_| {
                while let Ok(msg) =  read.next().await.unwrap() {
                    match msg {
                        WsClientMessage::Binary(bin) => {
                            let msg = Message::deserialize(&bin).unwrap();
                            match msg {
                                Message::PublishBlock { block } => {
                                    println!("received a block from server: {:?}", block);
                                    return;
                                }
                            }
                        },
                        _ => {
                            debug!("unknown message")
                        }
                    } 
                };
            });

            // send a dummy block
            println!("mock sending dummy block from another service");
            let parent_digest = sha256::hash(&[0; 32]);
            let height = 0;
            let timestamp = 1;
            let block = Block::new(parent_digest, height, timestamp, vec![], sha256::hash(&[0; 32]));
            mailbox.broadcast_block(block).await;

            context.sleep(Duration::from_millis(1000)).await;

            join_all(vec![client_handler]).await;
        })
    }
}
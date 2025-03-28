pub mod router;
pub mod ingress;
pub mod actor;

#[cfg(test)]
mod tests {
    use core::panic;
    use std::time::Duration;
    use axum::{
        body::Body,
        http::{Request, StatusCode}
    };
    use commonware_macros::test_traced;
    use commonware_runtime::{deterministic::{Context, Executor}, Clock, Metrics, Runner, Spawner};
    use futures::channel::mpsc;
    use tower::ServiceExt;

    use super::{actor::Actor, ingress::Mailbox, router};

    #[test_traced]
    fn test_msg() {
        let (runner, mut context, _) = Executor::timed(Duration::from_secs(30));
        runner.start(async move {
            let (actor, mailbox) = Actor::new();

            context.with_label("net_actor").spawn(|_| {
                actor.run()
            });

            let app = router::Router::new(
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
}
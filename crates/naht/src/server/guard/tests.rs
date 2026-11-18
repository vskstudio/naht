use std::io;
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::Duration;

use axum::Router;
use axum::body::{Body, to_bytes};
use axum::http::{Request, StatusCode};
use axum::middleware::from_fn_with_state;
use axum::response::Response;
use axum::routing::post;
use tokio::task::JoinHandle;
use tokio::time::{sleep, timeout};
use tokio_util::task::TaskTracker;
use tower::ServiceExt;

use super::{POST_BODY_SLOTS, PostGate, body_error_status, limit_post_bodies};

fn slow_post_app(in_flight: &Arc<AtomicUsize>, peak: &Arc<AtomicUsize>) -> Router {
    let counter = Arc::clone(in_flight);
    let high_water = Arc::clone(peak);
    Router::new()
        .route(
            "/changes",
            post(move || {
                let counter = Arc::clone(&counter);
                let high_water = Arc::clone(&high_water);
                async move {
                    let now = counter.fetch_add(1, Ordering::SeqCst).saturating_add(1);
                    high_water.fetch_max(now, Ordering::SeqCst);
                    sleep(Duration::from_millis(200)).await;
                    counter.fetch_sub(1, Ordering::SeqCst);
                    StatusCode::OK
                }
            }),
        )
        .layer(from_fn_with_state(
            Arc::new(PostGate::new(
                Duration::from_secs(30),
                TaskTracker::new(),
                64 * 1024 * 1024,
            )),
            limit_post_bodies,
        ))
}

fn spawn_post(app: Router) -> JoinHandle<Response> {
    tokio::spawn(async move {
        let request = Request::post("/changes")
            .body(Body::from("batch"))
            .expect("valid request");
        app.oneshot(request).await.expect("infallible service")
    })
}

async fn wait_until_running(in_flight: &AtomicUsize, expected: usize) {
    timeout(Duration::from_secs(5), async {
        while in_flight.load(Ordering::SeqCst) < expected {
            sleep(Duration::from_millis(5)).await;
        }
    })
    .await
    .expect("handlers started");
}

#[tokio::test]
async fn at_most_two_post_handlers_run_at_once() {
    let in_flight = Arc::new(AtomicUsize::new(0));
    let peak = Arc::new(AtomicUsize::new(0));
    let app = slow_post_app(&in_flight, &peak);

    let calls: Vec<_> = (0..4).map(|_| spawn_post(app.clone())).collect();

    for call in calls {
        let response = call.await.expect("task finished");
        assert_eq!(response.status(), StatusCode::OK);
    }
    assert_eq!(peak.load(Ordering::SeqCst), POST_BODY_SLOTS);
}

#[tokio::test]
async fn a_dropped_request_keeps_its_slot_until_its_handler_returns() {
    let in_flight = Arc::new(AtomicUsize::new(0));
    let peak = Arc::new(AtomicUsize::new(0));
    let app = slow_post_app(&in_flight, &peak);

    let abandoned: Vec<_> = (0..POST_BODY_SLOTS)
        .map(|_| spawn_post(app.clone()))
        .collect();
    wait_until_running(&in_flight, POST_BODY_SLOTS).await;
    for call in &abandoned {
        call.abort();
    }

    let replacements: Vec<_> = (0..POST_BODY_SLOTS)
        .map(|_| spawn_post(app.clone()))
        .collect();
    for call in replacements {
        let response = call.await.expect("task finished");
        assert_eq!(response.status(), StatusCode::OK);
    }
    assert_eq!(peak.load(Ordering::SeqCst), POST_BODY_SLOTS);
}

#[tokio::test]
async fn only_the_length_limit_is_413_and_a_truncated_body_is_400() {
    let over_limit = to_bytes(Body::from(vec![0_u8; 8]), 4)
        .await
        .expect_err("body over the limit");
    assert_eq!(
        body_error_status(&over_limit),
        StatusCode::PAYLOAD_TOO_LARGE
    );

    let truncated = axum::Error::new(io::Error::new(
        io::ErrorKind::UnexpectedEof,
        "connection closed",
    ));
    assert_eq!(body_error_status(&truncated), StatusCode::BAD_REQUEST);
}

use super::*;
use axum::{
    Router,
    body::{Body, to_bytes},
    http::Request,
};
use base64::{Engine, prelude::BASE64_STANDARD};
use image::{DynamicImage, ImageFormat};
use serde_json::Value;
use std::{
    io::Cursor,
    sync::atomic::{AtomicBool, AtomicUsize, Ordering},
};
use tempfile::TempDir;
use tower::ServiceExt;

#[derive(Default)]
struct FakeGithub {
    posts: AtomicUsize,
    gets: AtomicUsize,
    lose_response: AtomicBool,
    hide_issue: AtomicBool,
    reports: Mutex<Vec<Value>>,
}

async fn github_stub() -> (String, Arc<FakeGithub>, tokio::task::JoinHandle<()>) {
    let state = Arc::new(FakeGithub::default());
    let app = Router::new().route("/issues", post(|State(state): State<Arc<FakeGithub>>, Json(report): Json<Value>| async move {
        state.posts.fetch_add(1, Ordering::SeqCst);
        state.reports.lock().unwrap().push(report.clone());
        if state.lose_response.load(Ordering::SeqCst) {
            // GitHub accepted the issue; the response body was lost in transit.
            return (StatusCode::CREATED, "{truncated").into_response();
        }
        (StatusCode::CREATED, Json(json!({"number": 1234, "html_url": "https://github.com/Bootstrap-Academy/Bootstrap-Academy/issues/1234", "body": report["body"]}))).into_response()
    }).get(|State(state): State<Arc<FakeGithub>>| async move {
        state.gets.fetch_add(1, Ordering::SeqCst);
        if state.hide_issue.load(Ordering::SeqCst) { return Json(json!([])); }
        Json(Value::Array(state.reports.lock().unwrap().iter().map(|report| json!({"number":1234,"html_url":"https://github.com/Bootstrap-Academy/Bootstrap-Academy/issues/1234","body":report["body"],"state":"closed"})).collect()))
    })).with_state(Arc::clone(&state));
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let endpoint = format!("http://{}/issues", listener.local_addr().unwrap());
    let task = tokio::spawn(async move {
        axum::serve(listener, app).await.unwrap();
    });
    (endpoint, state, task)
}

fn service(temp: &TempDir, endpoint: &str) -> Arc<Service> {
    Arc::new(Service {
        store: Arc::new(Mutex::new(Store::open(temp.path(), now()).unwrap())),
        github: Github::test_client(endpoint),
        public_base_url: "https://api.example.org".into(),
        admission: Arc::new(Semaphore::new(2)),
        submission: tokio::sync::Mutex::new(()),
        throttle: Mutex::default(),
        reconciled_at: Mutex::default(),
    })
}

fn report() -> Value {
    json!({"request_id":Uuid::new_v4(),"kind":"bug","title":"Ein echter Testbericht","description":"Der Test enthält nur gewählten Freitext.","diagnostics_consent":false})
}

fn diagnostics() -> Value {
    json!({"app_build":"git-abcd123","browser":"Firefox 130.0","os":"Linux","viewport":"1920x1080","language":"de-DE","theme":"dark","reduced_motion":false,"area":"learning"})
}

async fn send(service: &Arc<Service>, payload: Value) -> (StatusCode, Value) {
    let request = Request::post("/feedback")
        .header("Content-Type", "application/json")
        .extension(ClientIp("192.0.2.1".parse().unwrap()))
        .body(Body::from(serde_json::to_vec(&payload).unwrap()))
        .unwrap();
    let response = service_router(Arc::clone(service))
        .finish_api(&mut Default::default())
        .oneshot(request)
        .await
        .unwrap();
    let status = response.status();
    let bytes = to_bytes(response.into_body(), MAX_BODY_BYTES)
        .await
        .unwrap();
    (status, serde_json::from_slice(&bytes).unwrap())
}

#[tokio::test]
async fn guest_text_only_is_public_without_hidden_diagnostics_and_retries_are_idempotent() {
    let (endpoint, github, task) = github_stub().await;
    let temp = TempDir::new().unwrap();
    let service = service(&temp, &endpoint);
    let payload = report();
    let (status, result) = send(&service, payload.clone()).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(
        result["issue_url"],
        "https://github.com/Bootstrap-Academy/Bootstrap-Academy/issues/1234"
    );
    assert_eq!(send(&service, payload).await.1, result);
    assert_eq!(github.posts.load(Ordering::SeqCst), 1);
    let reports = github.reports.lock().unwrap();
    assert_eq!(reports[0].as_object().unwrap().len(), 2);
    let body = reports[0]["body"].as_str().unwrap();
    for secret in [
        "192.0.2.1",
        "diagnostics",
        "app_build",
        "browser",
        "account",
        "cookie",
    ] {
        assert!(!body.contains(secret));
    }
    assert!(body.contains("Der Test enthält nur gewählten Freitext."));
    task.abort();
}

#[tokio::test]
async fn consent_unknown_fields_and_uuid_conflicts_cannot_publish_more_issues() {
    let (endpoint, github, task) = github_stub().await;
    let temp = TempDir::new().unwrap();
    let service = service(&temp, &endpoint);
    let mut no_consent = report();
    no_consent["diagnostics"] = diagnostics();
    assert_eq!(
        send(&service, no_consent).await.0,
        StatusCode::UNPROCESSABLE_ENTITY
    );
    let mut missing = report();
    missing["diagnostics_consent"] = json!(true);
    assert_eq!(
        send(&service, missing).await.0,
        StatusCode::UNPROCESSABLE_ENTITY
    );
    let mut hidden = report();
    hidden["user_id"] = json!("secret");
    assert_eq!(
        send(&service, hidden).await.0,
        StatusCode::UNPROCESSABLE_ENTITY
    );
    let mut extra = report();
    extra["diagnostics_consent"] = json!(true);
    extra["diagnostics"] = diagnostics();
    extra["diagnostics"]["cookies"] = json!("secret");
    assert_eq!(
        send(&service, extra).await.0,
        StatusCode::UNPROCESSABLE_ENTITY
    );
    let mut raw_route = report();
    raw_route["diagnostics_consent"] = json!(true);
    raw_route["diagnostics"] = diagnostics();
    raw_route["diagnostics"]["area"] = json!("/users/alice?token=secret");
    assert_eq!(
        send(&service, raw_route).await.0,
        StatusCode::UNPROCESSABLE_ENTITY
    );
    assert_eq!(github.posts.load(Ordering::SeqCst), 0);
    let mut valid = report();
    valid["diagnostics_consent"] = json!(true);
    valid["diagnostics"] = diagnostics();
    assert_eq!(send(&service, valid.clone()).await.0, StatusCode::OK);
    valid["description"] = json!("A changed report under the same UUID");
    let (status, error) = send(&service, valid).await;
    assert_eq!(status, StatusCode::CONFLICT);
    assert_eq!(error["error"], "request_conflict");
    assert_eq!(github.posts.load(Ordering::SeqCst), 1);
    task.abort();
}

#[tokio::test]
async fn lost_github_response_survives_restart_and_reconciles_closed_issue_without_repost() {
    let (endpoint, github, task) = github_stub().await;
    github.lose_response.store(true, Ordering::SeqCst);
    let temp = TempDir::new().unwrap();
    let first = service(&temp, &endpoint);
    let payload = report();
    assert_eq!(send(&first, payload.clone()).await.0, StatusCode::ACCEPTED);
    // The receipt has been durably written, but contains no title or description.
    let filename = temp
        .path()
        .join("receipts")
        .join(format!("{}.json", payload["request_id"].as_str().unwrap()));
    let saved = std::fs::read_to_string(filename).unwrap();
    assert!(!saved.contains("Testbericht"));
    drop(first);
    let restarted = service(&temp, &endpoint);
    assert_eq!(send(&restarted, payload).await.0, StatusCode::OK);
    assert_eq!(github.posts.load(Ordering::SeqCst), 1);
    assert_eq!(github.gets.load(Ordering::SeqCst), 1);
    task.abort();
}

#[tokio::test]
async fn absent_reconciliation_result_remains_pending_without_another_post() {
    let (endpoint, github, task) = github_stub().await;
    github.lose_response.store(true, Ordering::SeqCst);
    github.hide_issue.store(true, Ordering::SeqCst);
    let temp = TempDir::new().unwrap();
    let service = service(&temp, &endpoint);
    let payload = report();
    for _ in 0..3 {
        assert_eq!(
            send(&service, payload.clone()).await.0,
            StatusCode::ACCEPTED
        );
    }
    assert_eq!(github.posts.load(Ordering::SeqCst), 1);
    assert_eq!(github.gets.load(Ordering::SeqCst), 1);
    task.abort();
}

fn png(width: u32, height: u32) -> Vec<u8> {
    let image = DynamicImage::new_rgba8(width, height);
    let mut data = Cursor::new(Vec::new());
    image.write_to(&mut data, ImageFormat::Png).unwrap();
    data.into_inner()
}

#[test]
fn screenshot_reencoding_drops_source_extras_and_enforces_type_pixels_and_bytes() {
    let mut source = png(2, 2);
    source.extend_from_slice(b"PRIVATE_SOURCE_METADATA");
    let screenshot = model::Screenshot {
        data_url: format!("data:image/png;base64,{}", BASE64_STANDARD.encode(source)),
    };
    let result = sanitize_image(&screenshot).unwrap();
    assert!(
        !result
            .windows(23)
            .any(|window| window == b"PRIVATE_SOURCE_METADATA")
    );
    assert_eq!(image::load_from_memory(&result).unwrap().width(), 2);
    assert_eq!(
        image::load_from_memory(&result)
            .unwrap()
            .to_rgba8()
            .get_pixel(0, 0)
            .0,
        [255, 255, 255, 255]
    );
    for data_url in [
        "data:image/svg+xml;base64,PHN2Zy8+".into(),
        format!(
            "data:image/jpeg;base64,{}",
            BASE64_STANDARD.encode(png(2, 2))
        ),
        format!(
            "data:image/png;base64,{}",
            BASE64_STANDARD.encode(png(4097, 1))
        ),
        format!(
            "data:image/png;base64,{}",
            "A".repeat(model::MAX_IMAGE_BYTES.div_ceil(3) * 4 + 4)
        ),
    ] {
        assert!(sanitize_image(&model::Screenshot { data_url }).is_err());
    }
}

#[tokio::test]
async fn screenshot_is_sanitized_publicly_readable_and_expires_without_losing_idempotency() {
    let (endpoint, github, task) = github_stub().await;
    let temp = TempDir::new().unwrap();
    let service = service(&temp, &endpoint);
    let mut payload = report();
    payload["screenshot"] =
        json!({"data_url":format!("data:image/png;base64,{}", BASE64_STANDARD.encode(png(4, 3)))});
    assert_eq!(send(&service, payload.clone()).await.0, StatusCode::OK);
    let request_id: Uuid = payload["request_id"].as_str().unwrap().parse().unwrap();
    let mut receipt = service.store.lock().unwrap().receipts[&request_id].clone();
    let image_id = receipt.image.unwrap();
    let response = download_image(State(Arc::clone(&service)), Path(image_id)).await;
    assert_eq!(response.status(), StatusCode::OK);
    assert_eq!(response.headers()[header::CONTENT_TYPE], "image/png");
    assert_eq!(response.headers()[header::CACHE_CONTROL], "no-store");
    assert_eq!(
        image::load_from_memory(
            &to_bytes(response.into_body(), model::MAX_IMAGE_BYTES)
                .await
                .unwrap()
        )
        .unwrap()
        .height(),
        3
    );
    receipt.created_at = now() - storage::IMAGE_TTL;
    {
        let mut store = service.store.lock().unwrap();
        store.save(receipt).unwrap();
        store.cleanup(now()).unwrap();
        assert!(!store.image_path(image_id).exists());
        assert_eq!(store.receipts.len(), 1);
    }
    assert_eq!(
        download_image(State(Arc::clone(&service)), Path(image_id))
            .await
            .status(),
        StatusCode::NOT_FOUND
    );
    assert_eq!(send(&service, payload).await.0, StatusCode::OK);
    assert_eq!(github.posts.load(Ordering::SeqCst), 1);
    task.abort();
}

#[tokio::test]
async fn guest_rate_limit_and_body_limit_are_enforced_before_publication() {
    let (endpoint, github, task) = github_stub().await;
    let temp = TempDir::new().unwrap();
    let service = service(&temp, &endpoint);
    let payload = report();
    for _ in 0..30 {
        assert_eq!(send(&service, payload.clone()).await.0, StatusCode::OK);
    }
    let (status, error) = send(&service, payload).await;
    assert_eq!(status, StatusCode::TOO_MANY_REQUESTS);
    assert_eq!(error["error"], "rate_limited");
    let request = Request::post("/feedback")
        .header("Content-Type", "application/json")
        .extension(ClientIp("192.0.2.2".parse().unwrap()))
        .body(Body::from("x".repeat(MAX_BODY_BYTES + 1)))
        .unwrap();
    let response = service_router(Arc::clone(&service))
        .finish_api(&mut Default::default())
        .oneshot(request)
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::PAYLOAD_TOO_LARGE);
    assert_eq!(github.posts.load(Ordering::SeqCst), 1);
    task.abort();
}

#[tokio::test]
async fn storage_capacity_and_single_writer_prevent_unbounded_or_parallel_publishing() {
    let (endpoint, github, task) = github_stub().await;
    let temp = TempDir::new().unwrap();
    let service = service(&temp, &endpoint);
    assert!(Store::open(temp.path(), now()).is_err());
    {
        let mut store = service.store.lock().unwrap();
        let image = Uuid::new_v4();
        // A sparse file exercises the storage bound without allocating 256 MiB.
        std::fs::File::create(store.image_path(image))
            .unwrap()
            .set_len(MAX_IMAGE_STORAGE)
            .unwrap();
        store
            .save(Receipt {
                request_id: Uuid::new_v4(),
                fingerprint: "existing".into(),
                marker: Uuid::new_v4(),
                created_at: now(),
                image: Some(image),
                issue_url: None,
            })
            .unwrap();
    }
    let mut payload = report();
    payload["screenshot"] =
        json!({"data_url":format!("data:image/png;base64,{}", BASE64_STANDARD.encode(png(2,2)))});
    let (status, error) = send(&service, payload).await;
    assert_eq!(status, StatusCode::SERVICE_UNAVAILABLE);
    assert_eq!(error["error"], "capacity_exceeded");
    assert_eq!(github.posts.load(Ordering::SeqCst), 0);
    task.abort();
}

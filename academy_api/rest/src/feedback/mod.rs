//! Guest-safe public feedback. The local journal is a single-writer, durable
//! idempotency receipt, not a queue that blindly retries GitHub creation.
mod github;
mod model;
mod storage;
#[cfg(test)]
mod tests;

use std::{
    collections::{HashMap, VecDeque},
    net::IpAddr,
    sync::{Arc, Mutex},
    time::Duration,
};

use academy_config::FeedbackConfig;
use aide::axum::ApiRouter;
use axum::{
    Extension, Json,
    extract::{DefaultBodyLimit, Path, Request, State, rejection::JsonRejection},
    http::{StatusCode, header},
    middleware::{self, Next},
    response::{IntoResponse, Response},
    routing::{get, post},
};
use serde_json::json;
use tokio::sync::Semaphore;
use uuid::Uuid;

use crate::middlewares::client_ip::ClientIp;
use github::Github;
use model::{FeedbackRequest, MAX_BODY_BYTES, sanitize_image};
use storage::{MAX_IMAGE_STORAGE, MAX_RECORDS, Receipt, Store};

struct Service {
    store: Arc<Mutex<Store>>,
    github: Github,
    public_base_url: String,
    // No unbounded queue of image decoders or uploads in memory.
    admission: Arc<Semaphore>,
    submission: tokio::sync::Mutex<()>,
    throttle: Mutex<Throttle>,
    reconciled_at: Mutex<HashMap<Uuid, i64>>,
}

#[derive(Default)]
struct Throttle {
    clients: HashMap<IpAddr, VecDeque<i64>>,
    global: VecDeque<i64>,
}

impl Throttle {
    fn allow(&mut self, ip: IpAddr, now: i64) -> bool {
        self.clients.retain(|_, times| {
            times.retain(|time| *time > now - 3600);
            !times.is_empty()
        });
        self.global.retain(|time| *time > now - 3600);
        if self.global.len() >= 120
            || (!self.clients.contains_key(&ip) && self.clients.len() >= 4096)
        {
            return false;
        }
        let client = self.clients.entry(ip).or_default();
        if client.len() >= 30 {
            return false;
        }
        client.push_back(now);
        self.global.push_back(now);
        true
    }
}

pub async fn router(config: &FeedbackConfig) -> anyhow::Result<ApiRouter<()>> {
    if !config.enabled {
        return Ok(ApiRouter::new());
    }
    let url = reqwest::Url::parse(&config.public_base_url)?;
    anyhow::ensure!(
        url.scheme() == "https"
            && url.host_str().is_some()
            && url.username().is_empty()
            && url.password().is_none()
            && url.query().is_none()
            && url.fragment().is_none()
            && url.path() == "/",
        "feedback public_base_url must be an HTTPS origin"
    );
    anyhow::ensure!(
        config.storage_path.is_absolute() && config.github_token_file.is_absolute(),
        "feedback paths must be absolute"
    );
    let root = config.storage_path.clone();
    let store = tokio::task::spawn_blocking(move || Store::open(&root, now())).await??;
    let github = Github::from_token_file(&config.github_token_file)?;
    let service = Arc::new(Service {
        store: Arc::new(Mutex::new(store)),
        github,
        public_base_url: config.public_base_url.trim_end_matches('/').into(),
        admission: Arc::new(Semaphore::new(2)),
        submission: tokio::sync::Mutex::new(()),
        throttle: Mutex::default(),
        reconciled_at: Mutex::default(),
    });
    let weak = Arc::downgrade(&service);
    tokio::spawn(async move {
        let mut interval = tokio::time::interval(Duration::from_secs(3600));
        loop {
            interval.tick().await;
            let Some(service) = weak.upgrade() else {
                break;
            };
            let store = Arc::clone(&service.store);
            let result = tokio::task::spawn_blocking(move || {
                store.lock().expect("feedback store").cleanup(now())
            })
            .await;
            if !matches!(result, Ok(Ok(()))) {
                tracing::error!("feedback image cleanup failed");
            }
        }
    });
    Ok(service_router(service))
}

fn service_router(service: Arc<Service>) -> ApiRouter<()> {
    ApiRouter::new()
        .route(
            "/feedback",
            post(submit)
                .layer(DefaultBodyLimit::max(MAX_BODY_BYTES))
                .layer(middleware::from_fn_with_state(Arc::clone(&service), admit)),
        )
        .route("/feedback/images/{image_id}", get(download_image))
        .with_state(service)
}

async fn admit(
    State(service): State<Arc<Service>>,
    Extension(ClientIp(ip)): Extension<ClientIp>,
    request: Request,
    next: Next,
) -> Response {
    if !service
        .throttle
        .lock()
        .expect("feedback throttle")
        .allow(ip, now())
    {
        return (
            [(header::RETRY_AFTER, "3600")],
            error(
                StatusCode::TOO_MANY_REQUESTS,
                "rate_limited",
                "Please try again later",
            ),
        )
            .into_response();
    }
    let Ok(_permit) = service.admission.try_acquire() else {
        return error(
            StatusCode::SERVICE_UNAVAILABLE,
            "unavailable",
            "Feedback is busy; please try again later",
        );
    };
    next.run(request).await
}

async fn submit(
    State(service): State<Arc<Service>>,
    payload: Result<Json<FeedbackRequest>, JsonRejection>,
) -> Response {
    let request = match payload {
        Ok(Json(request)) => request,
        Err(rejection) => {
            return error(
                rejection.status(),
                "invalid_request",
                "Invalid feedback JSON or request too large",
            );
        }
    };
    if let Err(message) = request.validate() {
        return error(StatusCode::UNPROCESSABLE_ENTITY, "invalid_request", message);
    }
    // Serialize creation and reconciliation; no lock is held across image downloads.
    let Ok(_submission) = service.submission.try_lock() else {
        return error(
            StatusCode::SERVICE_UNAVAILABLE,
            "unavailable",
            "Feedback is busy; keep your draft and try again",
        );
    };
    let fingerprint = request.fingerprint();
    let prior = service
        .store
        .lock()
        .expect("feedback store")
        .receipts
        .get(&request.request_id)
        .cloned();
    if let Some(mut receipt) = prior {
        if receipt.fingerprint != fingerprint {
            return error(
                StatusCode::CONFLICT,
                "request_conflict",
                "This request ID already belongs to a different report",
            );
        }
        if let Some(url) = &receipt.issue_url {
            return created(url);
        }
        let should_reconcile = {
            let mut times = service
                .reconciled_at
                .lock()
                .expect("feedback reconcile throttle");
            let due = times
                .get(&request.request_id)
                .is_none_or(|time| *time <= now() - 60);
            if due {
                times.insert(request.request_id, now());
            }
            due
        };
        if should_reconcile {
            let result =
                tokio::time::timeout(Duration::from_secs(20), service.github.reconcile(&receipt))
                    .await;
            if let Ok(Some(url)) = result {
                receipt.issue_url = Some(url.clone());
                if save_receipt(&service, receipt).await.is_ok() {
                    return created(&url);
                }
            }
        }
        return pending(request.request_id);
    }
    let image = match request.screenshot.as_ref() {
        Some(_) => {
            let screenshot = request
                .screenshot
                .as_ref()
                .map(|s| model::Screenshot {
                    data_url: s.data_url.clone(),
                })
                .expect("screenshot");
            match tokio::task::spawn_blocking(move || sanitize_image(&screenshot)).await {
                Ok(Ok(image)) => Some(image),
                Ok(Err(message)) => {
                    return error(StatusCode::UNPROCESSABLE_ENTITY, "invalid_request", message);
                }
                Err(_) => {
                    return error(
                        StatusCode::SERVICE_UNAVAILABLE,
                        "unavailable",
                        "Could not process screenshot",
                    );
                }
            }
        }
        None => None,
    };
    let receipt = Receipt {
        request_id: request.request_id,
        fingerprint,
        marker: Uuid::new_v4(),
        created_at: now(),
        image: image.as_ref().map(|_| Uuid::new_v4()),
        issue_url: None,
    };
    let store = Arc::clone(&service.store);
    let prepared = receipt.clone();
    // This fsync MUST succeed before any non-idempotent GitHub request.
    let result = tokio::task::spawn_blocking(move || -> anyhow::Result<bool> {
        let mut store = store.lock().expect("feedback store");
        store.cleanup(now())?;
        if store.receipts.len() >= MAX_RECORDS
            || store
                .receipts
                .values()
                .filter(|r| r.created_at > now() - 86400)
                .count()
                >= 100
            || store.image_bytes()? + image.as_ref().map_or(0, |b| b.len() as u64)
                > MAX_IMAGE_STORAGE
        {
            return Ok(false);
        }
        if let (Some(id), Some(bytes)) = (prepared.image, image) {
            store.put_image(id, &bytes)?;
        }
        store.save(prepared)?;
        Ok(true)
    })
    .await;
    match result {
        Ok(Ok(true)) => (),
        Ok(Ok(false)) => {
            return error(
                StatusCode::SERVICE_UNAVAILABLE,
                "capacity_exceeded",
                "Feedback storage or daily submission limit reached",
            );
        }
        _ => {
            return error(
                StatusCode::SERVICE_UNAVAILABLE,
                "unavailable",
                "Could not accept feedback; keep the same request ID when retrying",
            );
        }
    }
    let image_url = receipt
        .image
        .map(|id| format!("{}/feedback/images/{id}", service.public_base_url));
    let body = request.issue_body(receipt.marker, image_url.as_deref());
    if let Some(url) = service
        .github
        .create(&request.title, &body, receipt.marker)
        .await
    {
        let mut receipt = receipt;
        receipt.issue_url = Some(url.clone());
        if save_receipt(&service, receipt).await.is_ok() {
            return created(&url);
        }
    }
    pending(request.request_id)
}

async fn save_receipt(service: &Service, receipt: Receipt) -> anyhow::Result<()> {
    let store = Arc::clone(&service.store);
    tokio::task::spawn_blocking(move || store.lock().expect("feedback store").save(receipt)).await?
}

async fn download_image(
    State(service): State<Arc<Service>>,
    Path(image_id): Path<Uuid>,
) -> Response {
    let path = {
        let store = service.store.lock().expect("feedback store");
        if !store.image_is_live(image_id, now()) {
            return StatusCode::NOT_FOUND.into_response();
        }
        store.image_path(image_id)
    };
    match tokio::fs::read(path).await {
        Ok(bytes) if bytes.len() <= model::MAX_IMAGE_BYTES => (
            [
                (header::CONTENT_TYPE, "image/png"),
                (header::CACHE_CONTROL, "no-store"),
                (header::X_CONTENT_TYPE_OPTIONS, "nosniff"),
                (header::CONTENT_DISPOSITION, "inline; filename=feedback.png"),
                (
                    header::CONTENT_SECURITY_POLICY,
                    "default-src 'none'; sandbox",
                ),
            ],
            bytes,
        )
            .into_response(),
        _ => StatusCode::NOT_FOUND.into_response(),
    }
}

fn error(status: StatusCode, code: &str, message: &str) -> Response {
    (status, Json(json!({ "error": code, "message": message }))).into_response()
}
fn created(url: &str) -> Response {
    Json(json!({ "status": "created", "issue_url": url })).into_response()
}
fn pending(request_id: Uuid) -> Response {
    (
        StatusCode::ACCEPTED,
        Json(json!({ "status": "pending", "request_id": request_id })),
    )
        .into_response()
}
fn now() -> i64 {
    chrono::Utc::now().timestamp()
}

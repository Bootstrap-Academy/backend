use super::*;
use academy_models::{
    coin::Balance,
    heart::{HeartOperationOutcome, HeartOperationReceipt, Hearts},
    user::UserComposite,
};
use axum::{
    body::{Body, to_bytes},
    http::Request,
};
use serde_json::{Value, json};
use std::sync::Mutex;
use tower::ServiceExt;

const OPERATION: &str = "11111111-1111-4111-8111-111111111111";
const USER: &str = "22222222-2222-4222-8222-222222222222";

#[derive(Default)]
struct Feature {
    calls: Mutex<Vec<HeartOperation>>,
    failure: Option<&'static str>,
}

impl InternalService for Feature {
    async fn apply_heart_operation(
        &self,
        token: &InternalToken,
        operation: HeartOperation,
    ) -> Result<HeartOperationReceipt, InternalHeartOperationError> {
        assert_eq!(token.as_str(), "synthetic-internal-token");
        self.calls.lock().unwrap().push(operation.clone());
        match self.failure {
            Some("conflict") => Err(InternalHeartOperationError::OperationConflict),
            Some("invalid") => Err(InternalHeartOperationError::InvalidRequest),
            Some("missing") => Err(InternalHeartOperationError::UserNotFound),
            Some("auth") => Err(AuthInternalAuthenticateError::InvalidToken.into()),
            _ => Ok(HeartOperationReceipt {
                operation_id: operation.id,
                user_id: operation.user_id,
                charged_half_hearts: 2,
                hearts: 8,
                outcome: HeartOperationOutcome::Charged,
            }),
        }
    }
    async fn get_user(
        &self,
        _: &InternalToken,
        _: UserId,
    ) -> Result<UserComposite, InternalGetUserError> {
        panic!("unexpected route")
    }
    async fn get_user_by_email(
        &self,
        _: &InternalToken,
        _: EmailAddress,
    ) -> Result<UserComposite, InternalGetUserByEmailError> {
        panic!("unexpected route")
    }
    async fn add_coins(
        &self,
        _: &InternalToken,
        _: UserId,
        _: i64,
        _: Option<TransactionDescription>,
        _: bool,
    ) -> Result<Balance, InternalAddCoinsError> {
        panic!("unexpected route")
    }
    async fn apply_coin_operation(
        &self,
        _: &InternalToken,
        _: CoinOperation,
    ) -> Result<Balance, InternalAddCoinsError> {
        panic!("unexpected route")
    }
    async fn get_hearts(
        &self,
        _: &InternalToken,
        _: UserId,
    ) -> Result<Hearts, InternalGetHeartsError> {
        panic!("unexpected route")
    }
    async fn add_hearts(
        &self,
        _: &InternalToken,
        _: UserId,
        _: i64,
    ) -> Result<Hearts, InternalAddHeartsError> {
        panic!("unexpected route")
    }
    async fn has_premium(
        &self,
        _: &InternalToken,
        _: UserId,
    ) -> Result<bool, InternalHasPremiumError> {
        panic!("unexpected route")
    }
}

async fn request(service: Arc<Feature>, body: Value) -> Response {
    router(service)
        .finish_api(&mut Default::default())
        .oneshot(
            Request::builder()
                .method("PUT")
                .uri(format!(
                    "/shop/_internal/heart-operations/{OPERATION}/{USER}"
                ))
                .header("authorization", "synthetic-internal-token")
                .header("content-type", "application/json")
                .body(Body::from(body.to_string()))
                .unwrap(),
        )
        .await
        .unwrap()
}

#[tokio::test]
async fn exact_route_preserves_operation_identity_and_receipt_wire_units() {
    let service = Arc::new(Feature::default());
    let result = request(
        Arc::clone(&service),
        json!({"half_hearts":2,"reason":"incorrect_challenge_attempt"}),
    )
    .await;
    assert_eq!(result.status(), StatusCode::OK);
    let body: Value =
        serde_json::from_slice(&to_bytes(result.into_body(), 4096).await.unwrap()).unwrap();
    assert_eq!(
        body,
        json!({"operation_id":OPERATION,"user_id":USER,"charged_half_hearts":2,"hearts":8,"outcome":"charged"})
    );
    let calls = service.calls.lock().unwrap();
    assert_eq!(calls.len(), 1);
    assert_eq!(calls[0].id.to_string(), OPERATION);
    assert_eq!(calls[0].user_id.to_string(), USER);
    assert_eq!(calls[0].half_hearts, 2);
    assert_eq!(calls[0].reason, "incorrect_challenge_attempt");
}

#[tokio::test]
async fn malformed_types_and_unrecognized_body_fields_never_reach_a_debit() {
    for body in [
        json!({"half_hearts":true,"reason":"incorrect_challenge_attempt"}),
        json!({"half_hearts":-2,"reason":"incorrect_challenge_attempt"}),
        json!({"half_hearts":2.0,"reason":"incorrect_challenge_attempt"}),
        json!({"half_hearts":"2","reason":"incorrect_challenge_attempt"}),
        json!({"half_hearts":2,"reason":"incorrect_challenge_attempt","premium":false}),
        json!({"half_hearts":2}),
    ] {
        let service = Arc::new(Feature::default());
        let response = request(Arc::clone(&service), body).await;
        assert_eq!(response.status(), StatusCode::UNPROCESSABLE_ENTITY);
        assert!(service.calls.lock().unwrap().is_empty());
    }
}

#[tokio::test]
async fn retry_conflict_auth_invalid_and_missing_have_distinct_http_outcomes() {
    for (failure, status) in [
        ("conflict", StatusCode::CONFLICT),
        ("invalid", StatusCode::UNPROCESSABLE_ENTITY),
        ("missing", StatusCode::NOT_FOUND),
        ("auth", StatusCode::UNAUTHORIZED),
    ] {
        let service = Arc::new(Feature {
            failure: Some(failure),
            ..Default::default()
        });
        assert_eq!(
            request(
                service,
                json!({"half_hearts":2,"reason":"incorrect_challenge_attempt"})
            )
            .await
            .status(),
            status
        );
    }
}

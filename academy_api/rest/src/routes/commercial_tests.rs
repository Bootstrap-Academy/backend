use super::*;
use academy_models::purchase::PurchaseStatus;
use axum::{
    body::{Body, to_bytes},
    http::{HeaderValue, Method, Request},
};
use std::sync::Mutex;
use tower::ServiceExt;

const OFFER: &str = "11111111-1111-4111-8111-111111111111";
const OWNER: &str = "22222222-2222-4222-8222-222222222222";

#[derive(Default)]
struct CountingFeature {
    calls: Mutex<Vec<Value>>,
    failure: Option<&'static str>,
}
impl CountingFeature {
    fn record(&self, credentials: CommercialCredentials, kind: &str) -> anyhow::Result<()> {
        self.calls.lock().unwrap().push(json!({
            "kind":kind,"claim":credentials.claim_key,
            "capability":credentials.recipient.capability,
            "ordinary":credentials.recipient.ordinary.map(|v|v.as_str().to_owned()),
        }));
        match self.failure {
            Some("missing") => Err(academy_core_purchase_contracts::PurchaseError::NotFound.into()),
            Some("unavailable") => {
                Err(academy_core_purchase_contracts::PurchaseError::Unavailable.into())
            }
            _ => Ok(()),
        }
    }
}
impl CommercialFeatureService for CountingFeature {
    async fn commercial_document_inventory(
        &self,
        credentials: CommercialCredentials,
    ) -> anyhow::Result<academy_models::commercial_document::DocumentInventory> {
        self.record(credentials, "inventory")?;
        Ok(serde_json::from_value(
            json!({"protocol":1,"claimant_subject":OWNER,"observed_at":"2026-09-10T00:00:00Z",
              "scope":{"finance":"claimant_only","purchases":"claimant_and_same_case_learning_subjects","archives_scanned":false,"remote_sources_queried":false,"catalog_complete":false,"historical_owner_inventory_complete":false,"known_local_enumeration_complete":true},
              "records":[{"family":"finance","kind":"final-statement","source_service":"backend","source_subject":OWNER,"owner_relation":"claimant","purchase_source":null,"offer_id":null,"printed_number":"S18446744073709551615","record_basis":"owned_document","reader_state":"archive_unchecked","reason":null,"selector":{"kind":"final-statement","id":"18446744073709551615","variant":"original"},"artifacts":[]}]
            }),
        )?)
    }
    async fn commercial_learning(&self, _: &str, _: &str, _: Value) -> anyhow::Result<Value> {
        panic!("unexpected learning request")
    }
    async fn commercial_admin(
        &self,
        access: &AccessToken,
        operation: &str,
        body: Value,
    ) -> anyhow::Result<Value> {
        self.calls.lock().unwrap().push(
            json!({"kind":"staff-capacity","token":access.as_str(),"operation":operation,"body":body}),
        );
        if matches!(
            operation,
            "cash_basis_review" | "reserve" | "split_cash" | "record_cash_payment" | "outcome"
        ) {
            if access.as_str().is_empty() {
                return Err(academy_models::auth::AuthenticateError::InvalidToken.into());
            }
            return Err(RecipientAccessError::Malformed.into());
        }
        if operation == "retention_page" {
            if access.as_str().is_empty() || self.failure == Some("auth") {
                return Err(academy_models::auth::AuthenticateError::InvalidToken.into());
            }
            match self.failure {
                Some("auth-unavailable") => {
                    return Err(
                        academy_models::auth::AuthenticateError::Other(anyhow::anyhow!(
                            "auth unavailable"
                        ))
                        .into(),
                    );
                }
                Some("malformed") => return Err(RecipientAccessError::Malformed.into()),
                Some("mfa") => return Err(academy_models::auth::AuthorizeError::AdminMfa.into()),
                Some("conflict") => return Err(RecipientAccessError::Conflict.into()),
                Some("unavailable") => anyhow::bail!("unexpected retention projection"),
                _ => {}
            }
            return Ok(
                json!({"protocol":1,"family":body["family"],"limit":body["limit"],"observed_at":"2026-09-11 00:00:00.123456+00","semantics":"live_queue","rows":[],"next_cursor":null,"exhausted":true}),
            );
        }
        if operation == "determination_status" {
            if access.as_str().is_empty() {
                return Err(academy_models::auth::AuthenticateError::InvalidToken.into());
            }
            match self.failure {
                Some("missing") => return Err(RecipientAccessError::NotFound.into()),
                Some("malformed") => return Err(RecipientAccessError::Malformed.into()),
                Some("unavailable") => anyhow::bail!("stored matching history unavailable"),
                Some("auth") => {
                    return Err(academy_models::auth::AuthenticateError::InvalidToken.into());
                }
                Some("auth-unavailable") => {
                    return Err(
                        academy_models::auth::AuthenticateError::Other(anyhow::anyhow!(
                            "final auth unavailable"
                        ))
                        .into(),
                    );
                }
                Some("mfa") => return Err(academy_models::auth::AuthorizeError::AdminMfa.into()),
                _ => {}
            }
            return Ok(
                json!({"protocol":1,"case_id":OFFER,"subject":OWNER,"observed_at":"2026-09-11 00:00:00.123456+00","obligation":{"id":OFFER,"units":"9223372036854775807","cash_units":null,"original_json":"9007199254740993","determination_json":"null"},"journal":null}),
            );
        }
        if matches!(operation, "hold_review" | "hold_queue") {
            if access.as_str().is_empty() {
                return Err(academy_models::auth::AuthenticateError::InvalidToken.into());
            }
            return Ok(json!({"operation":operation,"public_body":body}));
        }
        assert_eq!(operation, "cash_capacity");
        if access.as_str().is_empty() {
            return Err(academy_models::auth::AuthenticateError::InvalidToken.into());
        }
        Ok(
            json!({"case_id":OFFER,"subject":OWNER,"remaining_purchase_capacity":null,"captured_purchase_units":"9007199254740993"}),
        )
    }
    async fn commercial_admin_document(
        &self,
        access: &AccessToken,
        case_id: uuid::Uuid,
        kind: &str,
        id: &str,
        variant: &str,
    ) -> anyhow::Result<Vec<u8>> {
        self.calls.lock().unwrap().push(json!({"kind":"staff-document","token":access.as_str(),"case_id":case_id,"document_kind":kind,"id":id,"variant":variant}));
        if access.as_str().is_empty() {
            return Err(academy_models::auth::AuthenticateError::InvalidToken.into());
        }
        match self.failure {
            Some("auth") => Err(academy_models::auth::AuthenticateError::InvalidToken.into()),
            Some("mfa") => Err(academy_models::auth::AuthorizeError::AdminMfa.into()),
            Some("missing") => Err(RecipientAccessError::NotFound.into()),
            Some("malformed") => Err(RecipientAccessError::Malformed.into()),
            Some("unavailable") => {
                Err(academy_core_purchase_contracts::PurchaseError::Unavailable.into())
            }
            _ => Ok(b"original\r\n\0exact".to_vec()),
        }
    }
    async fn commercial_internal(
        &self,
        _: &InternalToken,
        _: &str,
        _: Value,
    ) -> anyhow::Result<Value> {
        panic!("unexpected internal request")
    }
    async fn commercial_recipient(
        &self,
        credentials: CommercialCredentials,
        operation: &str,
        body: Value,
    ) -> anyhow::Result<Value> {
        if operation == "restore_credit" {
            self.record(credentials, "restore_credit")?;
            self.calls.lock().unwrap().push(json!({"body":body}));
            return Ok(json!({"historical":"opaque original receipt"}));
        }
        assert_eq!(operation, "export");
        assert_eq!(body, json!({}));
        self.record(credentials, "recipient")?;
        Ok(json!({"observed":true}))
    }
    async fn commercial_document(
        &self,
        credentials: CommercialCredentials,
        kind: &str,
        id: &str,
        variant: &str,
    ) -> anyhow::Result<Vec<u8>> {
        assert_eq!((kind, id, variant), ("purchase", OFFER, "timing"));
        self.record(credentials, "document")?;
        Ok(b"exact original bytes".to_vec())
    }
    async fn commercial_purchase_status(
        &self,
        credentials: CommercialCredentials,
        offer: uuid::Uuid,
    ) -> anyhow::Result<PurchaseStatus> {
        assert_eq!(offer.to_string(), OFFER);
        self.record(credentials, "status")?;
        Ok(serde_json::from_value(json!({
            "offer":{"id":OFFER,"user_id":OWNER,"source":"skills",
                "created_at":"2026-09-01T00:00:00Z","expires_at":"2026-09-01T00:10:00Z",
                "recipient":"original@example.invalid","product":{"kind":"course","reference":"original",
                    "title":"Original","description":"Original","coins":7,"facts":{},"revision":"r1","service_starts_at":null},
                "document_hash":"original-document","hash":"original-offer","text":"original","declaration":"original"},
            "state":"review","accepted_at":null,"confirmation_smtp_accepted_at":null,
            "fulfillment":null,"financial_evidence":null,"review_reason":"Original review",
            "provision_deadline":null,"provision_timing":null,"document_corrections":["timing"]
        }))?)
    }
}
fn path(family: &str) -> String {
    match family {
        "inventory" => "/shop/claims/documents".into(),
        "recipient" => "/shop/claims/recipient/export".into(),
        "document" => format!("/shop/claims/documents/purchase/{OFFER}/timing"),
        _ => format!("/shop/claims/purchases/{OFFER}/status"),
    }
}
async fn request(
    service: Arc<CountingFeature>,
    family: &str,
    headers: Vec<(&str, HeaderValue)>,
) -> Response {
    let mut request = Request::builder()
        .method(if family == "recipient" {
            Method::POST
        } else {
            Method::GET
        })
        .uri(path(family));
    for (name, value) in headers {
        request = request.header(name, value);
    }
    if family == "recipient" {
        request = request.header(header::CONTENT_TYPE, "application/json");
    }
    router(service)
        .finish_api(&mut Default::default())
        .oneshot(
            request
                .body(Body::from(if family == "recipient" { "{}" } else { "" }))
                .unwrap(),
        )
        .await
        .unwrap()
}
fn bad() -> HeaderValue {
    HeaderValue::from_bytes(&[0xff]).unwrap()
}
fn text(v: &str) -> HeaderValue {
    HeaderValue::from_str(v).unwrap()
}
async fn error(response: Response, expected: StatusCode) {
    assert_eq!(response.status(), expected);
    assert_eq!(response.headers()[header::CACHE_CONTROL], "no-store");
    assert_eq!(response.headers()[header::REFERRER_POLICY], "no-referrer");
    let body: Value =
        serde_json::from_slice(&to_bytes(response.into_body(), 65536).await.unwrap()).unwrap();
    assert_eq!(body.as_object().unwrap().len(), 1);
    assert!(body["detail"].is_string());
}

#[tokio::test]
async fn commercial_malformed_selected_proof_stops_all_three_assembled_routes() {
    for family in ["recipient", "document", "status", "inventory"] {
        for malformed_claim in [true, false] {
            let service = Arc::new(CountingFeature::default());
            let mut headers = vec![("authorization", text("Bearer valid ordinary"))];
            if malformed_claim {
                headers.push(("x-commercial-claim-key", bad()));
                headers.push(("x-moderation-capability", text(&"c".repeat(40))));
            } else {
                headers.push(("x-moderation-capability", bad()));
            }
            error(
                request(Arc::clone(&service), family, headers).await,
                StatusCode::UNAUTHORIZED,
            )
            .await;
            assert!(service.calls.lock().unwrap().is_empty());
        }
    }
}
#[tokio::test]
async fn commercial_selected_valid_proof_ignores_malformed_lower_headers() {
    for family in ["recipient", "document", "status", "inventory"] {
        for claim in [true, false] {
            let service = Arc::new(CountingFeature::default());
            let value = if claim {
                "k".repeat(43)
            } else {
                "c".repeat(40)
            };
            let mut headers = vec![("authorization", bad())];
            if claim {
                headers.push(("x-commercial-claim-key", text(&value)));
                headers.push(("x-moderation-capability", bad()));
            } else {
                headers.push(("x-moderation-capability", text(&value)));
            }
            let response = request(Arc::clone(&service), family, headers).await;
            assert_eq!(response.status(), StatusCode::OK);
            let calls = service.calls.lock().unwrap();
            assert_eq!(calls.len(), 1);
            assert_eq!(
                calls[0],
                json!({"kind":family,"claim":if claim{Some(&value)}else{None},"capability":if claim{None}else{Some(&value)},"ordinary":null})
            );
        }
    }
}
#[tokio::test]
async fn commercial_decoded_empty_short_proof_stays_present_for_the_real_principal() {
    // This counting boundary proves extraction only; core tests exercise rejection.
    for family in ["recipient", "document", "status", "inventory"] {
        for header_name in ["x-commercial-claim-key", "x-moderation-capability"] {
            for value in ["", "short"] {
                let service = Arc::new(CountingFeature::default());
                let response = request(
                    Arc::clone(&service),
                    family,
                    vec![
                        (header_name, text(value)),
                        ("authorization", text("Bearer lower")),
                    ],
                )
                .await;
                assert_eq!(response.status(), StatusCode::OK);
                let calls = service.calls.lock().unwrap();
                assert_eq!(calls.len(), 1);
                assert_eq!(
                    calls[0][if header_name == "x-commercial-claim-key" {
                        "claim"
                    } else {
                        "capability"
                    }],
                    value
                );
                assert!(calls[0]["ordinary"].is_null());
            }
        }
    }
}
#[tokio::test]
async fn commercial_ordinary_only_and_absent_credentials_keep_the_existing_parsing() {
    for family in ["recipient", "document", "status", "inventory"] {
        for (header, expected) in [
            (Some(text("Bearer ordinary")), Some("ordinary")),
            (Some(text("ordinary")), None),
            (Some(bad()), None),
            (None, None),
        ] {
            let service = Arc::new(CountingFeature::default());
            let response = request(
                Arc::clone(&service),
                family,
                header
                    .map(|v| vec![("authorization", v)])
                    .unwrap_or_default(),
            )
            .await;
            assert_eq!(response.status(), StatusCode::OK);
            let calls = service.calls.lock().unwrap();
            assert_eq!(calls.len(), 1);
            assert_eq!(calls[0]["ordinary"], json!(expected));
            assert!(calls[0]["claim"].is_null() && calls[0]["capability"].is_null());
        }
    }
}
#[tokio::test]
async fn commercial_status_uuid_and_typed_read_errors_use_the_shared_json_reply() {
    let service = Arc::new(CountingFeature::default());
    let response = router(Arc::clone(&service))
        .finish_api(&mut Default::default())
        .oneshot(
            Request::builder()
                .uri("/shop/claims/purchases/not-a-uuid/status")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    error(response, StatusCode::BAD_REQUEST).await;
    assert!(service.calls.lock().unwrap().is_empty());
    for (failure, status) in [
        ("missing", StatusCode::NOT_FOUND),
        ("unavailable", StatusCode::SERVICE_UNAVAILABLE),
    ] {
        for family in ["document", "status"] {
            let service = Arc::new(CountingFeature {
                failure: Some(failure),
                ..Default::default()
            });
            error(
                request(
                    Arc::clone(&service),
                    family,
                    vec![("x-commercial-claim-key", text(&"k".repeat(43)))],
                )
                .await,
                status,
            )
            .await;
            assert_eq!(service.calls.lock().unwrap().len(), 1);
        }
    }
}
#[tokio::test]
async fn commercial_status_get_preserves_original_json_and_has_no_write_method() {
    let service = Arc::new(CountingFeature::default());
    let response = request(
        Arc::clone(&service),
        "status",
        vec![("authorization", text("Bearer ordinary"))],
    )
    .await;
    assert_eq!(response.status(), StatusCode::OK);
    assert_eq!(response.headers()[header::CACHE_CONTROL], "no-store");
    assert_eq!(response.headers()[header::REFERRER_POLICY], "no-referrer");
    let value: Value =
        serde_json::from_slice(&to_bytes(response.into_body(), 65536).await.unwrap()).unwrap();
    assert_eq!(value["offer"]["id"], OFFER);
    assert_eq!(value["offer"]["user_id"], OWNER);
    assert!(value["financial_evidence"].is_null());
    assert_eq!(value["document_corrections"], json!(["timing"]));
    let response = router(Arc::clone(&service))
        .finish_api(&mut Default::default())
        .oneshot(
            Request::builder()
                .method(Method::POST)
                .uri(path("status"))
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from("{}"))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::METHOD_NOT_ALLOWED);
    assert_eq!(service.calls.lock().unwrap().len(), 1);
}

#[tokio::test]
async fn commercial_inventory_fixed_json_get_preserves_decimal_strings_and_has_no_body_reader() {
    let service = Arc::new(CountingFeature::default());
    let response = request(
        Arc::clone(&service),
        "inventory",
        vec![("x-moderation-capability", text("full-personal"))],
    )
    .await;
    assert_eq!(response.status(), StatusCode::OK);
    assert_eq!(response.headers()[header::CACHE_CONTROL], "no-store");
    assert_eq!(response.headers()[header::REFERRER_POLICY], "no-referrer");
    let result: Value =
        serde_json::from_slice(&to_bytes(response.into_body(), 65536).await.unwrap()).unwrap();
    assert_eq!(
        result["records"][0]["selector"]["id"],
        "18446744073709551615"
    );
    assert_eq!(
        *service.calls.lock().unwrap(),
        vec![json!({"kind":"inventory","claim":null,"capability":"full-personal","ordinary":null})]
    );
    let response = router(Arc::clone(&service))
        .finish_api(&mut Default::default())
        .oneshot(
            Request::builder()
                .method(Method::POST)
                .uri("/shop/claims/documents")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::METHOD_NOT_ALLOWED);
    assert_eq!(service.calls.lock().unwrap().len(), 1);
}

async fn staff_request(
    service: Arc<CountingFeature>,
    case_id: &str,
    kind: &str,
    id: &str,
    variant: &str,
    token: bool,
) -> Response {
    let mut req = Request::builder()
        .uri(format!(
            "/shop/claims/admin/cases/{case_id}/documents/{kind}/{id}/{variant}"
        ))
        .header("x-commercial-claim-key", "not-staff")
        .header("x-moderation-capability", "not-staff")
        .header("x-learning-key", "not-staff");
    if token {
        req = req.header(header::AUTHORIZATION, "Bearer captured-staff");
    }
    router(service)
        .finish_api(&mut Default::default())
        .oneshot(req.body(Body::empty()).unwrap())
        .await
        .unwrap()
}
#[tokio::test]
async fn commercial_staff_original_route_preserves_bytes_mime_and_fixed_case_selection() {
    for (kind, id, variant) in [
        ("purchase", OFFER, "terms"),
        ("purchase", OFFER, "withdrawal"),
        ("purchase", OFFER, "confirmation"),
        ("purchase", OFFER, "timing"),
        ("purchase", OFFER, "timing-original"),
        ("purchase", OFFER, "fulfillment"),
        ("purchase", OFFER, "fulfillment-original"),
        ("invoice", "10000000", "original"),
        ("final-statement", "18446744073709551615", "original"),
        ("credit-note", "2026", "9"),
    ] {
        let service = Arc::new(CountingFeature::default());
        let reply = staff_request(Arc::clone(&service), OFFER, kind, id, variant, true).await;
        assert_eq!(reply.status(), StatusCode::OK);
        assert_eq!(reply.headers()[header::CACHE_CONTROL], "no-store");
        assert_eq!(reply.headers()[header::REFERRER_POLICY], "no-referrer");
        assert_eq!(reply.headers()[header::CONTENT_DISPOSITION], "attachment");
        assert_eq!(
            reply.headers()[header::CONTENT_TYPE],
            if kind == "purchase" && !matches!(variant, "terms" | "withdrawal") {
                "text/plain; charset=utf-8"
            } else {
                "application/pdf"
            }
        );
        assert_eq!(
            &to_bytes(reply.into_body(), 1024).await.unwrap()[..],
            b"original\r\n\0exact"
        );
        assert_eq!(
            *service.calls.lock().unwrap(),
            vec![
                json!({"kind":"staff-document","token":"captured-staff","case_id":OFFER,"document_kind":kind,"id":id,"variant":variant})
            ]
        );
    }
}
#[tokio::test]
async fn commercial_staff_original_route_rejects_bad_case_and_maps_current_authority_errors() {
    let service = Arc::new(CountingFeature::default());
    let reply = staff_request(
        Arc::clone(&service),
        "bad",
        "purchase",
        OFFER,
        "terms",
        true,
    )
    .await;
    assert_eq!(reply.status(), StatusCode::BAD_REQUEST);
    assert!(service.calls.lock().unwrap().is_empty());
    for (failure, expected) in [
        ("auth", StatusCode::UNAUTHORIZED),
        ("mfa", StatusCode::FORBIDDEN),
        ("missing", StatusCode::NOT_FOUND),
        ("malformed", StatusCode::BAD_REQUEST),
        ("unavailable", StatusCode::SERVICE_UNAVAILABLE),
    ] {
        let service = Arc::new(CountingFeature {
            failure: Some(failure),
            ..Default::default()
        });
        let reply = staff_request(service, OFFER, "purchase", OFFER, "terms", true).await;
        assert_eq!(reply.status(), expected);
        assert!(!reply.headers().contains_key(header::CONTENT_DISPOSITION));
    }
    let service = Arc::new(CountingFeature::default());
    let reply = staff_request(
        Arc::clone(&service),
        OFFER,
        "purchase",
        OFFER,
        "terms",
        false,
    )
    .await;
    assert_eq!(reply.status(), StatusCode::UNAUTHORIZED);
    assert_eq!(service.calls.lock().unwrap()[0]["token"], "");
}
#[tokio::test]
async fn commercial_staff_capacity_route_preserves_public_shape_without_recipient_authority() {
    for token in [true, false] {
        let service = Arc::new(CountingFeature::default());
        let body = json!({"case_id":OFFER,"subject":OWNER});
        let mut req = Request::builder()
            .method(Method::POST)
            .uri("/shop/claims/admin/cash_capacity")
            .header(header::CONTENT_TYPE, "application/json")
            .header("x-commercial-claim-key", "no-staff");
        if token {
            req = req.header(header::AUTHORIZATION, "Bearer captured-staff");
        }
        let reply = router(Arc::clone(&service))
            .finish_api(&mut Default::default())
            .oneshot(req.body(Body::from(body.to_string())).unwrap())
            .await
            .unwrap();
        assert_eq!(
            reply.status(),
            if token {
                StatusCode::OK
            } else {
                StatusCode::UNAUTHORIZED
            }
        );
        assert_eq!(service.calls.lock().unwrap()[0]["body"], body);
        if token {
            let value: Value =
                serde_json::from_slice(&to_bytes(reply.into_body(), 4096).await.unwrap()).unwrap();
            assert_eq!(value["captured_purchase_units"], "9007199254740993");
            assert!(value["remaining_purchase_capacity"].is_null());
        }
    }
}

#[tokio::test]
async fn commercial_hold_fixed_admin_post_routes_preserve_exact_body_and_staff_credentials() {
    for operation in ["hold_review", "hold_queue"] {
        for token in [false, true] {
            let service = Arc::new(CountingFeature::default());
            let body = if operation == "hold_queue" {
                json!({"version":1,"limit":100,"cursor":null})
            } else {
                json!({"version":1,"command_id":OFFER,"case_id":OFFER,"subject":OWNER,
                    "hold":{"kind":"financial_document","record_id":" original Ä / 10000000 "},
                    "expected":{"incarnation_id":OFFER,"review_version":"9007199254740993"},
                    "decision":"keep","review_scope":"entire_existing_hold","assessment":"Specific existing hold assessment",
                    "next_review_at":"2500-01-01 00:00:00.123456+00"})
            };
            let mut req = Request::builder()
                .method(Method::POST)
                .uri(format!("/shop/claims/admin/{operation}"))
                .header(header::CONTENT_TYPE, "application/json")
                .header("x-commercial-claim-key", "personal-is-not-staff");
            if token {
                req = req.header(header::AUTHORIZATION, "Bearer captured-staff");
            }
            let reply = router(Arc::clone(&service))
                .finish_api(&mut Default::default())
                .oneshot(req.body(Body::from(body.to_string())).unwrap())
                .await
                .unwrap();
            assert_eq!(
                reply.status(),
                if token {
                    StatusCode::OK
                } else {
                    StatusCode::UNAUTHORIZED
                }
            );
            let call = service.calls.lock().unwrap()[0].clone();
            assert_eq!(call["operation"], operation);
            assert_eq!(call["body"], body);
            assert_eq!(call["token"], if token { "captured-staff" } else { "" });
            if token {
                let response: Value =
                    serde_json::from_slice(&to_bytes(reply.into_body(), 8192).await.unwrap())
                        .unwrap();
                assert_eq!(response["public_body"], body);
            }
        }
    }
}

#[tokio::test]
async fn commercial_determination_status_fixed_route_preserves_lossless_projection_and_errors() {
    for (failure, expected) in [
        (None, StatusCode::OK),
        (Some("missing"), StatusCode::NOT_FOUND),
        (Some("malformed"), StatusCode::BAD_REQUEST),
        (Some("unavailable"), StatusCode::SERVICE_UNAVAILABLE),
        (Some("auth"), StatusCode::UNAUTHORIZED),
        (Some("auth-unavailable"), StatusCode::UNAUTHORIZED),
        (Some("mfa"), StatusCode::FORBIDDEN),
    ] {
        let service = Arc::new(CountingFeature {
            calls: Mutex::default(),
            failure,
        });
        let body = json!({"case_id":OFFER,"subject":OWNER,"obligation_id":OFFER,"command_id":null});
        let reply = router(Arc::clone(&service))
            .finish_api(&mut Default::default())
            .oneshot(
                Request::builder()
                    .method(Method::POST)
                    .uri("/shop/claims/admin/determination_status")
                    .header(header::CONTENT_TYPE, "application/json")
                    .header(header::AUTHORIZATION, "Bearer captured-staff")
                    .header("x-commercial-claim-key", "not-staff")
                    .body(Body::from(body.to_string()))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(reply.status(), expected);
        assert_eq!(service.calls.lock().unwrap()[0]["body"], body);
        assert_eq!(service.calls.lock().unwrap()[0]["token"], "captured-staff");
        if failure.is_none() {
            let v: Value =
                serde_json::from_slice(&to_bytes(reply.into_body(), 4096).await.unwrap()).unwrap();
            assert_eq!(v["obligation"]["units"], "9223372036854775807");
            assert_eq!(v["obligation"]["original_json"], "9007199254740993");
            assert!(v["journal"].is_null());
        }
    }
    let service = Arc::new(CountingFeature::default());
    let reply=router(service).finish_api(&mut Default::default()).oneshot(Request::builder().method(Method::POST).uri("/shop/claims/admin/determination_status").header(header::CONTENT_TYPE,"application/json").header("x-commercial-claim-key","not-staff").body(Body::from(json!({"case_id":OFFER,"subject":OWNER,"obligation_id":OFFER,"command_id":null}).to_string())).unwrap()).await.unwrap();
    assert_eq!(reply.status(), StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn commercial_retention_page_fixed_post_preserves_public_shape_and_error_statuses() {
    for (failure, expected) in [
        (None, StatusCode::OK),
        (Some("malformed"), StatusCode::BAD_REQUEST),
        (Some("auth"), StatusCode::UNAUTHORIZED),
        (Some("auth-unavailable"), StatusCode::UNAUTHORIZED),
        (Some("mfa"), StatusCode::FORBIDDEN),
        (Some("conflict"), StatusCode::CONFLICT),
        (Some("unavailable"), StatusCode::SERVICE_UNAVAILABLE),
    ] {
        let service = Arc::new(CountingFeature {
            failure,
            ..Default::default()
        });
        let b = json!({"family":"invoice_identity_reviews","limit":100,"cursor":{"protocol":1,"family":"invoice_identity_reviews","after":{"at":"infinity","number":" unsupported ","reason":"","source_key":"🙂"}}});
        let reply = router(Arc::clone(&service))
            .finish_api(&mut Default::default())
            .oneshot(
                Request::builder()
                    .method(Method::POST)
                    .uri("/shop/claims/admin/retention_page")
                    .header(header::CONTENT_TYPE, "application/json")
                    .header(header::AUTHORIZATION, "Bearer captured-staff")
                    .body(Body::from(b.to_string()))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(reply.status(), expected, "{failure:?}");
        assert_eq!(service.calls.lock().unwrap()[0]["body"], b);
        assert_eq!(service.calls.lock().unwrap()[0]["token"], "captured-staff");
        if expected == StatusCode::OK {
            let p: Value =
                serde_json::from_slice(&to_bytes(reply.into_body(), 4096).await.unwrap()).unwrap();
            assert_eq!(p["rows"], json!([]));
            assert_eq!(p["exhausted"], true);
            assert!(p["next_cursor"].is_null());
        }
    }
    let service = Arc::new(CountingFeature::default());
    let reply = router(Arc::clone(&service))
        .finish_api(&mut Default::default())
        .oneshot(
            Request::builder()
                .method(Method::POST)
                .uri("/shop/claims/admin/retention_page")
                .header(header::CONTENT_TYPE, "application/json")
                .header("x-commercial-claim-key", "personal-not-staff")
                .body(Body::from(
                    json!({"family":"archives","limit":1,"cursor":null}).to_string(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(reply.status(), StatusCode::UNAUTHORIZED);
    let reply = router(Arc::clone(&service))
        .finish_api(&mut Default::default())
        .oneshot(
            Request::builder()
                .uri("/shop/claims/admin/retention_page")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(reply.status(), StatusCode::METHOD_NOT_ALLOWED);
}

#[tokio::test]
async fn commercial_wallet_restore_route_preserves_legacy_json_and_selected_proof() {
    for malformed in [false, true] {
        let service = Arc::new(CountingFeature::default());
        let exact = json!({"command_id":OFFER,"obligation_id":OFFER,"units":"125","choose_coins":true,"ignored":{"expected_subject":null}});
        let response = router(Arc::clone(&service))
            .finish_api(&mut Default::default())
            .oneshot(
                Request::builder()
                    .method(Method::POST)
                    .uri("/shop/claims/recipient/restore_credit")
                    .header(header::CONTENT_TYPE, "application/json")
                    .header(
                        "x-commercial-claim-key",
                        if malformed {
                            bad()
                        } else {
                            text(&"k".repeat(43))
                        },
                    )
                    .header("x-moderation-capability", text(&"c".repeat(40)))
                    .header(header::AUTHORIZATION, "Bearer unrelated ordinary")
                    .body(Body::from(exact.to_string()))
                    .unwrap(),
            )
            .await
            .unwrap();
        if malformed {
            error(response, StatusCode::UNAUTHORIZED).await;
            assert!(service.calls.lock().unwrap().is_empty());
        } else {
            assert_eq!(response.status(), StatusCode::OK);
            let calls = service.calls.lock().unwrap();
            assert_eq!(calls[0]["claim"], "k".repeat(43));
            assert!(calls[0]["capability"].is_null());
            assert!(calls[0]["ordinary"].is_null());
            assert_eq!(calls[1]["body"], exact);
        }
    }
}

#[tokio::test]
async fn commercial_close_staff_settlement_paths_keep_uncertainty_and_exact_public_body() {
    for operation in [
        "cash_basis_review",
        "reserve",
        "split_cash",
        "record_cash_payment",
        "outcome",
    ] {
        for token in [false, true] {
            let service = Arc::new(CountingFeature::default());
            let body = json!({"command_id":OFFER,"case_id":OFFER,"subject":OWNER,"units":"9007199254740993","cash_units":null,"operation":"queue","original":"exact saved request"});
            let mut req = Request::builder()
                .method(Method::POST)
                .uri(format!("/shop/claims/admin/{operation}"))
                .header(header::CONTENT_TYPE, "application/json")
                .header("x-commercial-claim-key", "personal-is-not-staff");
            if token {
                req = req.header(header::AUTHORIZATION, "Bearer captured-staff");
            }
            let response = router(Arc::clone(&service))
                .finish_api(&mut Default::default())
                .oneshot(req.body(Body::from(body.to_string())).unwrap())
                .await
                .unwrap();
            assert_eq!(
                response.status(),
                if token {
                    StatusCode::BAD_REQUEST
                } else {
                    StatusCode::UNAUTHORIZED
                }
            );
            assert_eq!(response.headers()[header::CACHE_CONTROL], "no-store");
            assert_eq!(response.headers()[header::REFERRER_POLICY], "no-referrer");
            let result: Value =
                serde_json::from_slice(&to_bytes(response.into_body(), 4096).await.unwrap())
                    .unwrap();
            assert_eq!(
                result,
                json!({"detail":"Commercial request not confirmed; preserve the exact request and proof. Existing rights and original deadlines remain."})
            );
            let calls = service.calls.lock().unwrap();
            assert_eq!(calls.len(), 1);
            assert_eq!(calls[0]["operation"], operation);
            assert_eq!(calls[0]["body"], body);
            assert_eq!(calls[0]["token"], if token { "captured-staff" } else { "" });
        }
    }
}

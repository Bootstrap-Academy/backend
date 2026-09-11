use academy_di::{Provide, provider};
use academy_extern_contracts::paypal::{PaypalApiService, PaypalCaptureOrderError};
use academy_extern_impl::paypal::{PaypalApiServiceConfig, PaypalApiServiceImpl};
use academy_models::{paypal::PaypalOrderId, url::Url};
use academy_utils::assert_matches;

#[tokio::test]
async fn ok() {
    let (sut, base_url) = make_sut();

    let order_id = sut.create_order(1337).await.unwrap();
    assert_eq!(sut.get_order(&order_id).await.unwrap().status, "CREATED");

    confirm_order(&base_url, &order_id).await;
    assert_eq!(sut.get_order(&order_id).await.unwrap().status, "APPROVED");

    let request_id = uuid::Uuid::new_v4();
    let captured = sut.capture_order(&order_id, request_id).await.unwrap();
    assert_eq!(captured.captures.len(), 1);
    assert_eq!(captured.captures[0].status, "COMPLETED");
    assert_eq!(
        captured,
        sut.capture_order(&order_id, request_id).await.unwrap()
    );
    assert_eq!(captured, sut.get_order(&order_id).await.unwrap());
}

#[tokio::test]
async fn no_confirm() {
    let (sut, _) = make_sut();

    let order_id = sut.create_order(1337).await.unwrap();
    assert_eq!(sut.get_order(&order_id).await.unwrap().status, "CREATED");

    let result = sut.capture_order(&order_id, uuid::Uuid::new_v4()).await;
    assert_matches!(result, Err(PaypalCaptureOrderError::Other(_)));
}

#[tokio::test]
async fn order_not_found() {
    let (sut, _) = make_sut();

    let order_id = "asdf1234".try_into().unwrap();

    let result = sut.capture_order(&order_id, uuid::Uuid::new_v4()).await;
    assert_matches!(result, Err(PaypalCaptureOrderError::Other(_)));
}

fn make_sut() -> (PaypalApiServiceImpl, Url) {
    let config = academy_config::load().unwrap();

    provider! {
        Provider { paypal_api_service_config: PaypalApiServiceConfig, }
    }

    let mut provider = Provider {
        _cache: Default::default(),
        paypal_api_service_config: PaypalApiServiceConfig::new(
            config.paypal.base_url_override.clone(),
            config.paypal.client_id,
            config.paypal.client_secret,
        ),
    };

    (provider.provide(), config.paypal.base_url_override.unwrap())
}

async fn confirm_order(base_url: &Url, order_id: &PaypalOrderId) {
    reqwest::Client::new()
        .post(
            base_url
                .join(&format!(
                    "v2/checkout/orders/{}/confirm-payment-source",
                    **order_id
                ))
                .unwrap(),
        )
        .send()
        .await
        .unwrap()
        .error_for_status()
        .unwrap();
}

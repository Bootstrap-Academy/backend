use academy_di::{provider, Provide};
use academy_extern_contracts::paypal::{PaypalApiService, PaypalCaptureOrderError};
use academy_extern_impl::paypal::{PaypalApiServiceConfig, PaypalApiServiceImpl};
use academy_models::{paypal::PaypalOrderId, url::Url};
use academy_utils::assert_matches;
use serde::Deserialize;

#[tokio::test]
async fn ok() {
    let (sut, base_url) = make_sut();

    let order_id = sut.create_order(1337).await.unwrap();
    assert_eq!(get_order(&base_url, &order_id).await, Order::Created(1337));

    confirm_order(&base_url, &order_id).await;
    assert_eq!(
        get_order(&base_url, &order_id).await,
        Order::Confirmed(1337)
    );

    sut.capture_order(&order_id).await.unwrap();
    assert_eq!(get_order(&base_url, &order_id).await, Order::Captured);
}

#[tokio::test]
async fn no_confirm() {
    let (sut, base_url) = make_sut();

    let order_id = sut.create_order(1337).await.unwrap();
    assert_eq!(get_order(&base_url, &order_id).await, Order::Created(1337));

    let result = sut.capture_order(&order_id).await;
    assert_matches!(result, Err(PaypalCaptureOrderError::Failed));
}

#[tokio::test]
async fn order_not_found() {
    let (sut, _) = make_sut();

    let order_id = "asdf1234".try_into().unwrap();

    let result = sut.capture_order(&order_id).await;
    assert_matches!(result, Err(PaypalCaptureOrderError::Failed));
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

async fn get_order(base_url: &Url, order_id: &PaypalOrderId) -> Order {
    reqwest::Client::new()
        .get(
            base_url
                .join(&format!("v2/checkout/orders/{}", **order_id))
                .unwrap(),
        )
        .send()
        .await
        .unwrap()
        .error_for_status()
        .unwrap()
        .json::<Order>()
        .await
        .unwrap()
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

#[derive(Debug, PartialEq, Deserialize)]
#[serde(tag = "status", content = "coins")]
enum Order {
    Created(u64),
    Confirmed(u64),
    Captured,
}

use std::sync::Arc;

use academy_di::Build;
use academy_extern_contracts::paypal::{
    PaypalApiService, PaypalCaptureOrderError, PaypalCreateOrderError,
};
use academy_models::{
    paypal::{PaypalCapture, PaypalOrderId, PaypalRemoteOrder},
    url::Url,
};
use anyhow::Context;
use serde::Deserialize;
use serde_json::json;

use crate::http::HttpClient;

/// API documentation: https://developer.paypal.com/docs/api/orders/v2/
const BASE_URL: &str = "https://api-m.paypal.com";

#[derive(Debug, Clone, Build)]
pub struct PaypalApiServiceImpl {
    config: Arc<PaypalApiServiceConfig>,
    #[di(default)]
    client: HttpClient,
}

#[derive(Debug, Clone)]
pub struct PaypalApiServiceConfig {
    base_url: Url,
    client_id: String,
    client_secret: String,
}

impl PaypalApiServiceConfig {
    pub fn new(base_url_override: Option<Url>, client_id: String, client_secret: String) -> Self {
        Self {
            base_url: base_url_override.unwrap_or_else(|| BASE_URL.parse().unwrap()),
            client_id,
            client_secret,
        }
    }
}

impl PaypalApiServiceImpl {
    // Every request has a deadline so a stalled provider cannot keep the payment lock forever.
    async fn access_token(&self) -> anyhow::Result<String> {
        #[derive(Deserialize)]
        struct Token {
            access_token: String,
        }
        Ok(self
            .client
            .post(self.config.base_url.join("v1/oauth2/token")?)
            .timeout(std::time::Duration::from_secs(15))
            .basic_auth(&self.config.client_id, Some(&self.config.client_secret))
            .form(&[("grant_type", "client_credentials")])
            .send()
            .await?
            .error_for_status()?
            .json::<Token>()
            .await?
            .access_token)
    }

    async fn order_request(
        &self,
        id: &PaypalOrderId,
        request_id: Option<uuid::Uuid>,
    ) -> anyhow::Result<PaypalRemoteOrder> {
        // IDs are path segments, never caller-controlled URLs or query strings.
        anyhow::ensure!(
            !id.is_empty() && id.bytes().all(|c| c.is_ascii_alphanumeric()),
            "Invalid PayPal order ID"
        );
        let suffix = if request_id.is_some() { "/capture" } else { "" };
        let url = self
            .config
            .base_url
            .join(&format!("v2/checkout/orders/{}{suffix}", **id))?;
        let request = if let Some(request_id) = request_id {
            self.client
                .post(url)
                .header("PayPal-Request-Id", request_id.to_string())
                .header("Prefer", "return=representation")
                .json(&json!({}))
        } else {
            self.client.get(url)
        };
        let response = request
            .timeout(std::time::Duration::from_secs(30))
            .bearer_auth(self.access_token().await?)
            .send()
            .await?
            .error_for_status()?
            .json::<OrderResponse>()
            .await?;
        response.decode()
    }
}

impl PaypalApiService for PaypalApiServiceImpl {
    fn client_id(&self) -> &str {
        &self.config.client_id
    }

    async fn create_order(&self, coins: u64) -> Result<PaypalOrderId, PaypalCreateOrderError> {
        let price = format!("{}.{:02}", coins / 100, coins % 100);
        let data = json!({"intent": "CAPTURE", "purchase_units": [{"amount": {"currency_code": "EUR", "value": price}}]});
        let response = self
            .client
            .post(
                self.config
                    .base_url
                    .join("v2/checkout/orders")
                    .map_err(anyhow::Error::from)?,
            )
            .timeout(std::time::Duration::from_secs(30))
            .bearer_auth(self.access_token().await?)
            .json(&data)
            .send()
            .await
            .context("Failed to send create order request")?;
        if !response.status().is_success() {
            return Err(PaypalCreateOrderError::Failed);
        }
        #[derive(Deserialize)]
        struct Created {
            id: PaypalOrderId,
        }
        Ok(response
            .json::<Created>()
            .await
            .context("Invalid PayPal create response")?
            .id)
    }

    async fn get_order(&self, order_id: &PaypalOrderId) -> anyhow::Result<PaypalRemoteOrder> {
        self.order_request(order_id, None).await
    }

    async fn capture_order(
        &self,
        order_id: &PaypalOrderId,
        request_id: uuid::Uuid,
    ) -> Result<PaypalRemoteOrder, PaypalCaptureOrderError> {
        self.order_request(order_id, Some(request_id))
            .await
            .map_err(Into::into)
    }
}

#[derive(Deserialize)]
struct OrderResponse {
    id: PaypalOrderId,
    intent: String,
    status: String,
    purchase_units: Vec<PurchaseUnit>,
}
#[derive(Deserialize)]
struct PurchaseUnit {
    amount: Amount,
    payee: Payee,
    #[serde(default)]
    payments: Payments,
}
#[derive(Deserialize)]
struct Amount {
    currency_code: String,
    value: rust_decimal::Decimal,
}
#[derive(Deserialize)]
struct Payee {
    merchant_id: String,
}
#[derive(Default, Deserialize)]
struct Payments {
    #[serde(default)]
    captures: Vec<Capture>,
}
#[derive(Deserialize)]
struct Capture {
    id: String,
    status: String,
    amount: Amount,
    create_time: chrono::DateTime<chrono::Utc>,
}
impl OrderResponse {
    fn decode(mut self) -> anyhow::Result<PaypalRemoteOrder> {
        anyhow::ensure!(
            self.purchase_units.len() == 1,
            "Expected exactly one PayPal purchase unit"
        );
        let unit = self.purchase_units.remove(0);
        Ok(PaypalRemoteOrder {
            id: self.id,
            intent: self.intent,
            status: self.status,
            merchant_id: unit.payee.merchant_id,
            currency: unit.amount.currency_code,
            amount: unit.amount.value,
            captures: unit
                .payments
                .captures
                .into_iter()
                .map(|c| PaypalCapture {
                    id: c.id,
                    status: c.status,
                    currency: c.amount.currency_code,
                    amount: c.amount.value,
                    created_at: c.create_time,
                })
                .collect(),
        })
    }
}

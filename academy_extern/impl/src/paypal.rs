use std::sync::Arc;

use academy_di::Build;
use academy_extern_contracts::paypal::{
    PaypalApiService, PaypalCaptureOrderError, PaypalCreateOrderError,
};
use academy_models::{paypal::PaypalOrderId, url::Url};
use anyhow::Context;
use serde::Deserialize;
use serde_json::json;
use tracing::error;

use crate::http::HttpClient;

/// API documentation: https://developer.paypal.com/docs/api/orders/v2/
const BASE_URL: &str = "https://api.paypal.com";

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

impl PaypalApiService for PaypalApiServiceImpl {
    fn client_id(&self) -> &str {
        &self.config.client_id
    }

    async fn create_order(&self, coins: u64) -> Result<PaypalOrderId, PaypalCreateOrderError> {
        let price = format!("{}.{:02}", coins / 100, coins % 100);

        let data = json!({
            "intent": "CAPTURE",
            "purchase_units": [{"amount": {"currency_code": "EUR", "value": price}}],
        });

        let response = self
            .client
            .post(
                self.config
                    .base_url
                    .join("v2/checkout/orders")
                    .map_err(anyhow::Error::from)?,
            )
            .basic_auth(&self.config.client_id, Some(&self.config.client_secret))
            .json(&data)
            .send()
            .await
            .context("Failed to send create order request")?;

        let status = response.status();
        if !status.is_success() {
            let body = response.text().await;
            error!(%status, ?body, "Failed to create order");
            return Err(PaypalCreateOrderError::Failed);
        }

        let response = response
            .json::<CreateOrderResponse>()
            .await
            .map_err(|err| {
                error!(%err, "Failed to parse success response");
                PaypalCreateOrderError::Failed
            })?;

        Ok(response.id)
    }

    async fn capture_order(&self, order_id: &PaypalOrderId) -> Result<(), PaypalCaptureOrderError> {
        let response = self
            .client
            .post(
                self.config
                    .base_url
                    .join(&format!("/v2/checkout/orders/{}/capture", **order_id))
                    .map_err(anyhow::Error::from)?,
            )
            .basic_auth(&self.config.client_id, Some(&self.config.client_secret))
            .json(&json!({}))
            .send()
            .await
            .context("Failed to send capture order request")?;

        let status = response.status();
        if !status.is_success() {
            let body = response.text().await;
            error!(%status, ?body, "Failed to capture order");
            return Err(PaypalCaptureOrderError::Failed);
        }

        let response = response
            .json::<CaptureOrderResponse>()
            .await
            .map_err(|err| {
                error!(%err, "Failed to parse success response");
                PaypalCaptureOrderError::Failed
            })?;

        if response.status != "COMPLETED" {
            return Err(PaypalCaptureOrderError::Failed);
        }

        Ok(())
    }
}

#[derive(Deserialize)]
struct CreateOrderResponse {
    id: PaypalOrderId,
}

#[derive(Deserialize)]
struct CaptureOrderResponse {
    status: String,
}

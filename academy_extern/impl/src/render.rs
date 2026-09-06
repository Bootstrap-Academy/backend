use std::{sync::Arc, time::Duration};

use academy_di::Build;
use academy_extern_contracts::render::RenderApiService;
use academy_models::url::Url;
use anyhow::Context;

use crate::http::HttpClient;

#[derive(Debug, Clone, Build)]
pub struct RenderApiServiceImpl {
    config: RenderApiServiceConfig,
    #[di(default)]
    http: HttpClient,
}

#[derive(Debug, Clone)]
pub struct RenderApiServiceConfig {
    base_url: Arc<Url>,
    /// How long a single render request may take.
    ///
    /// Without one a render daemon that accepts the connection and then stops
    /// answering keeps the caller waiting for as long as the socket stays
    /// open.
    timeout: Duration,
}

impl RenderApiServiceConfig {
    pub fn new(base_url: Url, timeout: Duration) -> Self {
        Self {
            base_url: base_url.into(),
            timeout,
        }
    }
}

impl RenderApiService for RenderApiServiceImpl {
    async fn render_html_to_pdf(&self, html: String) -> anyhow::Result<Vec<u8>> {
        let url = self.config.base_url.join("html_to_pdf")?;

        self.http
            .post(url)
            .timeout(self.config.timeout)
            .body(html)
            .send()
            .await
            .context("Failed to send html_to_pdf request")?
            .error_for_status()
            .context("html_to_pdf request returned an error")?
            .bytes()
            .await
            .map(|x| x.into())
            .context("Failed to read html_to_pdf response")
    }
}

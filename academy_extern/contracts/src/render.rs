use std::future::Future;

#[cfg_attr(feature = "mock", mockall::automock)]
pub trait RenderApiService: Send + Sync + 'static {
    /// Render the given `html` source into a PDF file.
    fn render_html_to_pdf(
        &self,
        html: String,
    ) -> impl Future<Output = anyhow::Result<Vec<u8>>> + Send;
}

#[cfg(feature = "mock")]
impl MockRenderApiService {
    pub fn with_render_html_to_pdf(mut self, html: String, result: Vec<u8>) -> Self {
        self.expect_render_html_to_pdf()
            .once()
            .with(mockall::predicate::eq(html))
            .return_once(|_| Box::pin(std::future::ready(Ok(result))));
        self
    }
}

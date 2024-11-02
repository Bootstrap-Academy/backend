use std::future::Future;

#[cfg_attr(feature = "mock", mockall::automock)]
pub trait RenderPdfService: Send + Sync + 'static {
    /// Render the given `html` source into a PDF file.
    fn render(&self, html: &str) -> impl Future<Output = anyhow::Result<Vec<u8>>> + Send;
}

#[cfg(feature = "mock")]
impl MockRenderPdfService {
    pub fn with_render(mut self, html: String, result: Vec<u8>) -> Self {
        self.expect_render()
            .once()
            .with(mockall::predicate::eq(html))
            .return_once(|_| Box::pin(std::future::ready(Ok(result))));
        self
    }
}

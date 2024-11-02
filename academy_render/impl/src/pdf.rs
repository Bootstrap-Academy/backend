use std::{path::PathBuf, process::Stdio, sync::Arc};

use academy_di::Build;
use academy_render_contracts::pdf::RenderPdfService;
use academy_utils::trace_instrument;
use anyhow::{anyhow, Context};
use tempfile::tempdir;

#[derive(Debug, Clone, Build)]
pub struct RenderPdfServiceImpl {
    pub config: RenderPdfServiceConfig,
}

#[derive(Debug, Clone)]
pub struct RenderPdfServiceConfig {
    pub chrome_bin: Arc<PathBuf>,
}

impl RenderPdfService for RenderPdfServiceImpl {
    #[trace_instrument(skip(self))]
    async fn render(&self, html: &str) -> anyhow::Result<Vec<u8>> {
        let dir = tempdir().context("Failed to create tempdir")?;
        let index_path = dir.path().join("index.html");
        let output_path = dir.path().join("output.pdf");

        tokio::fs::write(&index_path, html)
            .await
            .context("Failed to write html source")?;

        tokio::process::Command::new(&*self.config.chrome_bin)
            .arg("--headless")
            .arg("--disable-gpu")
            .arg("--no-pdf-header-footer")
            .arg(format!("--print-to-pdf={}", output_path.display()))
            .arg(index_path)
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .context("Failed to spawn pdf render process")?
            .wait()
            .await
            .context("Failed to await pdf render process")?
            .success()
            .then_some(())
            .ok_or_else(|| anyhow!("Failed to render pdf"))?;

        let pdf = tokio::fs::read(output_path)
            .await
            .context("Failed to read pdf output")?;

        Ok(pdf)
    }
}

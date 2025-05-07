use std::{
    net::{IpAddr, SocketAddr},
    path::{Path, PathBuf},
    process::Stdio,
    sync::Arc,
};

use academy_utils::{academy_version, bin_name};
use anyhow::{Context, anyhow};
use axum::{
    Router,
    extract::State,
    http::{StatusCode, header::CONTENT_TYPE},
    response::{IntoResponse, Response},
    routing,
};
use clap::{CommandFactory, Parser};
use clap_complete::CompleteEnv;
use tempfile::tempdir;
use tokio::net::TcpListener;
use tracing::{error, info};

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    CompleteEnv::with_factory(Cli::command).complete();

    tracing_subscriber::fmt::init();

    let cli = Cli::parse();

    let router = Router::new()
        .route("/html_to_pdf", routing::post(handler))
        .with_state(Arc::new(Config {
            chrome_bin: cli.chrome_bin,
        }));

    let addr = SocketAddr::new(cli.host, cli.port);
    let listener = TcpListener::bind(addr)
        .await
        .with_context(|| format!("Failed to bind to {addr}"))?;

    info!(
        "Starting Render Daemon API server on http://{}",
        listener.local_addr()?
    );
    axum::serve(listener, router)
        .await
        .context("Failed to start HTTP server")
}

struct Config {
    chrome_bin: PathBuf,
}

async fn handler(config: State<Arc<Config>>, html: String) -> Response {
    match render(&config.chrome_bin, &html).await {
        Ok(pdf) => ([(CONTENT_TYPE, "application/pdf")], pdf).into_response(),
        Err(err) => {
            error!("{err:#}");
            StatusCode::INTERNAL_SERVER_ERROR.into_response()
        }
    }
}

async fn render(chrome_bin: &Path, html: &str) -> anyhow::Result<Vec<u8>> {
    let dir = tempdir().context("Failed to create tempdir")?;
    let index_path = dir.path().join("index.html");
    let output_path = dir.path().join("output.pdf");

    tokio::fs::write(&index_path, html)
        .await
        .context("Failed to write html source")?;

    tokio::process::Command::new(chrome_bin)
        .arg("--headless")
        .arg("--no-first-run")
        .arg("--no-default-browser-check")
        .arg("--no-sandbox")
        .arg("--disable-setuid-sandbox")
        .arg("--disable-gpu")
        .arg("--disable-dev-shm-usage")
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

    tokio::fs::read(output_path)
        .await
        .context("Failed to read pdf output")
}

#[derive(Debug, Parser)]
#[command(name = bin_name!(), version = academy_version())]
struct Cli {
    #[arg(long, default_value = "127.0.0.1")]
    host: IpAddr,
    #[arg(long)]
    port: u16,
    #[arg(long, env)]
    chrome_bin: PathBuf,
}

use std::{path::Path, time::Duration};

use anyhow::Context;
use reqwest::{
    Client,
    header::{AUTHORIZATION, HeaderMap, HeaderValue},
};
use serde::Deserialize;
use uuid::Uuid;

use super::{model::issue_marker, storage::Receipt};

pub const REPOSITORY: &str = "Bootstrap-Academy/Bootstrap-Academy";
const ISSUES_ENDPOINT: &str =
    "https://api.github.com/repos/Bootstrap-Academy/Bootstrap-Academy/issues";
const MAX_RESPONSE_BYTES: usize = 2 * 1024 * 1024;

pub struct Github {
    client: Client,
    endpoint: String,
}

#[derive(Deserialize)]
struct Issue {
    html_url: String,
    number: u64,
    body: Option<String>,
    pull_request: Option<serde_json::Value>,
}

impl Issue {
    fn verified_url(self, marker: Uuid) -> Option<String> {
        let expected = format!("https://github.com/{REPOSITORY}/issues/{}", self.number);
        (self.number > 0
            && self.html_url == expected
            && self.pull_request.is_none()
            && self
                .body
                .as_deref()
                .is_some_and(|body| body.ends_with(&issue_marker(marker))))
        .then_some(expected)
    }
}

impl Github {
    pub fn from_token_file(path: &Path) -> anyhow::Result<Self> {
        let token =
            std::fs::read_to_string(path).context("could not read feedback GitHub token file")?;
        Self::new(token.trim(), ISSUES_ENDPOINT)
    }

    fn new(token: &str, endpoint: &str) -> anyhow::Result<Self> {
        anyhow::ensure!(!token.is_empty(), "empty feedback GitHub token");
        let mut auth = HeaderValue::from_str(&format!("Bearer {token}"))
            .map_err(|_| anyhow::anyhow!("invalid feedback token format"))?;
        auth.set_sensitive(true);
        let mut headers = HeaderMap::new();
        headers.insert(AUTHORIZATION, auth);
        headers.insert(
            "Accept",
            HeaderValue::from_static("application/vnd.github+json"),
        );
        headers.insert(
            "X-GitHub-Api-Version",
            HeaderValue::from_static("2022-11-28"),
        );
        let client = Client::builder()
            .user_agent("Bootstrap-Academy-Feedback")
            .default_headers(headers)
            .redirect(reqwest::redirect::Policy::none())
            .retry(reqwest::retry::never())
            .connect_timeout(Duration::from_secs(5))
            .timeout(Duration::from_secs(15))
            .build()?;
        Ok(Self {
            client,
            endpoint: endpoint.into(),
        })
    }

    /// No retry, including library-level retries. Failure of any kind is
    /// ambiguous after the durable receipt has been written.
    pub async fn create(&self, title: &str, body: &str, marker: Uuid) -> Option<String> {
        let response = self
            .client
            .post(&self.endpoint)
            .json(&serde_json::json!({"title": title, "body": body}))
            .send()
            .await
            .ok()?;
        if response.status() != reqwest::StatusCode::CREATED {
            return None;
        }
        let bytes = bounded_response(response).await?;
        serde_json::from_slice::<Issue>(&bytes)
            .ok()?
            .verified_url(marker)
    }

    /// Search the fixed repository directly, including closed issues, instead
    /// of trusting eventually indexed search. The scan is bounded. Absence,
    /// truncation and errors all remain pending and never cause another POST.
    pub async fn reconcile(&self, receipt: &Receipt) -> Option<String> {
        let since = chrono::DateTime::from_timestamp(receipt.created_at.saturating_sub(60), 0)?
            .to_rfc3339();
        for page in 1..=10 {
            let mut url = reqwest::Url::parse(&self.endpoint).ok()?;
            url.query_pairs_mut().extend_pairs([
                ("state", "all"),
                ("sort", "created"),
                ("direction", "desc"),
                ("since", &since),
                ("per_page", "100"),
                ("page", &page.to_string()),
            ]);
            let response = self.client.get(url).send().await.ok()?;
            if !response.status().is_success() {
                return None;
            }
            let bytes = bounded_response(response).await?;
            let issues: Vec<Issue> = serde_json::from_slice(&bytes).ok()?;
            let count = issues.len();
            for issue in issues {
                if let Some(url) = issue.verified_url(receipt.marker) {
                    return Some(url);
                }
            }
            if count < 100 {
                break;
            }
        }
        None
    }

    #[cfg(test)]
    pub fn test_client(endpoint: &str) -> Self {
        Self::new("test-only-token", endpoint).unwrap()
    }
}

async fn bounded_response(mut response: reqwest::Response) -> Option<Vec<u8>> {
    if response
        .content_length()
        .is_some_and(|size| size > MAX_RESPONSE_BYTES as u64)
    {
        return None;
    }
    let mut bytes = Vec::new();
    while let Some(chunk) = response.chunk().await.ok()? {
        if bytes.len() + chunk.len() > MAX_RESPONSE_BYTES {
            return None;
        }
        bytes.extend_from_slice(&chunk);
    }
    Some(bytes)
}

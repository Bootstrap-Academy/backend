use std::sync::LazyLock;

use aide::redoc::Redoc;
use axum::{
    Router,
    response::{Html, IntoResponse, Response},
    routing,
};

static REDOC_HTML: LazyLock<String> = LazyLock::new(|| {
    let redoc = Redoc::new("/openapi.json").with_title("Redoc - Bootstrap Academy");
    redoc.html().replacen(
        "</head>",
        r#"  <link rel="icon" href="https://static.bootstrap.academy/logo.svg">
  </head>"#,
        1,
    )
});

pub fn router() -> Router<()> {
    Router::new().route("/redoc", routing::get(serve_redoc))
}

async fn serve_redoc() -> Response {
    Html(REDOC_HTML.as_str()).into_response()
}

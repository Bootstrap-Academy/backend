use std::sync::LazyLock;

use academy_assets::swagger_ui::{SWAGGER_UI_BUNDLE_JS, SWAGGER_UI_CSS};
use axum::{
    Router,
    response::{Html, IntoResponse, Response},
    routing,
};

static SWAGGER_UI_HTML: LazyLock<String> = LazyLock::new(|| {
    academy_assets::swagger_ui::SWAGGER_UI_HTML
        .replace("{{SWAGGER_UI_CSS}}", SWAGGER_UI_CSS)
        .replace("{{SWAGGER_UI_JS}}", SWAGGER_UI_BUNDLE_JS)
});

pub fn router() -> Router<()> {
    Router::new().route("/docs", routing::get(serve_swagger_ui))
}

async fn serve_swagger_ui() -> Response {
    Html(SWAGGER_UI_HTML.as_str()).into_response()
}

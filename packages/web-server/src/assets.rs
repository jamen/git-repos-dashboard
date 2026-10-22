//! Static assets compiled into the binary.
//!
//! Nothing here is read from disk at runtime: the client bundle and the files
//! served under `/assets` are embedded at build time.

use axum::{
    Router,
    http::{HeaderValue, StatusCode, header},
    response::{IntoResponse, Response},
    routing::get,
};

pub struct Assets {
    pub css: &'static str,
    pub js: &'static str,
    favicon: &'static [u8],
    robots: &'static str,
}

impl Assets {
    pub const fn new() -> Self {
        Self {
            css: include_str!("../../web-client/dist/main.css"),
            js: include_str!("../../web-client/dist/main.js"),
            favicon: include_bytes!("../assets/favicon.svg"),
            robots: include_str!("../assets/robots.txt"),
        }
    }

    /// Routes for the files served under `/assets`.
    pub fn router<S>() -> Router<S>
    where
        S: Clone + Send + Sync + 'static,
    {
        Router::new()
            .route("/favicon.svg", get(favicon))
            .route("/robots.txt", get(robots))
    }
}

impl Default for Assets {
    fn default() -> Self {
        Self::new()
    }
}

pub static ASSETS: Assets = Assets::new();

async fn favicon() -> Response {
    send(ASSETS.favicon, "image/svg+xml")
}

async fn robots() -> Response {
    send(ASSETS.robots.as_bytes(), "text/plain; charset=utf-8")
}

fn send(bytes: &'static [u8], content_type: &'static str) -> Response {
    (
        StatusCode::OK,
        [
            (header::CONTENT_TYPE, HeaderValue::from_static(content_type)),
            (
                header::CACHE_CONTROL,
                HeaderValue::from_static("public, max-age=3600"),
            ),
        ],
        bytes,
    )
        .into_response()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn everything_is_embedded() {
        assert!(!ASSETS.css.is_empty());
        assert!(!ASSETS.js.is_empty());
        assert!(ASSETS.favicon.starts_with(b"<svg"));
        assert!(ASSETS.robots.contains("User-agent"));
    }
}

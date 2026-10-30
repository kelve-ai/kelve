//! The web app, built into the binary. Paths that aren't files get the app's
//! page, which reads its own route from the address.

use axum::http::{StatusCode, Uri, header};
use axum::response::{IntoResponse, Response};
use rust_embed::RustEmbed;
use sha2::{Digest, Sha256};

#[derive(RustEmbed)]
#[folder = "../../web/dist"]
#[allow_missing = true]
struct Assets;

const MISSING: &str = "<!doctype html><meta charset=utf-8><title>kelve</title>\
<body style='font:16px system-ui;max-width:32rem;margin:15vh auto;padding:0 1rem'>\
<h1 style='font-size:1.25rem'>The web app isn’t built into this server</h1>\
<p>Build it with <code>bun install &amp;&amp; bun run build</code> in <code>web/</code>, then build kelve again. \
Or run the web app’s dev server, which talks to this one.</p>";

/// The build of the page served: a print of the app's page, which names
/// every built file by its contents, so it changes whenever the app does.
pub fn build() -> &'static str {
    static BUILD: std::sync::OnceLock<String> = std::sync::OnceLock::new();
    BUILD.get_or_init(|| {
        Assets::get("index.html")
            .map(|page| {
                Sha256::digest(&page.data)
                    .iter()
                    .take(8)
                    .map(|b| format!("{b:02x}"))
                    .collect()
            })
            .unwrap_or_default()
    })
}

pub async fn serve(uri: Uri) -> Response {
    let path = uri.path().trim_start_matches('/');
    if !path.is_empty()
        && let Some(file) = Assets::get(path)
    {
        // Built assets have hashed names, so they never change under the same name.
        let cache = if path.starts_with("assets/") {
            "public, max-age=31536000, immutable"
        } else {
            "public, max-age=3600"
        };
        // Text is UTF-8, like the license notices, whose names aren't all ASCII.
        let kind = file.metadata.mimetype();
        let kind = if kind.starts_with("text/") && !kind.contains("charset") {
            format!("{kind}; charset=utf-8")
        } else {
            kind.to_string()
        };
        return (
            [
                (header::CONTENT_TYPE, kind),
                (header::CACHE_CONTROL, cache.to_string()),
            ],
            file.data,
        )
            .into_response();
    }
    // A built file that isn't here, like code from an older version, is missing, not a page.
    if path.starts_with("assets/") {
        return (StatusCode::NOT_FOUND, "Not found").into_response();
    }
    match Assets::get("index.html") {
        Some(page) => (
            [
                (header::CONTENT_TYPE, "text/html; charset=utf-8"),
                (header::CACHE_CONTROL, "no-cache"),
            ],
            page.data,
        )
            .into_response(),
        None => (
            StatusCode::SERVICE_UNAVAILABLE,
            [(header::CONTENT_TYPE, "text/html; charset=utf-8")],
            MISSING,
        )
            .into_response(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn built_files_that_have_gone_are_missing_not_the_page() {
        let response = serve(Uri::from_static("/assets/Connect-gone.js")).await;
        assert_eq!(response.status(), StatusCode::NOT_FOUND);
    }
}

//! Public static landing page. No handler receives a fragment or talks to a route.
use axum::{Router, http::StatusCode, response::IntoResponse, routing::get};

const POLICY: &str = "default-src 'none'; script-src 'self' 'wasm-unsafe-eval'; worker-src 'self'; connect-src 'self'; style-src 'self'; frame-ancestors 'none'; base-uri 'none'; form-action 'none'";

pub(super) fn router() -> Router {
    macro_rules! assets {
        ($($path:literal => ($file:literal, $mime:literal)),+) => {
            Router::new()$(.route($path, get(|| async {
                (StatusCode::OK, [
                    ("content-type", $mime),
                    ("content-security-policy", POLICY),
                    ("cache-control", "no-store"),
                    ("referrer-policy", "no-referrer"),
                    ("x-frame-options", "DENY"),
                    ("x-content-type-options", "nosniff"),
                    ("x-robots-tag", "noindex, nofollow"),
                ], include_bytes!(concat!("viewer/assets/", $file)).as_slice()).into_response()
            })))+
        }
    }
    assets!(
        "/share/" => ("index.html", "text/html; charset=utf-8"),
        "/share/viewer.js" => ("viewer.js", "text/javascript"),
        "/share/viewer.css" => ("viewer.css", "text/css"),
        "/share/worker.js" => ("worker.js", "text/javascript"),
        "/share/guest.js" => ("guest.js", "text/javascript"),
        "/share/guest_bg.wasm" => ("guest_bg.wasm", "application/wasm")
    )
}

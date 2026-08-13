//! Native TLS HTTP client for providers that reject rustls TLS fingerprints.
//!
//! `ani-desk-core` uses rustls by default, which Cloudflare-style bot
//! management on some sites (e.g. AniDB) challenges. System TLS stacks
//! (SecureTransport on macOS, OpenSSL on Linux) present widely accepted
//! fingerprints, so this crate exists so those providers can use them.

pub use reqwest;

use reqwest::header::{HeaderMap, HeaderValue, USER_AGENT};

pub fn client(user_agent: &str) -> reqwest::Client {
    let mut headers = HeaderMap::new();
    headers.insert(
        USER_AGENT,
        HeaderValue::from_str(user_agent).expect("user agent must be a valid header value"),
    );
    reqwest::Client::builder()
        .default_headers(headers)
        .redirect(reqwest::redirect::Policy::limited(8))
        .timeout(std::time::Duration::from_secs(30))
        .build()
        .expect("failed to build secure HTTP client")
}

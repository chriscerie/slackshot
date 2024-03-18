use std::collections::HashSet;

use reqwest::header::{HeaderMap, AUTHORIZATION};
use serde::{Deserialize, Serialize};

const REQUIRED_SCOPES: [&str; 9] = [
    "admin.usergroups:read",
    "channels:history",
    "channels:read",
    "groups:history",
    "groups:read",
    "im:history",
    "im:read",
    "mpim:history",
    "mpim:read",
];

#[derive(Deserialize, Serialize, Debug, Clone)]
struct TestResponse {
    ok: bool,
    error: Option<String>,
}

pub async fn validate_token(token: &str) -> Result<(), String> {
    let mut headers = HeaderMap::new();
    headers.insert(AUTHORIZATION, format!("Bearer {token}").parse().unwrap());

    let client = reqwest::Client::new();
    let response = client
        .get("https://slack.com/api/auth.test")
        .headers(headers)
        .send()
        .await
        .map_err(|e| e.to_string())?;

    let oauth_scopes = response
        .headers()
        .get("x-oauth-scopes")
        .ok_or_else(|| "x-oauth-scopes header not found".to_string())?
        .to_str()
        .map_err(|e| e.to_string())?
        .split(',')
        .map(str::trim)
        .collect::<HashSet<_>>();

    let required_scopes = REQUIRED_SCOPES.into();

    if !oauth_scopes.is_superset(&required_scopes) {
        let missing_scopes = required_scopes
            .difference(&oauth_scopes)
            .collect::<HashSet<_>>();

        return Err(format!("Missing scopes {missing_scopes:?}"));
    }

    let test_response = response
        .json::<TestResponse>()
        .await
        .map_err(|e| e.to_string())?;

    test_response.ok.then_some(()).ok_or_else(|| {
        test_response.error.map_or_else(
            || "Could not validate auth token".to_string(),
            |e| e.to_string(),
        )
    })
}

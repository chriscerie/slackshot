use reqwest::header::{HeaderMap, AUTHORIZATION};
use serde::{Deserialize, Serialize};

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

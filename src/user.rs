use reqwest::header::HeaderMap;
use serde::{Deserialize, Serialize};

#[derive(Deserialize, Serialize, Debug, Clone)]
pub struct Profile {
    pub real_name_normalized: String,
    pub display_name_normalized: String,
}

#[derive(Deserialize, Serialize, Debug, Clone)]
pub struct User {
    profile: Profile,
}

#[derive(Deserialize, Serialize, Debug, Clone)]
pub struct InfoResponse {
    pub ok: bool,
    pub user: Option<User>,
}

pub async fn get_user_display_name(headers: HeaderMap, user_id: &str) -> Result<String, String> {
    let client = reqwest::Client::new();

    let info_response = client
        .get("https://slack.com/api/users.info")
        .headers(headers.clone())
        .query(&[("user", user_id)])
        .send()
        .await
        .map_err(|e| e.to_string())?;

    let response = info_response
        .json::<InfoResponse>()
        .await
        .map_err(|e| e.to_string())?;

    Ok(response.user.map_or("UNKNOWN".to_string(), |user| {
        if user.profile.display_name_normalized.is_empty() {
            user.profile.real_name_normalized
        } else {
            user.profile.display_name_normalized
        }
    }))
}

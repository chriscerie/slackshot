use indicatif::ProgressBar;
use reqwest::header::HeaderMap;

use crate::conversations::{HistoryResponse, Message};

pub async fn get_messages(
    headers: HeaderMap,
    channel_id: &str,
    channel_name: &str,
    pb: &ProgressBar,
) -> Result<Vec<Message>, String> {
    let client = reqwest::Client::new();
    let mut all_messages = Vec::new();
    let mut next_cursor: Option<String> = None;

    let mut page = 1;

    loop {
        let limit = 999.to_string();
        let mut params = vec![("channel", channel_id), ("limit", &limit)];
        if let Some(cursor) = &next_cursor {
            params.push(("cursor", cursor));
        }

        let channel_history_response = client
            .get("https://slack.com/api/conversations.history")
            .headers(headers.clone())
            .query(&params)
            .send()
            .await
            .map_err(|e| e.to_string())?;

        let channel_history: HistoryResponse = channel_history_response
            .json()
            .await
            .map_err(|e| e.to_string())?;

        if let Some(messages) = channel_history.messages {
            all_messages.extend(messages);
        }

        let Some(response_metadata) = &channel_history.response_metadata else {
            break;
        };

        match response_metadata.next_cursor.as_str() {
            "" => break,
            cursor => next_cursor = Some(cursor.to_string()),
        }

        page += 1;
        pb.set_message(format!(": #{channel_name} ({page})"));
    }

    Ok(all_messages)
}

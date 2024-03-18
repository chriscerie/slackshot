use serde::{Deserialize, Serialize};

#[derive(Deserialize, Serialize, Debug, Clone)]
pub struct Channel {
    pub id: String,
    pub name: String,
    pub is_channel: bool,
    pub is_group: bool,
    pub is_im: bool,
    pub is_private: bool,
    pub created: i64,
    pub is_archived: bool,
    pub num_members: i64,
}

#[derive(Deserialize, Serialize, Debug, Clone)]
pub struct ListResponse {
    pub ok: bool,
    pub channels: Vec<Channel>,
}

#[derive(Deserialize, Serialize, Debug, Clone)]
pub struct Message {
    /// Might not exist for bots
    pub user: Option<String>,

    /// Timestamp
    pub ts: String,

    pub text: String,

    /// Only present when reply_count > 0
    pub reply_count: Option<i64>,
}

#[derive(Deserialize, Serialize, Debug, Clone)]
pub struct HistoryResponse {
    pub ok: bool,
    pub error: Option<String>,
    pub messages: Option<Vec<Message>>,
    pub has_more: Option<bool>,
    pub pin_count: Option<i64>,
}

#![warn(clippy::pedantic)]
#![allow(clippy::too_many_lines)]
#![allow(clippy::struct_excessive_bools)]
#![allow(clippy::struct_field_names)]
#![allow(clippy::cast_possible_truncation)]

use auth::validate_token;
use channel::get_messages;
use chrono::{DateTime, TimeZone, Utc};
use chrono_tz::America::Los_Angeles;
use conversations::{Channel, HistoryResponse, Message};
use derive_more::Display;
use dialoguer::{theme::ColorfulTheme, Password, Select};
use dirs::download_dir;
use indicatif::{ProgressBar, ProgressStyle};
use reqwest::header::{HeaderMap, AUTHORIZATION};
use serde::{Deserialize, Serialize};
use serde_json::to_writer_pretty;
use std::{
    collections::HashMap,
    fs::{self, File},
    io::{BufWriter, Write},
    process,
    time::SystemTime,
};
use strum::{EnumProperty, IntoEnumIterator};
use strum_macros::{EnumIter, EnumProperty};
use tempfile::tempdir;
use terminal::{create_new_pb, get_formatted_left_output, OutputColor};
use user::get_user_display_name;

use crate::conversations::ListResponse;

mod args;
mod auth;
mod channel;
mod conversations;
mod terminal;
mod user;

#[derive(Deserialize, Serialize, Debug, Clone)]
struct MessageExport {
    message: Message,

    /// Replies in thread in reverse chronological order
    replies: Vec<Message>,
}

#[derive(Deserialize, Serialize, Debug, Clone)]
struct ChannelExport {
    metadata: Channel,

    /// Messages in reverse chronological order
    messages: Vec<MessageExport>,
}

async fn start(pb: &ProgressBar) -> Result<(), String> {
    let start_time = std::time::Instant::now();

    let name_option = prompt_name_option();

    let token = prompt_password_input("Enter OAuth token: ");

    pb.set_message(": token");

    validate_token(&token).await.map_err(|e| e.to_string())?;

    pb.println(format!(
        "{} token scopes",
        get_formatted_left_output("Validated", &OutputColor::Green),
    ));
    pb.inc(1);

    pb.set_message(": channels");

    let client = reqwest::Client::new();
    let mut headers = HeaderMap::new();
    headers.insert(AUTHORIZATION, format!("Bearer {token}").parse().unwrap());

    let channels_response = client
        .get("https://slack.com/api/conversations.list")
        .headers(headers.clone())
        .query(&[("types", "public_channel,private_channel")])
        .send()
        .await
        .map_err(|e| e.to_string())?;

    let channels = channels_response
        .json::<ListResponse>()
        .await
        .map_err(|e| e.to_string())?;

    pb.println(format!(
        "{} {} channels",
        get_formatted_left_output("Found", &OutputColor::Green),
        channels.channels.len()
    ));
    pb.inc(1);
    pb.inc_length(channels.channels.len() as u64);

    pb.println(format!(
        "{} temporary output directory",
        get_formatted_left_output("Created", &OutputColor::Green)
    ));
    pb.inc(1);

    // Put output in tempdir first to gracefully handle user cancelling halfway through
    let temp_dir = tempdir().map_err(|e| format!("Failed to create temp dir: {e}"))?;

    for channel in &channels.channels {
        pb.set_message(format!(": #{}", channel.name));

        let messages = get_messages(headers.clone(), &channel.id, &channel.name, pb).await?;

        fs::create_dir_all(temp_dir.path().join("channels"))
            .map_err(|e| format!("Failed to create channels directory: {e}"))?;

        let output_file_json = File::create(
            temp_dir
                .path()
                .join(format!("channels/{}.json", channel.name)),
        )
        .map_err(|e| {
            format!(
                "Failed to create output file for channel #{}: {}",
                channel.name, e
            )
        })?;

        let output_file_txt = File::create(
            temp_dir
                .path()
                .join(format!("channels/{}.txt", channel.name)),
        )
        .map_err(|e| {
            format!(
                "Failed to create output file for channel #{}: {}",
                channel.name, e
            )
        })?;

        let mut messages_with_replies = Vec::new();

        for message in messages {
            if message.reply_count.is_some() {
                let replies_response = client
                    .get("https://slack.com/api/conversations.replies")
                    .headers(headers.clone())
                    .query(&[("channel", &channel.id), ("ts", &message.ts)])
                    .send()
                    .await
                    .map_err(|e| e.to_string())?;

                let replies = replies_response
                    .json::<HistoryResponse>()
                    .await
                    .map_err(|e| e.to_string())?;

                if let Some(mut replies) = replies.messages {
                    messages_with_replies.push(MessageExport {
                        message: message.clone(),
                        replies: replies.split_off(1),
                    });
                }
            } else {
                messages_with_replies.push(MessageExport {
                    message: message.clone(),
                    replies: Vec::new(),
                });
            }
        }

        // Messages are returned in reverse chronological order
        messages_with_replies.sort_by_key(|message| message.message.ts.clone());

        let channel_export = ChannelExport {
            metadata: channel.clone(),
            messages: messages_with_replies,
        };

        let writer_json = BufWriter::new(output_file_json);
        let mut writer_txt = BufWriter::new(output_file_txt);

        to_writer_pretty(writer_json, &channel_export).map_err(|e| {
            format!(
                "Failed to write to output file for channel {}: {}",
                channel.name, e
            )
        })?;

        let mut user_id_to_name: HashMap<String, String> = HashMap::new();
        let mut processing_index = 0;

        for message in &channel_export.messages {
            processing_index += 1;
            pb.set_message(format!(
                ": #{} (message {processing_index} / {})",
                channel.name,
                channel_export.messages.len()
            ));

            let user = if name_option == NameOption::UserId {
                message
                    .message
                    .user
                    .clone()
                    .unwrap_or("UNKNOWN".to_string())
            } else if let Some(user_id) = &message.message.user {
                if let Some(name) = user_id_to_name.get(user_id) {
                    name.clone()
                } else {
                    let user_name = get_user_display_name(headers.clone(), user_id)
                        .await
                        .map_err(|e| {
                            format!("Failed to get user display name for user ID {user_id}: {e}")
                        })?;

                    user_id_to_name.insert(user_id.clone(), user_name.clone());

                    user_name
                }
            } else {
                "UNKNOWN".to_string()
            };

            writeln!(
                writer_txt,
                "{user} - {}\n{}\n",
                format_timestamp(&message.message.ts),
                message.message.text
            )
            .map_err(|e| {
                format!(
                    "Failed to write to output file for channel {}: {}",
                    channel.name, e
                )
            })?;

            for reply in &message.replies {
                let user = if name_option == NameOption::DisplayName {
                    message
                        .message
                        .user
                        .clone()
                        .unwrap_or("UNKNOWN".to_string())
                } else if let Some(user_id) = &message.message.user {
                    if let Some(name) = user_id_to_name.get(user_id) {
                        name.clone()
                    } else {
                        let user_name = get_user_display_name(headers.clone(), user_id)
                            .await
                            .map_err(|e| {
                                format!(
                                    "Failed to get user display name for user ID {user_id}: {e}"
                                )
                            })?;

                        user_id_to_name.insert(user_id.clone(), user_name.clone());

                        user_name
                    }
                } else {
                    "UNKNOWN".to_string()
                };

                writeln!(
                    writer_txt,
                    "    {user} - {}\n    {}\n",
                    format_timestamp(&reply.ts),
                    reply.text
                )
                .map_err(|e| {
                    format!(
                        "Failed to write to output file for channel {}: {}",
                        channel.name, e
                    )
                })?;
            }
        }

        pb.println(format!(
            "{} #{}",
            get_formatted_left_output("Processed", &OutputColor::Green),
            channel.name
        ));
        pb.inc(1);
    }

    let now = SystemTime::now();
    let timestamp = now
        .duration_since(SystemTime::UNIX_EPOCH)
        .unwrap()
        .as_secs();

    let downloads_dir = download_dir().unwrap();
    let final_output_dir = downloads_dir.join(format!("slackshot_{timestamp}"));

    fs::rename(temp_dir.path(), &final_output_dir)
        .map_err(|e| format!("Failed to move output directory to Downloads folder: {e}"))?;

    pb.println(format!(
        "{} workspace snapshot ({})",
        get_formatted_left_output("Exported", &OutputColor::Green),
        final_output_dir.display()
    ));

    pb.set_style(
        ProgressStyle::default_bar()
            .template(&format!(
                "{{msg}} in {}s",
                (start_time.elapsed().as_secs_f32() * 10.0).round() / 10.0
            ))
            .unwrap(),
    );
    pb.finish_with_message(get_formatted_left_output("Finished", &OutputColor::Green));

    Ok(())
}

fn main() {
    let pb = &create_new_pb(5, "Running");

    let rt = tokio::runtime::Runtime::new().unwrap();
    rt.block_on(async {
        let _ = start(pb).await.map_err(|e| {
            pb.abandon();

            eprintln!(
                "{} {}",
                get_formatted_left_output("Error", &OutputColor::Red),
                e
            );

            process::exit(1);
        });
    });
}

fn prompt_password_input(prompt: &str) -> String {
    let password = Password::with_theme(&ColorfulTheme::default())
        .with_prompt(prompt)
        .interact()
        .unwrap_or_default();

    password
}

#[derive(Display, EnumIter, Clone, EnumProperty, PartialEq)]
pub enum NameOption {
    #[strum(serialize = "User ID", props(Friendly = "User ID"))]
    UserId,

    #[strum(serialize = "Display Name", props(Friendly = "Display Name"))]
    DisplayName,
}

fn prompt_name_option() -> NameOption {
    let options = NameOption::iter().collect::<Vec<_>>();
    let options_friendly = options
        .iter()
        .map(|option| option.get_str("Friendly").unwrap())
        .collect::<Vec<_>>();

    let selection = Select::with_theme(&ColorfulTheme::default())
        .with_prompt("Select name display")
        .default(0)
        .items(&options_friendly)
        .interact()
        .unwrap();

    options[selection].clone()
}

fn format_timestamp(timestamp: &str) -> String {
    let timestamp = timestamp.parse::<f64>().unwrap();
    let datetime = DateTime::<Utc>::from_timestamp(timestamp as i64, 0);

    match datetime {
        Some(datetime) => {
            let naive_datetime = datetime.naive_utc();

            Los_Angeles
                .from_utc_datetime(&naive_datetime)
                .format("%Y-%m-%d %I:%M %p")
                .to_string()
        }
        None => "Invalid timestamp".to_string(),
    }
}

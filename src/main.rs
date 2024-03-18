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
use dirs::download_dir;
use indicatif::{ProgressBar, ProgressStyle};
use reqwest::header::{HeaderMap, AUTHORIZATION};
use serde::{Deserialize, Serialize};
use serde_json::to_writer_pretty;
use std::{
    fs::{self, File},
    io::{self, BufWriter, Write},
    process::{self},
    time::SystemTime,
};
use tempfile::tempdir;
use terminal::{create_new_pb, get_formatted_left_output, OutputColor};

use crate::conversations::ListResponse;

mod args;
mod auth;
mod channel;
mod conversations;
mod terminal;

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

    let token = prompt_input("Enter OAuth token: ").map_err(|e| e.to_string())?;

    pb.set_message(": token");

    validate_token(&token).await.map_err(|e| e.to_string())?;

    pb.println(format!(
        "{} token",
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

        for message in &channel_export.messages {
            writeln!(
                writer_txt,
                "{} - {}\n{}\n",
                message
                    .message
                    .user
                    .clone()
                    .unwrap_or("UNKNOWN".to_string()),
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
                writeln!(
                    writer_txt,
                    "    {} - {}\n    {}\n",
                    reply.user.clone().unwrap_or("UNKNOWN".to_string()),
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

fn prompt_input(prompt: &str) -> io::Result<String> {
    print!("{prompt}");
    io::stdout().flush()?;

    let mut input = String::new();
    io::stdin().read_line(&mut input)?;

    // Clear the line and move the cursor up
    print!("\x1b[1A\x1b[2K");
    io::stdout().flush()?;

    Ok(input.trim().to_string())
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

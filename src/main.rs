use clap::Parser;
use matrix_sdk::authentication::matrix::MatrixSession;
use matrix_sdk::ruma::api::client::filter::FilterDefinition;
use matrix_sdk::ruma::events::room::message::RoomMessageEventContent;
use matrix_sdk::ruma::{OwnedUserId, UserId};
use matrix_sdk::{Client, Room, RoomMemberships, config::SyncSettings};
use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use tokio::fs;

fn default_data_dir() -> PathBuf {
    dirs::data_dir()
        .unwrap_or_else(|| {
            eprintln!("Error: Could not determine data directory. Please specify --data-directory");
            std::process::exit(1);
        })
        .join(env!("CARGO_PKG_NAME"))
}

#[derive(Parser, Debug)]
struct Cli {
    /// The Data Directory to store session, with default in
    /// Linux: $XDG_DATA_HOME or $HOME/.local/share
    /// Windows {FOLDERID_RoamingAppData}
    /// MacOs: $HOME/Library/Application Support
    #[clap(long = "data-directory", default_value_os_t = default_data_dir())]
    data_dir: PathBuf,

    /// The Matrix user ID of the sender
    #[clap(long = "sender-id")]
    sender_id: String,

    /// The Matrix user ID of the recipient
    #[clap(long = "recipient-id")]
    recipient_id: String,

    /// The sender's Matrix account password
    #[clap(long = "password")]
    password: String,

    /// The message text to send
    #[clap(long = "message")]
    message: String,
}

#[derive(Debug, Serialize, Deserialize)]
struct FullSession {
    /// The Matrix user session
    user_session: MatrixSession,

    /// The latest sync token
    sync_token: String,
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let cli = Cli::parse();

    let user_id = UserId::parse(&cli.sender_id).expect("Valid user ID for sender");
    let recipient_id = UserId::parse(&cli.recipient_id).expect("Valid user ID for recipient");

    let filter = FilterDefinition::with_lazy_loading();
    let mut sync_settings = SyncSettings::default().filter(filter.into());

    let client = Client::builder()
        .server_name(user_id.server_name())
        .build()
        .await?;

    let session_file_path = foo(&cli.data_dir).await;

    if session_file_path.exists() {
        match restore_session(&client, &session_file_path).await {
            Ok(sync_token) => {
                sync_settings = sync_settings.token(sync_token);
            }
            Err(_) => {
                println!("Failed to restore session, attempting to login...");
                login(&client, &cli.sender_id, &cli.password).await?;
            }
        }
    } else {
        login(&client, &cli.sender_id, &cli.password).await?;
    }

    let next_sync_token = client.sync_once(sync_settings).await?.next_batch;
    let session = FullSession {
        user_session: client
            .matrix_auth()
            .session()
            .expect("Logged-in client should have a session"),
        sync_token: next_sync_token,
    };
    write_session(&session_file_path, &session).await?;

    let room = determine_room(&recipient_id, &client).await?;
    let content = RoomMessageEventContent::text_plain(&cli.message);
    room.send(content).await?;
    println!("Message sent successfully!");

    Ok(())
}

async fn login(client: &Client, sender_id: &str, password: &str) -> anyhow::Result<()> {
    client
        .matrix_auth()
        .login_username(sender_id, password)
        .send()
        .await?;
    Ok(())
}

async fn determine_room(recipient_id: &OwnedUserId, client: &Client) -> anyhow::Result<Room> {
    Ok(loop {
        match client.get_dm_room(&recipient_id) {
            Some(room) => {
                let members = room.members(RoomMemberships::ACTIVE).await?;
                if members.len() > 1 {
                    println!("Using existing DM room");
                    break room;
                } else {
                    room.leave().await?;
                    room.forget().await?;
                    println!("Existing room has only one member, leaving and forgetting...");
                    // Continue loop to create new room
                }
            }
            None => {
                let new_room = client.create_dm(recipient_id.as_ref()).await?;
                println!("Created new DM room");
                break new_room;
            }
        };
    })
}

/// Restores a previous session
async fn restore_session(client: &Client, path: &PathBuf) -> anyhow::Result<String> {
    let session = read_session(path).await?;
    client.restore_session(session.user_session).await?;
    println!("Session restored successfully");

    Ok(session.sync_token)
}

async fn read_session(file_path: &PathBuf) -> anyhow::Result<FullSession> {
    let serialized_session = fs::read_to_string(file_path).await?;
    let session: FullSession = serde_json::from_str(&serialized_session)?;
    Ok(session)
}

async fn write_session(file_path: &PathBuf, session: &FullSession) -> anyhow::Result<()> {
    if let Some(parent) = file_path.parent() {
        fs::create_dir_all(parent).await?;
    }

    let serialized_session = serde_json::to_string_pretty(session)?;
    fs::write(file_path, serialized_session).await?;
    Ok(())
}

async fn foo(data_dir: &PathBuf) -> PathBuf {
    let mut clone = data_dir.clone();
    if !clone.ends_with(env!("CARGO_PKG_NAME")) {
        clone.push(env!("CARGO_PKG_NAME"));
    }
    clone.join("session.json")
}

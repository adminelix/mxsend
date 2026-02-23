use clap::Parser;
use matrix_sdk::authentication::matrix::MatrixSession;
use matrix_sdk::ruma::api::client::filter::FilterDefinition;
use matrix_sdk::ruma::events::room::message::RoomMessageEventContent;
use matrix_sdk::ruma::{OwnedUserId, UserId};
use matrix_sdk::{Client, Room, RoomMemberships, config::SyncSettings};
use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use tokio::fs;
use url::Url;

#[derive(Parser, Debug)]
struct Cli {
    /// The Matrix user ID of the sender (e.g., @user:example.com)
    #[clap(long = "sender-id")]
    sender_id: String,

    /// The Matrix user ID of the recipient (e.g., @user:example.com)
    #[clap(long = "recipient-id")]
    recipient_id: String,

    /// The base URL of the Matrix homeserver (e.g., https://matrix.example.com)
    #[clap(long = "host", default_value = "https://matrix.org")]
    host: Url,

    /// The password for the sender's Matrix account
    #[clap(long = "password")]
    password: String,

    /// The text message to send to the recipient
    #[clap(long = "message")]
    message: String,
}

#[derive(Debug, Serialize, Deserialize)]
struct FullSession {
    /// The Matrix user session.
    user_session: MatrixSession,

    /// The latest sync token.
    ///
    /// It is only needed to persist it when using `Client::sync_once()` and we
    /// want to make our syncs faster by not receiving all the initial sync
    /// again.
    sync_token: String,
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let cli = Cli::parse();

    println!("Sender ID: {}", cli.sender_id);
    println!("Recipient ID: {}", cli.recipient_id);
    println!("Host: {}", cli.host);
    println!("Message: {}", cli.message);

    let user_id = UserId::parse(&cli.sender_id).expect("User ID for sender should be valid");
    let recipient_id =
        UserId::parse(&cli.recipient_id).expect("User ID for recipient should be valid");

    let filter = FilterDefinition::with_lazy_loading();
    let mut sync_settings = SyncSettings::default().filter(filter.into());

    let client = Client::builder()
        .server_name(user_id.server_name())
        .build()
        .await?;

    let session_file_path = session_file_path();

    if session_file_path.exists() {
        // Try to restore session, if it fails, login with credentials
        match restore_session(&client).await {
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

    println!("Logged in as {}", cli.sender_id);

    let next_sync_token = client.sync_once(sync_settings).await?.next_batch;
    let session = FullSession {
        user_session: client
            .matrix_auth()
            .session()
            .expect("A logged-in client should have a session"),
        sync_token: next_sync_token,
    };
    write_session(&session).await?;

    // Try to get existing DM room, if it doesn't exist create a new one
    let room = determine_room(&recipient_id, &client).await?;

    let content = RoomMessageEventContent::text_plain(&cli.message);
    room.send(content).await?;
    println!("Message sent successfully!");

    println!("Session saved for future use");

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

/// Restore a previous session.
async fn restore_session(client: &Client) -> anyhow::Result<String> {
    let session_file_path = session_file_path();
    println!(
        "Previous session found in '{}'",
        session_file_path.to_string_lossy()
    );

    // The session was serialized as JSON in a file.
    let session = read_session().await?;

    // Restore the Matrix user session.
    client.restore_session(session.user_session).await?;
    println!("Session restored successfully");

    Ok(session.sync_token)
}

async fn read_session() -> anyhow::Result<FullSession> {
    let session_file_path = session_file_path();
    let serialized_session = fs::read_to_string(session_file_path).await?;
    let session: FullSession = serde_json::from_str(&serialized_session)?;
    Ok(session)
}

async fn write_session(session: &FullSession) -> anyhow::Result<()> {
    let session_file_path = session_file_path();
    let serialized_session = serde_json::to_string_pretty(session)?;
    fs::write(session_file_path, serialized_session).await?;
    Ok(())
}

fn session_file_path() -> PathBuf {
    std::env::current_exe()
        .unwrap()
        .parent()
        .unwrap()
        .to_path_buf()
        .join("session.json")
}

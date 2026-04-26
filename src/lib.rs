use anyhow::Result;
use clap::Parser;
use matrix_sdk::ruma::api::client::filter::FilterDefinition;
use matrix_sdk::ruma::events::room::message::RoomMessageEventContent;
use matrix_sdk::ruma::{OwnedUserId, UserId};
use matrix_sdk::{Client, Room, RoomMemberships, config::SyncSettings};

#[derive(Parser, Debug)]
#[command(version, about, long_about = None)]
pub struct Cli {
    /// The Matrix user ID of the sender
    #[arg(
        long = "sender-id",
        short = 's',
        env = concat!(env!("CARGO_PKG_NAME_UPPERCASE"), "_SENDER_ID")
    )]
    pub sender_id: OwnedUserId,

    /// The sender's Matrix account password
    #[arg(
        long = "sender-password",
        short = 'p',
        env = concat!(env!("CARGO_PKG_NAME_UPPERCASE"), "_SENDER_PASSWORD")
    )]
    pub sender_password: String,

    /// The Matrix user ID of the recipient
    #[arg(
        long = "recipient-id",
        short = 'r',
        env = concat!(env!("CARGO_PKG_NAME_UPPERCASE"), "_RECIPIENT_ID")
    )]
    pub recipient_id: OwnedUserId,

    /// The recovery key for session verification
    #[arg(
        long = "recovery-key",
        short = 'k',
        env = concat!(env!("CARGO_PKG_NAME_UPPERCASE"), "_RECOVERY_KEY")
    )]
    pub recovery_key: Option<String>,

    /// The message text to send
    pub message: String,
}

/// Build a Matrix client, handling test and production environments
pub async fn build_client(user_id: &UserId) -> Result<Client> {
    #[cfg(debug_assertions)]
    {
        if let Ok(homeserver_url) = std::env::var("TEST_HOMESERVER_URL") {
            // For test environments, use HTTP instead of HTTPS discovery
            return Ok(Client::builder()
                .homeserver_url(&homeserver_url)
                .build()
                .await?);
        }
    }

    // Default behavior for non-test environments
    Ok(Client::builder()
        .server_name_or_homeserver_url(user_id.server_name())
        .build()
        .await?)
}

/// Main execution logic
pub async fn execute_main_logic(cli: Cli) -> Result<()> {
    let client = build_client(&cli.sender_id).await?;
    if client.session_meta().is_none() {
        login(&client, &cli.sender_id, &cli.sender_password).await?;
    }

    client
        .sync_once(SyncSettings::default().filter(FilterDefinition::with_lazy_loading().into()))
        .await?;

    if let Some(ref recovery_key) = cli.recovery_key {
        verify_session(&client, recovery_key).await?;
    }

    // Perform the actual sending logic
    let result = send_message(&client, &cli.message, &cli.recipient_id).await;

    // Logout after operation
    client.logout().await?;
    println!("Matrix auth logged out successfully");

    result?;
    Ok(())
}

/// Login to Matrix with the provided credentials
async fn login(client: &Client, sender_id: &OwnedUserId, password: &str) -> Result<()> {
    client
        .matrix_auth()
        .login_username(sender_id, password)
        .send()
        .await?;
    Ok(())
}

/// Verify session using recovery key if provided
async fn verify_session(client: &Client, recovery_key: &str) -> Result<()> {
    client.encryption().recovery().recover(recovery_key).await?;
    println!("Successfully verified session using recovery key");
    Ok(())
}

/// Send a message to a recipient
async fn send_message(client: &Client, message: &str, recipient_id: &OwnedUserId) -> Result<()> {
    let room = determine_room(client, recipient_id).await?;
    let content = RoomMessageEventContent::text_plain(message);
    room.send(content).await?;
    println!("Message sent successfully!");
    Ok(())
}

/// Determine which room to use for communication with the recipient.
///
/// Scans all joined rooms to find an existing DM with the recipient. If the
/// recipient left a previous DM, that stale room is cleaned up. Only creates
/// a new room if no valid DM exists.
async fn determine_room(client: &Client, recipient_id: &OwnedUserId) -> Result<Room> {
    let mut valid_room: Option<Room> = None;

    for room in client.joined_rooms() {
        if room.joined_members_count() != 2 {
            continue;
        }

        let members = room.members(RoomMemberships::ACTIVE).await?;
        let has_recipient = members.iter().any(|m| m.user_id() == recipient_id);

        if has_recipient && members.len() == 2 {
            match valid_room {
                None => {
                    valid_room = Some(room);
                }
                Some(_) => {
                    room.leave().await?;
                    room.forget().await?;
                    println!("Cleaned up duplicate DM room");
                }
            }
        } else if !has_recipient && members.len() == 1 {
            room.leave().await?;
            room.forget().await?;
            println!("Cleaned up stale DM room (recipient left)");
        }
    }

    if let Some(room) = valid_room {
        return Ok(room);
    }

    let new_room = client.create_dm(recipient_id.as_ref()).await?;
    println!("Created new DM room");
    Ok(new_room)
}

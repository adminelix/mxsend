use std::error::Error;
use matrix_sdk::ruma::{OwnedUserId, UserId};
use matrix_sdk::ruma::events::room::message::RoomMessageEventContent;
use matrix_sdk::{Client, RoomMemberships, config::SyncSettings, Room};
use matrix_sdk::ruma::api::client::filter::FilterDefinition;
use clap::Parser;
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

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let opt = Cli::parse();

    println!("Sender ID: {}", opt.sender_id);
    println!("Recipient ID: {}", opt.recipient_id);
    println!("Host: {}", opt.host);
    println!("Password: {}", opt.password);
    println!("Message: {}", opt.message);

    let user_id = UserId::parse(&opt.sender_id).expect("User ID for recipient should be valid");
    let recipient_id = UserId::parse(&opt.recipient_id).expect("User ID for recipient should be valid");

    let client = Client::builder()
        .server_name(user_id.server_name())
        .build()
        .await?;

    // Login using the new API
    client
        .matrix_auth()
        .login_username(&opt.sender_id, &opt.password)
        .send()
        .await?;

    let filter = FilterDefinition::with_lazy_loading();
    let sync_settings = SyncSettings::default().filter(filter.into());

    client.sync_once(sync_settings).await?;

    println!("Logged in as {}", opt.sender_id);

    // Try to get existing DM room, if it doesn't exist create a new one
    let room = determine_room(&recipient_id, &client).await?;

    let content = RoomMessageEventContent::text_plain(&opt.message);
    room.send(content).await?;

    println!("Message sent successfully!");

    client.logout().await?; // Logout after sending the message
    println!("Logged out");

    Ok(())
}

async fn determine_room(recipient_id: &OwnedUserId, client: &Client) -> Result<Room, Box<dyn Error>> {
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
                    println!("Existing room has only members, leaving and forgetting...");
                    // Continue loop to create new room
                }
            }
            None => {
                let room1 = client.create_dm(recipient_id.as_ref()).await?;
                println!("Created new DM room");
                break room1;
            }
        };
    })
}

use anyhow::{anyhow, Error};
use clap::Parser;
use matrix_sdk::ruma::api::client::filter::FilterDefinition;
use matrix_sdk::ruma::events::room::message::RoomMessageEventContent;
use matrix_sdk::ruma::{OwnedUserId, UserId};
use matrix_sdk::{Client, Room, RoomMemberships, config::SyncSettings};

#[derive(Parser, Debug)]
#[command(version, about, long_about = None)]
struct Cli {
    /// The Matrix user ID of the sender
    #[arg(
        long = "sender-id",
        short = 's',
        env = concat!(env!("CARGO_PKG_NAME_UPPERCASE"), "_SENDER_ID")
    )]
    sender_id: String,

    /// The sender's Matrix account password
    #[arg(
        long = "sender-password",
        short = 'p',
        env = concat!(env!("CARGO_PKG_NAME_UPPERCASE"), "_SENDER_PASSWORD")
    )]
    sender_password: String,

    /// The Matrix user ID of the recipient
    #[arg(
        long = "recipient-id",
        short = 'r',
        env = concat!(env!("CARGO_PKG_NAME_UPPERCASE"), "_RECIPIENT_ID")
    )]
    recipient_id: String,

    /// The message text to send
    message: String,
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let cli = Cli::parse();

    let user_id = UserId::parse(&cli.sender_id)
        .map_err(|err| anyhow!("invalid sender_id: {}", err.to_string()))?;
    let recipient_id = UserId::parse(&cli.recipient_id)
        .map_err(|err| anyhow!("invalid recipient_id: {}", err.to_string()))?;

    let filter = FilterDefinition::with_lazy_loading();
    let sync_settings = SyncSettings::default().filter(filter.into());

    let client = Client::builder()
        .server_name(user_id.server_name())
        .build()
        .await?;

    login(&client, &cli.sender_id, &cli.sender_password).await?;

    let result = async {
        client.sync_once(sync_settings).await?;
        let room = determine_room(&recipient_id, &client).await?;
        let content = RoomMessageEventContent::text_plain(&cli.message);
        room.send(content).await?;
        println!("Message sent successfully!");
        Ok::<(), Error>(())
    }.await;

    client.logout().await?;
    println!("Matrix auth logged out successfully");

    result?;

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
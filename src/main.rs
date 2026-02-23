use matrix_sdk::ruma::UserId;
use matrix_sdk::ruma::events::room::message::RoomMessageEventContent;
use matrix_sdk::{Client, RoomMemberships, config::SyncSettings};
use matrix_sdk::ruma::api::client::filter::FilterDefinition;
use structopt::StructOpt;
use url::Url;

#[derive(StructOpt, Debug)]
#[structopt(name = "message_sender")]
struct Opt {
    /// Sender ID
    #[structopt(long = "sender-id")]
    sender_id: String,

    /// Recipient ID
    #[structopt(long = "recipient-id")]
    recipient_id: String,

    /// Host URL
    #[structopt(long = "host")]
    host: Url,

    /// Password
    #[structopt(long = "password")]
    password: String,

    /// Message to send
    #[structopt(long = "message", default_value = "xyz")]
    message: String,
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let opt = Opt::from_args();

    println!("Sender ID: {}", opt.sender_id);
    println!("Recipient ID: {}", opt.recipient_id);
    println!("Host: {}", opt.host);
    println!("Password: {}", opt.password);
    println!("Message: {}", opt.message);

    // Create client using builder pattern
    let user_id = UserId::parse(&opt.sender_id)?;
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

    let recipient_id = UserId::parse(&opt.recipient_id).expect("Invalid user ID");

    // Try to get existing DM room, if it doesn't exist create a new one
    let room = loop {
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
    };

    let content = RoomMessageEventContent::text_plain(&opt.message);
    room.send(content).await?;

    println!("Message sent successfully!");

    client.logout().await?; // Logout after sending the message
    println!("Logged out");

    Ok(())
}

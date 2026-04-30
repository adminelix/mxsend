// SPDX-FileCopyrightText: 2026 mxsend contributors
// SPDX-License-Identifier: MIT OR Apache-2.0

use std::str::FromStr;

use anyhow::Result;
use clap::Parser;
use clap_verbosity_flag::Verbosity;
use matrix_sdk::ruma::api::client::filter::FilterDefinition;
use matrix_sdk::ruma::events::room::message::RoomMessageEventContent;
use matrix_sdk::ruma::{OwnedRoomId, OwnedUserId, RoomId, UserId};
use matrix_sdk::{Client, Room, RoomMemberships, RoomState, config::SyncSettings};
use tracing::info;

/// A message recipient — either a user ID (`@user:server`) or a room ID (`!room:server`).
#[derive(Debug, Clone)]
pub enum Recipient {
    User(OwnedUserId),
    Room(OwnedRoomId),
}

impl FromStr for Recipient {
    type Err = anyhow::Error;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        if s.starts_with('@') {
            Ok(Recipient::User(UserId::parse(s)?.to_owned()))
        } else if s.starts_with('!') {
            Ok(Recipient::Room(RoomId::parse(s)?.to_owned()))
        } else {
            Err(anyhow::anyhow!(
                "recipient must start with @ (user) or ! (room)"
            ))
        }
    }
}

/// CLI options parsed by [`clap`].
#[derive(Parser, Debug)]
#[command(version, about, long_about = None)]
pub struct SendOptions {
    /// Sender's Matrix user ID (e.g. @alice:server.com)
    #[arg(
        long = "from",
        short = 'f',
        env = concat!(env!("CARGO_PKG_NAME_UPPERCASE"), "_FROM")
    )]
    pub from: OwnedUserId,

    /// Sender's account password for login
    #[arg(
        long = "password",
        short = 'p',
        env = concat!(env!("CARGO_PKG_NAME_UPPERCASE"), "_PASSWORD")
    )]
    pub password: String,

    /// The recipient — a Matrix user ID (@user:server) or room ID (!room:server)
    #[arg(
        long = "to",
        short = 't',
        env = concat!(env!("CARGO_PKG_NAME_UPPERCASE"), "_TO")
    )]
    pub to: Recipient,

    /// Recovery key to verify the sender's E2EE device (optional)
    #[arg(
        long = "recovery-key",
        short = 'k',
        env = concat!(env!("CARGO_PKG_NAME_UPPERCASE"), "_RECOVERY_KEY")
    )]
    pub recovery_key: Option<String>,

    /// Verbosity level (use -v, -vv, -vvv, or -q to suppress output)
    #[command(flatten)]
    pub verbosity: Verbosity,

    /// Plain text message body to send
    pub message: String,
}

/// Builder for sending a Matrix message.
///
/// # Example
///
/// ```no_run
/// # use mxsend::{SendOptions, MessageSender};
/// # async fn example(opts: SendOptions) -> anyhow::Result<()> {
/// MessageSender::new(opts)
///     .with_homeserver("http://localhost:8008")
///     .send()
///     .await?;
/// # Ok(())
/// # }
/// ```
pub struct MessageSender {
    from: OwnedUserId,
    password: String,
    to: Recipient,
    message: String,
    recovery_key: Option<String>,
    homeserver_url: Option<String>,
}

impl MessageSender {
    /// Create a new sender from CLI arguments.
    pub fn new(opts: SendOptions) -> Self {
        Self {
            from: opts.from,
            password: opts.password,
            to: opts.to,
            message: opts.message,
            recovery_key: opts.recovery_key,
            homeserver_url: None,
        }
    }

    /// Override the homeserver URL instead of auto-discovering it.
    pub fn with_homeserver(mut self, url: &str) -> Self {
        self.homeserver_url = Some(url.to_string());
        self
    }

    /// Execute the full send pipeline: build client, login, sync, verify, send, logout.
    pub async fn send(self) -> Result<()> {
        let client = build_client(&self.from, self.homeserver_url.as_deref()).await?;

        if client.session_meta().is_none() {
            login(&client, &self.from, &self.password).await?;
        }

        client
            .sync_once(SyncSettings::default().filter(FilterDefinition::with_lazy_loading().into()))
            .await?;

        if let Some(ref recovery_key) = self.recovery_key {
            verify_session(&client, recovery_key).await?;
        }

        let result = send_to_recipient(&client, &self.message, &self.to).await;

        client.logout().await?;
        info!("Matrix auth logged out successfully");

        result?;
        Ok(())
    }
}

/// Build a Matrix client with an optional explicit homeserver URL.
///
/// When `homeserver_url` is `None`, the homeserver is discovered from the
/// user's server name via the Matrix well-known protocol.
pub async fn build_client(from: &UserId, homeserver_url: Option<&str>) -> Result<Client> {
    let mut builder = Client::builder();

    builder = if let Some(url) = homeserver_url {
        builder.homeserver_url(url)
    } else {
        builder.server_name_or_homeserver_url(from.server_name())
    };

    Ok(builder.build().await?)
}

/// Resolve a [`Recipient`] to a concrete [`Room`].
///
/// For user IDs, scans joined rooms for an existing DM or creates a new one.
/// For room IDs, joins the room if the sender is not already a member.
async fn resolve_room(client: &Client, recipient: &Recipient) -> Result<Room> {
    match recipient {
        Recipient::User(user_id) => resolve_dm_room(client, user_id).await,
        Recipient::Room(room_id) => {
            if let Some(room) = client.get_room(room_id) {
                if room.state() == RoomState::Joined {
                    Ok(room)
                } else {
                    room.join().await?;
                    client
                        .get_room(room_id)
                        .ok_or_else(|| anyhow::anyhow!("room {room_id} not found after join"))
                }
            } else {
                client.join_room_by_id(room_id).await.map_err(Into::into)
            }
        }
    }
}

async fn send_to_recipient(client: &Client, message: &str, recipient: &Recipient) -> Result<()> {
    let room = resolve_room(client, recipient).await?;
    let content = RoomMessageEventContent::text_plain(message);
    room.send(content).await?;
    info!("Message sent successfully!");
    Ok(())
}

async fn login(client: &Client, from: &OwnedUserId, password: &str) -> Result<()> {
    client
        .matrix_auth()
        .login_username(from, password)
        .send()
        .await?;
    Ok(())
}

async fn verify_session(client: &Client, recovery_key: &str) -> Result<()> {
    client.encryption().recovery().recover(recovery_key).await?;
    info!("Successfully verified session using recovery key");
    Ok(())
}

/// Resolve a user ID to a DM room.
///
/// Scans all joined rooms to find an existing DM with the recipient. If the
/// recipient left a previous DM, that stale room is cleaned up. Only creates
/// a new room if no valid DM exists.
async fn resolve_dm_room(client: &Client, user_id: &OwnedUserId) -> Result<Room> {
    let mut candidate_room: Option<Room> = None;

    for room in client.joined_rooms() {
        let members = room.members(RoomMemberships::ACTIVE).await?;
        let recipient_is_member = members.iter().any(|m| m.user_id() == user_id);

        if recipient_is_member && members.len() == 2 {
            match candidate_room {
                None => {
                    candidate_room = Some(room);
                }
                Some(_) => {
                    room.leave().await?;
                    room.forget().await?;
                    info!("Cleaned up duplicate DM room");
                }
            }
        } else if !recipient_is_member && members.len() == 1 {
            room.leave().await?;
            room.forget().await?;
            info!("Cleaned up stale DM room (recipient left)");
        }
    }

    if let Some(room) = candidate_room {
        return Ok(room);
    }

    let new_room = client.create_dm(user_id.as_ref()).await?;
    info!("Created new DM room");
    Ok(new_room)
}

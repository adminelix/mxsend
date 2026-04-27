mod common;

#[cfg(test)]
// Module-level `#[serial_test::serial]` is intentionally disabled because it
// breaks RustRover syntax highlighting (proc-macro on module level).
// Instead, each test function is annotated individually.
// #[serial_test::serial]
mod tests {
    use super::common::{SyncThread, TestContext, get_shared_context};
    use matrix_sdk::Room;
    use matrix_sdk::deserialized_responses::{EncryptionInfo, VerificationState};
    use matrix_sdk::encryption::EncryptionSettings;
    use matrix_sdk::ruma::events::room::member::StrippedRoomMemberEvent;
    use matrix_sdk::ruma::events::room::message::{SyncRoomMessageEvent, TextMessageEventContent};
    use matrix_sdk::ruma::{OwnedRoomId, UserId};
    use mxsend::Recipient;
    use std::sync::Arc;
    use std::time::Duration;
    use tokio::sync::RwLock;

    const DEFAULT_PASSWORD: &str = "testpass123";
    const TIMEOUT: Duration = Duration::from_secs(10);

    /// Tracks events received by the test receiver during a test.
    #[derive(Debug, Default)]
    struct ReceiverState {
        invite_received: bool,
        room_id: Option<OwnedRoomId>,
        message_received: bool,
        message_body: Option<String>,
        message_verified: bool,
        verification_level: Option<matrix_sdk::deserialized_responses::VerificationLevel>,
    }

    /// Creates a test user and returns their user ID and client.
    async fn create_test_user(ctx: &TestContext, name: &str) -> (String, matrix_sdk::Client) {
        let user_id_str = ctx.add_user(name, DEFAULT_PASSWORD, true).await;
        let user_id = UserId::parse(&user_id_str).expect("valid user id");
        let client = mxsend::build_client(&user_id, Some(&ctx.homeserver_url()))
            .await
            .expect("Failed to build client");
        (user_id_str, client)
    }

    /// Logs in the client with the given credentialguard.
    async fn login(client: &matrix_sdk::Client, user_id: &str, password: &str) {
        client
            .matrix_auth()
            .login_username(user_id, password)
            .send()
            .await
            .expect("Login failed");
    }

    /// Sets up event handlers for the receiver to capture invites and messageguard.
    ///
    /// Automatically accepts invites and stores message details in the shared state.
    fn setup_receiver_handlers(client: &matrix_sdk::Client, state: &Arc<RwLock<ReceiverState>>) {
        // Handle room invites - automatically join
        let invite_state = state.clone();
        client.add_event_handler(move |ev: StrippedRoomMemberEvent, room: Room| {
            let state = invite_state.clone();
            async move {
                if ev.content.membership.as_str() == "invite" {
                    println!("[Receiver] Received invite in room {:?}", room.room_id());
                    let _ = room.join().await;
                    let mut guard = state.write().await;
                    guard.invite_received = true;
                    guard.room_id = Some(room.room_id().to_owned());
                }
            }
        });

        // Handle room messages - extract body and verification info
        let message_state = state.clone();
        client.add_event_handler(
            move |ev: SyncRoomMessageEvent,
                  room: Room,
                  encryption_info: Option<EncryptionInfo>| {
                let state = message_state.clone();
                async move {
                    if let SyncRoomMessageEvent::Original(original) = ev
                        && let matrix_sdk::ruma::events::room::message::MessageType::Text(
                            TextMessageEventContent { body, .. },
                        ) = original.content.msgtype
                    {
                        println!("[Receiver] Received message: {body}");
                        println!("[Receiver] Encryption info: {:?}", encryption_info);
                        let mut guard = state.write().await;
                        guard.message_received = true;
                        guard.message_body = Some(body);
                        guard.room_id = Some(room.room_id().to_owned());
                        // Track if message was from a verified device
                        guard.message_verified = encryption_info
                            .as_ref()
                            .map(|info| {
                                matches!(info.verification_state, VerificationState::Verified)
                            })
                            .unwrap_or(false);
                        // Extract verification level for assertions
                        guard.verification_level = encryption_info.map(|info| match info.verification_state {
                            VerificationState::Verified => matrix_sdk::deserialized_responses::VerificationLevel::UnverifiedIdentity,
                            VerificationState::Unverified(level) => level,
                        });
                    }
                }
            },
        );
    }

    /// Waits for a message to be received, optionally checking for verification.
    async fn wait_for_message(state: &Arc<RwLock<ReceiverState>>, require_verified: bool) {
        let deadline = tokio::time::Instant::now() + TIMEOUT;
        while tokio::time::Instant::now() < deadline {
            let guard = state.read().await;
            if guard.message_received && (!require_verified || guard.message_verified) {
                return;
            }
            drop(guard);
            tokio::time::sleep(Duration::from_millis(200)).await;
        }
    }

    /// Creates a sender with cross-signing and recovery enabled.
    ///
    /// This is required for end-to-end encryption verification testguard.
    /// Returns the user ID and recovery key.
    async fn bootstrap_sender_with_recovery(ctx: &TestContext) -> (String, String) {
        let sender_id = ctx
            .add_user("verified_sender", DEFAULT_PASSWORD, true)
            .await;

        // Build client with encryption enabled
        let homeserver_url = ctx.homeserver_url();
        let sender_client = matrix_sdk::Client::builder()
            .homeserver_url(&homeserver_url)
            .with_encryption_settings(EncryptionSettings {
                auto_enable_cross_signing: true,
                auto_enable_backups: true,
                ..Default::default()
            })
            .build()
            .await
            .expect("Failed to build client with encryption");

        login(&sender_client, &sender_id, DEFAULT_PASSWORD).await;

        // Bootstrap cross-signing
        sender_client
            .encryption()
            .bootstrap_cross_signing(None)
            .await
            .expect("Failed to bootstrap cross-signing");

        // Enable recovery and get the recovery key
        let recovery_key = sender_client
            .encryption()
            .recovery()
            .enable()
            .await
            .expect("Failed to enable recovery");

        (sender_id, recovery_key)
    }

    #[serial_test::serial]
    #[tokio::test]
    async fn test_send_message_to_synapse() {
        let _ = env_logger::try_init();
        let ctx = get_shared_context().await;

        let (sender_id_str, _) = create_test_user(&ctx, "sender").await;
        let (recipient_id_str, _) = create_test_user(&ctx, "recipient").await;

        let recipient_id = UserId::parse(&recipient_id_str).expect("valid user id");
        let sender_id = UserId::parse(&sender_id_str).expect("valid user id");

        let opts = mxsend::SendOptions {
            from: sender_id,
            password: DEFAULT_PASSWORD.to_string(),
            to: Recipient::User(recipient_id),
            recovery_key: None,
            verbosity: Default::default(),
            message: "Integration test message".to_string(),
        };

        mxsend::MessageSender::new(opts)
            .with_homeserver(&ctx.homeserver_url())
            .send()
            .await
            .expect("Failed to execute main logic");
    }

    #[serial_test::serial]
    #[tokio::test]
    async fn test_receiver_listens_and_receives_message() {
        let _ = env_logger::try_init();
        let ctx = get_shared_context().await;

        // Create sender and receiver
        let (sender_id_str, _) = create_test_user(&ctx, "sender").await;
        let (receiver_id_str, receiver_client) = create_test_user(&ctx, "receiver").await;
        login(&receiver_client, &receiver_id_str, DEFAULT_PASSWORD).await;

        // Setup receiver to listen for events
        let state = Arc::new(RwLock::new(ReceiverState::default()));
        setup_receiver_handlers(&receiver_client, &state);
        let mut sync_thread = SyncThread::start(receiver_client.clone());

        // Send message from sender
        let receiver_id = UserId::parse(&receiver_id_str).expect("valid user id");
        let opts = mxsend::SendOptions {
            from: UserId::parse(&sender_id_str).expect("valid sender id"),
            password: DEFAULT_PASSWORD.to_string(),
            to: Recipient::User(receiver_id),
            recovery_key: None,
            verbosity: Default::default(),
            message: "Test message from sender to receiver".to_string(),
        };

        mxsend::MessageSender::new(opts)
            .with_homeserver(&ctx.homeserver_url())
            .send()
            .await
            .expect("Failed to execute main logic");

        // Wait for receiver to get the message
        wait_for_message(&state, false).await;
        sync_thread.stop();

        // Verify receiver state
        let state = state.read().await;
        assert!(
            state.invite_received,
            "Receiver should have received an invite"
        );
        assert!(
            state.room_id.is_some(),
            "Receiver should have joined a room"
        );
        assert!(
            state.message_received,
            "Receiver should have received a message"
        );
        assert_eq!(
            state.message_body.as_deref(),
            Some("Test message from sender to receiver"),
            "Message content should match"
        );
        // Without recovery_key, sender's device is unsigned
        assert_eq!(
            state.verification_level,
            Some(matrix_sdk::deserialized_responses::VerificationLevel::UnsignedDevice),
            "Sender's device should be unsigned without recovery key"
        );

        receiver_client.logout().await.ok();
    }

    #[serial_test::serial]
    #[tokio::test]
    async fn test_receiver_listens_and_receives_verified_message() {
        let _ = env_logger::try_init();
        let ctx = get_shared_context().await;

        // Step 1: Create sender with cross-signing and recovery enabled
        let (sender_id_str, recovery_key) = bootstrap_sender_with_recovery(&ctx).await;

        // Step 2: Create receiver
        let (receiver_id_str, receiver_client) = create_test_user(&ctx, "receiver").await;
        login(&receiver_client, &receiver_id_str, DEFAULT_PASSWORD).await;

        // Step 3: Setup receiver handlers and wait for invite
        let state = Arc::new(RwLock::new(ReceiverState::default()));
        setup_receiver_handlers(&receiver_client, &state);

        // Start sync briefly to accept the invite, then stop
        let mut sync_thread = SyncThread::start(receiver_client.clone());
        let deadline = tokio::time::Instant::now() + TIMEOUT;
        while tokio::time::Instant::now() < deadline {
            let guard = state.read().await;
            if guard.invite_received {
                drop(guard);
                break;
            }
            drop(guard);
            tokio::time::sleep(Duration::from_millis(200)).await;
        }
        sync_thread.stop();

        // Step 4: Send message with recovery key (enables verification)
        let sender_id = UserId::parse(&sender_id_str).expect("valid sender id");
        let receiver_id = UserId::parse(&receiver_id_str).expect("valid user id");
        let opts = mxsend::SendOptions {
            from: sender_id,
            password: DEFAULT_PASSWORD.to_string(),
            to: Recipient::User(receiver_id),
            recovery_key: Some(recovery_key),
            verbosity: Default::default(),
            message: "Verified test message".to_string(),
        };
        mxsend::MessageSender::new(opts)
            .with_homeserver(&ctx.homeserver_url())
            .send()
            .await
            .expect("Failed to execute main logic");

        // Step 5: Force receiver to fetch sender's updated device keys before syncing
        let sender_id = UserId::parse(&sender_id_str).expect("valid sender id");
        receiver_client
            .encryption()
            .request_user_identity(&sender_id)
            .await
            .expect("Failed to request sender identity");

        // Step 6: Start sync and wait for verified message
        let mut sync_thread = SyncThread::start(receiver_client.clone());
        wait_for_message(&state, true).await;
        sync_thread.stop();

        // Verify receiver state
        let final_state = state.read().await;
        assert!(
            final_state.invite_received,
            "Receiver should have received an invite"
        );
        assert!(
            final_state.room_id.is_some(),
            "Receiver should have joined a room"
        );
        assert!(
            final_state.message_received,
            "Receiver should have received a message"
        );
        assert_eq!(
            final_state.message_body.as_deref(),
            Some("Verified test message"),
            "Message content should match"
        );
        // With recovery, sender's device is signed by cross-signing keys
        assert_eq!(
            final_state.verification_level,
            Some(matrix_sdk::deserialized_responses::VerificationLevel::UnverifiedIdentity),
            "Sender's device should be signed by cross-signing keys after recovery"
        );

        receiver_client.logout().await.ok();
    }

    #[serial_test::serial]
    #[tokio::test]
    async fn test_dm_room_reused_on_second_send() {
        let _ = env_logger::try_init();
        let ctx = get_shared_context().await;

        let (sender_id_str, _) = create_test_user(&ctx, "sender").await;
        let (receiver_id_str, receiver_client) = create_test_user(&ctx, "receiver").await;
        login(&receiver_client, &receiver_id_str, DEFAULT_PASSWORD).await;

        let state = Arc::new(RwLock::new(ReceiverState::default()));
        setup_receiver_handlers(&receiver_client, &state);
        let mut sync_thread = SyncThread::start(receiver_client.clone());

        let receiver_id = UserId::parse(&receiver_id_str).expect("valid user id");
        let sender_id = UserId::parse(&sender_id_str).expect("valid sender id");

        // First send — creates the DM room
        let opts = mxsend::SendOptions {
            from: sender_id.clone(),
            password: DEFAULT_PASSWORD.to_string(),
            to: Recipient::User(receiver_id.clone()),
            recovery_key: None,
            verbosity: Default::default(),
            message: "First message".to_string(),
        };
        mxsend::MessageSender::new(opts)
            .with_homeserver(&ctx.homeserver_url())
            .send()
            .await
            .expect("First send failed");

        wait_for_message(&state, false).await;
        let first_room_id = state.read().await.room_id.clone().unwrap();

        // Reset receiver state for second message
        {
            let mut guard = state.write().await;
            guard.message_received = false;
            guard.message_body = None;
        }

        // Second send — must reuse the existing DM room
        let opts = mxsend::SendOptions {
            from: sender_id.clone(),
            password: DEFAULT_PASSWORD.to_string(),
            to: Recipient::User(receiver_id),
            recovery_key: None,
            verbosity: Default::default(),
            message: "Second message".to_string(),
        };
        mxsend::MessageSender::new(opts)
            .with_homeserver(&ctx.homeserver_url())
            .send()
            .await
            .expect("Second send failed");

        wait_for_message(&state, false).await;
        sync_thread.stop();

        // Receiver should have received both messages in the same room
        let guard = state.read().await;
        assert!(
            guard.message_received,
            "Receiver should have received the second message"
        );
        assert_eq!(
            guard.message_body.as_deref(),
            Some("Second message"),
            "Second message content should match"
        );
        let second_room_id = guard.room_id.clone().unwrap();
        assert_eq!(
            first_room_id, second_room_id,
            "Second send must reuse the same room"
        );
        drop(guard);

        // Verify receiver has exactly one joined room (room was reused, not duplicated)
        let receiver_joined = receiver_client.joined_rooms();
        assert_eq!(
            receiver_joined.len(),
            1,
            "Receiver should have exactly 1 joined room, found {}",
            receiver_joined.len()
        );

        receiver_client.logout().await.ok();
    }

    #[serial_test::serial]
    #[tokio::test]
    async fn test_dm_room_recreated_after_recipient_leaves() {
        let _ = env_logger::try_init();
        let ctx = get_shared_context().await;

        let (sender_id_str, _) = create_test_user(&ctx, "sender").await;
        let (receiver_id_str, receiver_client) = create_test_user(&ctx, "receiver").await;
        login(&receiver_client, &receiver_id_str, DEFAULT_PASSWORD).await;

        let state = Arc::new(RwLock::new(ReceiverState::default()));
        setup_receiver_handlers(&receiver_client, &state);
        let mut sync_thread = SyncThread::start(receiver_client.clone());

        let receiver_id = UserId::parse(&receiver_id_str).expect("valid user id");
        let sender_id = UserId::parse(&sender_id_str).expect("valid sender id");

        // First send — creates the DM room
        let opts = mxsend::SendOptions {
            from: sender_id.clone(),
            password: DEFAULT_PASSWORD.to_string(),
            to: Recipient::User(receiver_id.clone()),
            recovery_key: None,
            verbosity: Default::default(),
            message: "Message in first room".to_string(),
        };
        mxsend::MessageSender::new(opts)
            .with_homeserver(&ctx.homeserver_url())
            .send()
            .await
            .expect("First send failed");

        wait_for_message(&state, false).await;
        let first_room_id = state.read().await.room_id.clone().unwrap();

        // Receiver leaves the room
        let room = receiver_client
            .get_room(&first_room_id)
            .expect("Receiver should know the room");
        room.leave().await.expect("Receiver failed to leave room");
        room.forget().await.expect("Receiver failed to forget room");

        // Reset receiver state for the second send
        {
            let mut guard = state.write().await;
            guard.invite_received = false;
            guard.message_received = false;
            guard.message_body = None;
            guard.room_id = None;
        }

        // Second send — must create a new DM room after cleaning up the stale one
        let opts = mxsend::SendOptions {
            from: sender_id.clone(),
            password: DEFAULT_PASSWORD.to_string(),
            to: Recipient::User(receiver_id),
            recovery_key: None,
            verbosity: Default::default(),
            message: "Message in second room".to_string(),
        };
        mxsend::MessageSender::new(opts)
            .with_homeserver(&ctx.homeserver_url())
            .send()
            .await
            .expect("Second send failed");

        wait_for_message(&state, false).await;
        sync_thread.stop();

        // Receiver should have received the message in a new room
        let guard = state.read().await;
        assert!(
            guard.invite_received,
            "Receiver should have been invited to a new room"
        );
        assert!(
            guard.message_received,
            "Receiver should have received the second message"
        );
        assert_eq!(
            guard.message_body.as_deref(),
            Some("Message in second room"),
            "Second message content should match"
        );
        let second_room_id = guard.room_id.clone().unwrap();
        assert_ne!(
            first_room_id, second_room_id,
            "A new room must be created after the recipient left"
        );
        drop(guard);

        // Verify receiver has exactly one joined room (the new room, old one was left)
        let receiver_joined = receiver_client.joined_rooms();
        assert_eq!(
            receiver_joined.len(),
            1,
            "Receiver should have exactly 1 joined room, found {}",
            receiver_joined.len()
        );

        // The new room should be different from the first room
        let new_room_id = receiver_joined[0].room_id().to_owned();
        assert_ne!(
            first_room_id, new_room_id,
            "Receiver should be in a new room, not the old one"
        );

        receiver_client.logout().await.ok();
    }

    #[serial_test::serial]
    #[tokio::test]
    async fn test_send_message_to_public_room_id() {
        let _ = env_logger::try_init();
        let ctx = get_shared_context().await;

        // Create room creator, login, and create a public room
        let (creator_id_str, creator_client) = create_test_user(&ctx, "room_creator").await;
        login(&creator_client, &creator_id_str, DEFAULT_PASSWORD).await;

        let mut create_request =
            matrix_sdk::ruma::api::client::room::create_room::v3::Request::default();
        create_request.preset =
            Some(matrix_sdk::ruma::api::client::room::create_room::v3::RoomPreset::PublicChat);
        create_request.visibility = matrix_sdk::ruma::api::client::room::Visibility::Public;

        let room = creator_client
            .create_room(create_request)
            .await
            .expect("Failed to create public room");
        let room_id = room.room_id().to_owned();

        // Setup room creator to listen for messages
        let state = Arc::new(RwLock::new(ReceiverState::default()));
        setup_receiver_handlers(&creator_client, &state);
        let mut sync_thread = SyncThread::start(creator_client.clone());

        // Create sender and send message to the room ID
        let (sender_id_str, _) = create_test_user(&ctx, "sender").await;
        let sender_id = UserId::parse(&sender_id_str).expect("valid sender id");
        let opts = mxsend::SendOptions {
            from: sender_id,
            password: DEFAULT_PASSWORD.to_string(),
            to: Recipient::Room(room_id.clone()),
            recovery_key: None,
            verbosity: Default::default(),
            message: "Message to public room by ID".to_string(),
        };

        mxsend::MessageSender::new(opts)
            .with_homeserver(&ctx.homeserver_url())
            .send()
            .await
            .expect("Failed to send message to room ID");

        // Wait for room creator to receive the message
        wait_for_message(&state, false).await;
        sync_thread.stop();

        // Verify
        let guard = state.read().await;
        assert!(
            guard.message_received,
            "Room creator should have received the message"
        );
        assert_eq!(
            guard.message_body.as_deref(),
            Some("Message to public room by ID"),
            "Message content should match"
        );
        assert_eq!(
            guard.room_id.as_ref(),
            Some(&room_id),
            "Message should be in the created public room"
        );

        creator_client.logout().await.ok();
    }
}

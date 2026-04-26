mod common;

#[cfg(test)]
#[serial_test::serial]
mod tests {
    use super::common::{SyncThread, TestContext, get_shared_context};
    use matrix_sdk::Room;
    use matrix_sdk::deserialized_responses::{EncryptionInfo, VerificationState};
    use matrix_sdk::encryption::EncryptionSettings;
    use matrix_sdk::ruma::events::room::member::StrippedRoomMemberEvent;
    use matrix_sdk::ruma::events::room::message::{SyncRoomMessageEvent, TextMessageEventContent};
    use matrix_sdk::ruma::{OwnedRoomId, UserId};
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
        let user_id = ctx.add_user(name, DEFAULT_PASSWORD, true).await;
        let parsed = UserId::parse(&user_id).expect("valid user id");
        let client = matrix_send::build_client(&parsed)
            .await
            .expect("Failed to build client");
        (user_id, client)
    }

    /// Logs in the client with the given credentials.
    async fn login(client: &matrix_sdk::Client, user_id: &str, password: &str) {
        client
            .matrix_auth()
            .login_username(user_id, password)
            .send()
            .await
            .expect("Login failed");
    }

    /// Sets up event handlers for the receiver to capture invites and messages.
    ///
    /// Automatically accepts invites and stores message details in the shared state.
    fn setup_receiver_handlers(client: &matrix_sdk::Client, state: &Arc<RwLock<ReceiverState>>) {
        // Handle room invites - automatically join
        let state_invite = state.clone();
        client.add_event_handler(move |ev: StrippedRoomMemberEvent, room: Room| {
            let state = state_invite.clone();
            async move {
                if ev.content.membership.as_str() == "invite" {
                    println!("[Receiver] Received invite in room {:?}", room.room_id());
                    let _ = room.join().await;
                    let mut s = state.write().await;
                    s.invite_received = true;
                    s.room_id = Some(room.room_id().to_owned());
                }
            }
        });

        // Handle room messages - extract body and verification info
        let state_msg = state.clone();
        client.add_event_handler(
            move |ev: SyncRoomMessageEvent,
                  _room: Room,
                  encryption_info: Option<EncryptionInfo>| {
                let state = state_msg.clone();
                async move {
                    if let SyncRoomMessageEvent::Original(original) = ev {
                        if let matrix_sdk::ruma::events::room::message::MessageType::Text(
                            TextMessageEventContent { body, .. },
                        ) = original.content.msgtype
                        {
                            println!("[Receiver] Received message: {body}");
                            println!("[Receiver] Encryption info: {:?}", encryption_info);
                            let mut s = state.write().await;
                            s.message_received = true;
                            s.message_body = Some(body);
                            // Track if message was from a verified device
                            s.message_verified = encryption_info
                                .as_ref()
                                .map(|info| {
                                    matches!(info.verification_state, VerificationState::Verified)
                                })
                                .unwrap_or(false);
                            // Extract verification level for assertions
                            s.verification_level = encryption_info.map(|info| match info.verification_state {
                                VerificationState::Verified => matrix_sdk::deserialized_responses::VerificationLevel::UnverifiedIdentity,
                                VerificationState::Unverified(level) => level,
                            });
                        }
                    }
                }
            },
        );
    }

    /// Waits for a message to be received, optionally checking for verification.
    async fn wait_for_message(state: &Arc<RwLock<ReceiverState>>, require_verified: bool) {
        let deadline = tokio::time::Instant::now() + TIMEOUT;
        while tokio::time::Instant::now() < deadline {
            let s = state.read().await;
            if s.message_received && (!require_verified || s.message_verified) {
                return;
            }
            drop(s);
            tokio::time::sleep(Duration::from_millis(200)).await;
        }
    }

    /// Creates a sender with cross-signing and recovery enabled.
    ///
    /// This is required for end-to-end encryption verification tests.
    /// Returns the user ID and recovery key.
    async fn bootstrap_sender_with_recovery(ctx: &TestContext) -> (String, String) {
        let sender_id = ctx
            .add_user("verified_sender", DEFAULT_PASSWORD, true)
            .await;

        // Build client with encryption enabled
        let homeserver_url =
            std::env::var("TEST_HOMESERVER_URL").expect("TEST_HOMESERVER_URL must be set");
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

    #[tokio::test]
    async fn test_send_message_to_synapse() {
        let _ = env_logger::try_init();
        let ctx = get_shared_context().await;

        let (sender_id, _) = create_test_user(&ctx, "sender").await;
        let (recipient_id, _) = create_test_user(&ctx, "recipient").await;

        let recipient_parsed = UserId::parse(&recipient_id).expect("valid user id");
        let sender_parsed = UserId::parse(&sender_id).expect("valid user id");

        let cli = matrix_send::Cli {
            sender_id: sender_parsed,
            sender_password: DEFAULT_PASSWORD.to_string(),
            recipient_id: recipient_parsed,
            recovery_key: None,
            message: "Integration test message".to_string(),
        };

        matrix_send::execute_main_logic(cli)
            .await
            .expect("Failed to execute main logic");
    }

    #[tokio::test]
    async fn test_receiver_listens_and_receives_message() {
        let _ = env_logger::try_init();
        let ctx = get_shared_context().await;

        // Create sender and receiver
        let (sender_id, _) = create_test_user(&ctx, "sender").await;
        let (receiver_id, receiver_client) = create_test_user(&ctx, "receiver").await;
        login(&receiver_client, &receiver_id, DEFAULT_PASSWORD).await;

        // Setup receiver to listen for events
        let state = Arc::new(RwLock::new(ReceiverState::default()));
        setup_receiver_handlers(&receiver_client, &state);
        let mut sync_thread = SyncThread::start(receiver_client.clone());

        // Send message from sender
        let receiver_parsed = UserId::parse(&receiver_id).expect("valid user id");
        let cli = matrix_send::Cli {
            sender_id: UserId::parse(&sender_id).expect("valid user id"),
            sender_password: DEFAULT_PASSWORD.to_string(),
            recipient_id: receiver_parsed,
            recovery_key: None,
            message: "Test message from sender to receiver".to_string(),
        };

        matrix_send::execute_main_logic(cli)
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

    #[tokio::test]
    async fn test_receiver_listens_and_receives_verified_message() {
        let _ = env_logger::try_init();
        let ctx = get_shared_context().await;

        // Step 1: Create sender with cross-signing and recovery enabled
        let (sender_id, recovery_key) = bootstrap_sender_with_recovery(&ctx).await;

        // Step 2: Create receiver
        let (receiver_id, receiver_client) = create_test_user(&ctx, "receiver").await;
        login(&receiver_client, &receiver_id, DEFAULT_PASSWORD).await;

        // Step 3: Setup receiver handlers and wait for invite
        let state = Arc::new(RwLock::new(ReceiverState::default()));
        setup_receiver_handlers(&receiver_client, &state);

        // Start sync briefly to accept the invite, then stop
        let mut sync_thread = SyncThread::start(receiver_client.clone());
        let deadline = tokio::time::Instant::now() + TIMEOUT;
        while tokio::time::Instant::now() < deadline {
            let s = state.read().await;
            if s.invite_received {
                drop(s);
                break;
            }
            drop(s);
            tokio::time::sleep(Duration::from_millis(200)).await;
        }
        sync_thread.stop();

        // Step 4: Send message with recovery key (enables verification)
        let sender_parsed = UserId::parse(&sender_id).expect("valid sender id");
        let receiver_parsed = UserId::parse(&receiver_id).expect("valid user id");
        let cli = matrix_send::Cli {
            sender_id: sender_parsed,
            sender_password: DEFAULT_PASSWORD.to_string(),
            recipient_id: receiver_parsed,
            recovery_key: Some(recovery_key),
            message: "Verified test message".to_string(),
        };
        matrix_send::execute_main_logic(cli)
            .await
            .expect("Failed to execute main logic");

        // Step 5: Force receiver to fetch sender's updated device keys before syncing
        let sender_parsed = UserId::parse(&sender_id).expect("valid sender id");
        receiver_client
            .encryption()
            .request_user_identity(&sender_parsed)
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

    #[tokio::test]
    async fn test_dm_room_reused_on_second_send() {
        let _ = env_logger::try_init();
        let ctx = get_shared_context().await;

        let (sender_id, _) = create_test_user(&ctx, "sender").await;
        let (receiver_id, receiver_client) = create_test_user(&ctx, "receiver").await;
        login(&receiver_client, &receiver_id, DEFAULT_PASSWORD).await;

        let state = Arc::new(RwLock::new(ReceiverState::default()));
        setup_receiver_handlers(&receiver_client, &state);
        let mut sync_thread = SyncThread::start(receiver_client.clone());

        let receiver_parsed = UserId::parse(&receiver_id).expect("valid user id");
        let sender_parsed = UserId::parse(&sender_id).expect("valid sender id");

        // First send — creates the DM room
        let cli = matrix_send::Cli {
            sender_id: sender_parsed.clone(),
            sender_password: DEFAULT_PASSWORD.to_string(),
            recipient_id: receiver_parsed.clone(),
            recovery_key: None,
            message: "First message".to_string(),
        };
        matrix_send::execute_main_logic(cli)
            .await
            .expect("First send failed");

        wait_for_message(&state, false).await;
        let first_room_id = state.read().await.room_id.clone().unwrap();

        // Reset receiver state for second message
        {
            let mut s = state.write().await;
            s.message_received = false;
            s.message_body = None;
        }

        // Second send — must reuse the existing DM room
        let cli = matrix_send::Cli {
            sender_id: sender_parsed.clone(),
            sender_password: DEFAULT_PASSWORD.to_string(),
            recipient_id: receiver_parsed,
            recovery_key: None,
            message: "Second message".to_string(),
        };
        matrix_send::execute_main_logic(cli)
            .await
            .expect("Second send failed");

        wait_for_message(&state, false).await;
        sync_thread.stop();

        // Receiver should have received both messages in the same room
        let s = state.read().await;
        assert!(
            s.message_received,
            "Receiver should have received the second message"
        );
        assert_eq!(
            s.message_body.as_deref(),
            Some("Second message"),
            "Second message content should match"
        );
        let second_room_id = s.room_id.clone().unwrap();
        assert_eq!(
            first_room_id, second_room_id,
            "Second send must reuse the same room"
        );
        drop(s);

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

    #[tokio::test]
    async fn test_dm_room_recreated_after_recipient_leaves() {
        let _ = env_logger::try_init();
        let ctx = get_shared_context().await;

        let (sender_id, _) = create_test_user(&ctx, "sender").await;
        let (receiver_id, receiver_client) = create_test_user(&ctx, "receiver").await;
        login(&receiver_client, &receiver_id, DEFAULT_PASSWORD).await;

        let state = Arc::new(RwLock::new(ReceiverState::default()));
        setup_receiver_handlers(&receiver_client, &state);
        let mut sync_thread = SyncThread::start(receiver_client.clone());

        let receiver_parsed = UserId::parse(&receiver_id).expect("valid user id");
        let sender_parsed = UserId::parse(&sender_id).expect("valid sender id");

        // First send — creates the DM room
        let cli = matrix_send::Cli {
            sender_id: sender_parsed.clone(),
            sender_password: DEFAULT_PASSWORD.to_string(),
            recipient_id: receiver_parsed.clone(),
            recovery_key: None,
            message: "Message in first room".to_string(),
        };
        matrix_send::execute_main_logic(cli)
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
            let mut s = state.write().await;
            s.invite_received = false;
            s.message_received = false;
            s.message_body = None;
            s.room_id = None;
        }

        // Second send — must create a new DM room after cleaning up the stale one
        let cli = matrix_send::Cli {
            sender_id: sender_parsed.clone(),
            sender_password: DEFAULT_PASSWORD.to_string(),
            recipient_id: receiver_parsed,
            recovery_key: None,
            message: "Message in second room".to_string(),
        };
        matrix_send::execute_main_logic(cli)
            .await
            .expect("Second send failed");

        wait_for_message(&state, false).await;
        sync_thread.stop();

        // Receiver should have received the message in a new room
        let s = state.read().await;
        assert!(
            s.invite_received,
            "Receiver should have been invited to a new room"
        );
        assert!(
            s.message_received,
            "Receiver should have received the second message"
        );
        assert_eq!(
            s.message_body.as_deref(),
            Some("Message in second room"),
            "Second message content should match"
        );
        let second_room_id = s.room_id.clone().unwrap();
        assert_ne!(
            first_room_id, second_room_id,
            "A new room must be created after the recipient left"
        );
        drop(s);

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
}

use super::synapse::SynapseImage;
use reqwest::Client as HttpClient;
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use std::sync::Weak;
use std::sync::atomic::{AtomicU32, Ordering};
use testcontainers::ContainerAsync;
use testcontainers::ImageExt;
use testcontainers::core::logs::consumer::logging_consumer::LoggingConsumer;
use testcontainers::runners::AsyncRunner;
use tokio::sync::Mutex;

#[derive(Debug, Deserialize)]
struct LoginResponse {
    access_token: String,
}

#[derive(Debug, Serialize)]
struct CreateUserRequest {
    password: String,
    admin: bool,
}

#[allow(dead_code)]
pub struct TestContext {
    container: Arc<ContainerAsync<SynapseImage>>,
    port: u16,
    admin_token: String,
}

#[allow(dead_code)]
impl TestContext {
    pub async fn add_user(&self, base: &str, password: &str, admin: bool) -> String {
        let username = unique_name(base);
        create_user(self.port, &self.admin_token, &username, password, admin)
            .await
            .expect("Failed to create user")
    }

    pub fn homeserver_url(&self) -> String {
        format!("http://localhost:{}", self.port)
    }

    pub fn port(&self) -> u16 {
        self.port
    }
}

static SHARED_CONTEXT: std::sync::OnceLock<Mutex<Weak<TestContext>>> = std::sync::OnceLock::new();
static USER_COUNTER: AtomicU32 = AtomicU32::new(1);

pub async fn get_shared_context() -> Arc<TestContext> {
    let mut guard = SHARED_CONTEXT
        .get_or_init(|| Mutex::new(Weak::new()))
        .lock()
        .await;

    if let Some(ctx) = guard.upgrade() {
        return ctx;
    }

    let ctx = create_context().await;
    *guard = Arc::downgrade(&ctx);
    ctx
}

async fn create_context() -> Arc<TestContext> {
    let container = Arc::new(
        SynapseImage::default()
            .with_log_consumer(LoggingConsumer::new().with_prefix("SYNAPSE"))
            .start()
            .await
            .expect("Failed to start Synapse container"),
    );

    let port = container
        .get_host_port_ipv4(8008)
        .await
        .expect("Failed to get port");

    let homeserver_url = format!("http://localhost:{port}");
    // SAFETY: called once during test setup, no concurrent threads access env vars
    #[allow(unsafe_code)]
    unsafe {
        std::env::set_var("TEST_HOMESERVER_URL", &homeserver_url);
    }

    let admin_token = get_admin_access_token(port)
        .await
        .expect("Failed to get admin access token");

    Arc::new(TestContext {
        container,
        port,
        admin_token,
    })
}

async fn get_admin_access_token(port: u16) -> Result<String, anyhow::Error> {
    let client = HttpClient::new();
    let response = client
        .post(format!("http://localhost:{port}/_matrix/client/r0/login"))
        .json(&serde_json::json!({
            "type": "m.login.password",
            "user": "admin",
            "password": "admin"
        }))
        .send()
        .await?;

    let login_response: LoginResponse = response.json().await?;
    Ok(login_response.access_token)
}

async fn create_user(
    port: u16,
    admin_token: &str,
    username: &str,
    password: &str,
    admin: bool,
) -> Result<String, anyhow::Error> {
    let client = HttpClient::new();
    let user_id = format!("@{username}:localhost");
    let url = format!(
        "http://localhost:{port}/_synapse/admin/v2/users/{}",
        urlencoding::encode(&user_id)
    );

    let response = client
        .put(&url)
        .header("Authorization", format!("Bearer {admin_token}"))
        .header("Content-Type", "application/json")
        .json(&CreateUserRequest {
            password: password.to_string(),
            admin,
        })
        .send()
        .await?;

    if !response.status().is_success() {
        let error_text = response.text().await?;
        return Err(anyhow::anyhow!(
            "Failed to create user {username}: {error_text}"
        ));
    }

    Ok(user_id)
}

fn unique_name(base: &str) -> String {
    let id = USER_COUNTER.fetch_add(1, Ordering::SeqCst);
    format!("{base}_t{id}")
}

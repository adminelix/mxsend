use std::borrow::Cow;
use std::collections::BTreeMap;
use testcontainers::Image;
use testcontainers::core::{ContainerPort, ContainerState, ExecCommand, WaitFor};

const DEFAULT_IMAGE_NAME: &str = "matrixdotorg/synapse";
const DEFAULT_IMAGE_TAG: &str = "latest";
pub const SYNAPSE_PORT: ContainerPort = ContainerPort::Tcp(8008);
pub const DEFAULT_SERVER_NAME: &str = "localhost";

#[derive(Debug, Clone)]
pub struct SynapseImage {
    env_vars: BTreeMap<String, String>,
    admin_user: String,
    admin_pass: String,
}

impl SynapseImage {
    pub fn admin_user(&self) -> &str {
        &self.admin_user
    }

    pub fn admin_pass(&self) -> &str {
        &self.admin_pass
    }

    pub fn server_name(&self) -> &str {
        self.env_vars
            .get("SYNAPSE_SERVER_NAME")
            .map(|s| s.as_str())
            .unwrap_or(DEFAULT_SERVER_NAME)
    }

    #[allow(dead_code)]
    pub fn with_server_name(mut self, server_name: &str) -> Self {
        self.env_vars
            .insert("SYNAPSE_SERVER_NAME".to_string(), server_name.to_string());
        self
    }
}

impl Default for SynapseImage {
    fn default() -> Self {
        let server_name =
            std::env::var("SYNAPSE_SERVER_NAME").unwrap_or(DEFAULT_SERVER_NAME.to_string());
        Self {
            admin_user: "admin".to_string(),
            admin_pass: "admin".to_string(),
            env_vars: BTreeMap::from([
                ("SYNAPSE_CONFIG_DIR".to_string(), "/data".to_string()),
                ("SYNAPSE_SERVER_NAME".to_string(), server_name),
                ("SYNAPSE_REPORT_STATS".to_string(), "no".to_string()),
            ]),
        }
    }
}

impl Image for SynapseImage {
    fn name(&self) -> &str {
        DEFAULT_IMAGE_NAME
    }

    fn tag(&self) -> &str {
        DEFAULT_IMAGE_TAG
    }

    fn ready_conditions(&self) -> Vec<WaitFor> {
        vec![
            WaitFor::message_on_either_std("Synapse now listening on TCP port"),
            WaitFor::message_on_either_std("POST /_synapse/admin/v1/register"),
        ]
    }

    fn env_vars(
        &self,
    ) -> impl IntoIterator<Item = (impl Into<Cow<'_, str>>, impl Into<Cow<'_, str>>)> {
        &self.env_vars
    }

    fn entrypoint(&self) -> Option<&str> {
        Some("/bin/bash")
    }

    fn cmd(&self) -> impl IntoIterator<Item = impl Into<Cow<'_, str>>> {
        [
            "-c",
            "mkdir -p ${SYNAPSE_CONFIG_DIR} && /start.py generate && /start.py",
        ]
    }

    fn expose_ports(&self) -> &[ContainerPort] {
        &[SYNAPSE_PORT]
    }

    fn exec_before_ready(
        &self,
        _cs: ContainerState,
    ) -> testcontainers::core::error::Result<Vec<ExecCommand>> {
        let register = format!(
            "register_new_matrix_user http://localhost:8008 -c /data/homeserver.yaml -u {} -p {} --admin",
            &self.admin_user, &self.admin_pass
        );
        let loop_cmd = format!("until {register}; do sleep 0.5; done");
        let cmd = ExecCommand::new(["/bin/bash", "-c", loop_cmd.as_str()]);
        Ok(vec![cmd])
    }
}

#[cfg(test)]
mod tests {
    use super::SynapseImage;
    use matrix_sdk::Client;
    use testcontainers::ImageExt;
    use testcontainers::core::logs::consumer::logging_consumer::LoggingConsumer;
    use testcontainers::runners::AsyncRunner;

    #[tokio::test]
    async fn test_login() {
        let image = SynapseImage::default()
            .with_log_consumer(LoggingConsumer::new().with_prefix("SYNAPSE"));
        let container = image.start().await.expect("Failed to start Synapse");

        let host = container.get_host().await.expect("get host");
        let port = container.get_host_port_ipv4(8008).await.expect("get port");
        let user = container.image().admin_user();
        let pass = container.image().admin_pass();
        let server_name = container.image().server_name();

        let client = Client::builder()
            .homeserver_url(format!("http://{host}:{port}"))
            .build()
            .await
            .expect("build client");

        client
            .matrix_auth()
            .login_username(format!("@{user}:{server_name}"), pass)
            .send()
            .await
            .expect("login");

        let session = client.session().expect("session");
        let token = session.access_token();
        assert!(!token.is_empty(), "token must not be empty");
    }
}

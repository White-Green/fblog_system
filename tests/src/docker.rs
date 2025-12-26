use crate::docker::mastodon::MastodonClient;
use crate::docker::misskey::MisskeyClient;
use crate::docker::sharkey::SharkeyClient;
use ini::Ini;
use reqwest::Client;
use std::fs::File;
use std::path::Path;
use tokio::process::Child;

mod mastodon;
mod misskey;
mod sharkey;

pub struct DockerContainers {
    client: Client,
    _process: Child,
}

impl DockerContainers {
    pub fn new(client: Client) -> Self {
        let workspace_dir = Path::new(env!("CARGO_MANIFEST_DIR")).parent().unwrap();
        let process = tokio::process::Command::new("docker")
            .args(["compose", "up", "--build"])
            .current_dir(workspace_dir.join("test_config"))
            .stderr(std::process::Stdio::from(
                File::create_new(workspace_dir.join("logs").join("docker.stderr")).unwrap(),
            ))
            .stdout(std::process::Stdio::from(
                File::create_new(workspace_dir.join("logs").join("docker.stdout")).unwrap(),
            ))
            .stdin(std::process::Stdio::null())
            .spawn()
            .unwrap();
        Self { client, _process: process }
    }

    pub fn misskey_client(&self) -> MisskeyClient<'_> {
        let config = Ini::load_from_str(include_str!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../test_config/misskey/credentials.ini"
        )))
        .unwrap();
        let config = config.section::<&str>(None).unwrap();
        MisskeyClient::new(&self.client, "https://misskey.test", config.get("ACCESS_TOKEN").unwrap())
    }

    pub fn sharkey_client(&self) -> SharkeyClient<'_> {
        let config = Ini::load_from_str(include_str!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../test_config/sharkey/credentials.ini"
        )))
        .unwrap();
        let config = config.section::<&str>(None).unwrap();
        SharkeyClient::new(&self.client, "https://sharkey.test", config.get("ACCESS_TOKEN").unwrap())
    }

    pub fn mastodon_client(&self) -> MastodonClient<'_> {
        let config = Ini::load_from_str(include_str!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../test_config/mastodon/credentials.ini"
        )))
        .unwrap();
        let config = config.section::<&str>(None).unwrap();
        MastodonClient::new(
            &self.client,
            "https://mastodon.test",
            config.get("EMAIL").unwrap(),
            config.get("PASSWORD").unwrap(),
            config.get("CLIENT_KEY").unwrap(),
            config.get("CLIENT_SECRET").unwrap(),
        )
    }
}

impl Drop for DockerContainers {
    fn drop(&mut self) {
        let workspace_dir = Path::new(env!("CARGO_MANIFEST_DIR")).parent().unwrap();
        std::process::Command::new("docker")
            .args(["compose", "down"])
            .current_dir(workspace_dir.join("test_config"))
            .stderr(std::process::Stdio::inherit())
            .stdout(std::process::Stdio::inherit())
            .stdin(std::process::Stdio::null())
            .status()
            .unwrap();
    }
}

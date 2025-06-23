use reqwest::Client;
use std::path::Path;
use std::process::Child;
use std::time::Duration;

pub struct CloudflareWorkers {
    client: Client,
    process: Child,
    port: u16,
}

impl CloudflareWorkers {
    pub fn new(client: Client) -> Self {
        let workspace_dir = Path::new(env!("CARGO_MANIFEST_DIR")).parent().unwrap();
        let port = 8788; // Use a different port than the in-memory server

        let mut process;
        if cfg!(target_os = "windows") {
            process = std::process::Command::new("cmd");
            process.arg("/C");
        } else {
            process = std::process::Command::new("sh");
            process.arg("-c");
        }

        // Start wrangler dev with the test feature enabled
        let process = process
            .current_dir(workspace_dir.join("crates").join("cloudflare_workers"))
            .args(["pnpx", "wrangler", "dev", "--port", &port.to_string(), "--local"])
            .env("CARGO_FEATURE_TEST", "1") // Enable the test feature
            .stderr(std::process::Stdio::inherit())
            .stdout(std::process::Stdio::inherit())
            .stdin(std::process::Stdio::null())
            .spawn()
            .unwrap();

        Self { client, process, port }
    }

    pub async fn server_started(&mut self) -> bool {
        if let Ok(Ok(response)) =
            tokio::time::timeout(Duration::from_secs(1), self.client.get(format!("http://localhost:{}/", self.port)).send()).await
        {
            if response.status().is_success() {
                return true;
            }
            if let Some(code) = self.process.try_wait().unwrap() {
                panic!("wrangler dev exited with status {code}");
            }
        }
        false
    }

    pub async fn test_endpoint(&self) -> reqwest::Response {
        self.client.get(format!("http://localhost:{}/test", self.port)).send().await.unwrap()
    }
}

impl Drop for CloudflareWorkers {
    fn drop(&mut self) {
        self.process.kill().unwrap();
    }
}

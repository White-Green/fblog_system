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

        let output = std::process::Command::new("pnpx")
            .current_dir(workspace_dir.join("crates").join("cloudflare_workers"))
            .args(["wrangler", "d1", "migrations", "apply", "BLOG_DB", "--local"])
            .stderr(std::process::Stdio::inherit())
            .stdout(std::process::Stdio::inherit())
            .stdin(std::process::Stdio::null())
            .output()
            .unwrap();
        assert!(output.status.success());

        // Start wrangler dev with the test feature enabled
        let process = std::process::Command::new("pnpx")
            .current_dir(workspace_dir.join("crates").join("cloudflare_workers"))
            .args(["wrangler", "dev", "--port", &port.to_string(), "--local"])
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

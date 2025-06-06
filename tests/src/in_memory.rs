use fblog_system_core::traits::QueueData;
use reqwest::Client;
use std::path::Path;
use std::process::Child;
use std::time::Duration;

pub struct InMemoryBlog {
    client: Client,
    process: Child,
}

impl InMemoryBlog {
    pub fn new(client: Client) -> Self {
        let workspace_dir = Path::new(env!("CARGO_MANIFEST_DIR")).parent().unwrap();
        let process = std::process::Command::new("cargo")
            .current_dir(workspace_dir)
            .env(
                "ADDITIONAL_CERTIFICATE_PEM",
                workspace_dir.join("test_config/caddy-data/caddy/pki/authorities/local/root.crt"),
            )
            .args(["run", "-p", "fblog_system_in_memory_server", "--target-dir"])
            .arg(workspace_dir.join("target_for_e2e_test"))
            .stderr(std::process::Stdio::inherit())
            .stdout(std::process::Stdio::inherit())
            .stdin(std::process::Stdio::null())
            .spawn()
            .unwrap();
        Self { client, process }
    }

    pub async fn server_started(&self) -> bool {
        if let Ok(Ok(response)) = tokio::time::timeout(Duration::from_secs(1), self.client.get("http://localhost:8787/").send()).await {
            if !response.status().is_server_error() {
                return true;
            }
        }
        false
    }

    pub async fn wait_for_server_start(&self) {
        for _ in 0..1000 {
            if self.server_started().await {
                return;
            }
            tokio::time::sleep(Duration::from_millis(100)).await;
        }
        panic!("timeout");
    }

    pub async fn send_queue_data(&self, queue_data: QueueData) {
        self.client
            .post("http://localhost:8787/job_queue")
            .json(&queue_data)
            .send()
            .await
            .unwrap();
    }

    pub async fn replace_article_ap(&self, slug: &str, payload: String) {
        self.client
            .put(format!("http://localhost:8787/article_ap?slug={slug}"))
            .body(payload)
            .send()
            .await
            .unwrap();
    }

    pub async fn delete_article(&self, slug: &str) {
        self.client.delete(format!("http://localhost:8787/articles/{slug}")).send().await.unwrap();
    }
}

impl Drop for InMemoryBlog {
    fn drop(&mut self) {
        self.process.kill().unwrap();
    }
}

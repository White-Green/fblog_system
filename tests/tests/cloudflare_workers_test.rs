use e2e_test::cloudflare_workers::CloudflareWorkers;
use reqwest::Client;
use std::time::Duration;

async fn wait_for(mut pred: impl AsyncFnMut() -> bool) {
    for _ in 0..60 * 20 {
        if pred().await {
            return;
        }
        tokio::time::sleep(Duration::from_millis(1000)).await;
    }
    panic!("timeout");
}

#[test]
fn test_cloudflare_workers() {
    tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()
        .unwrap()
        .block_on(async {
            let client = Client::new();
            let mut cloudflare_workers = CloudflareWorkers::new(client);
            wait_for(async || cloudflare_workers.server_started().await).await;
            let response = cloudflare_workers.test_endpoint().await;
            assert!(response.status().is_success());
        });
}

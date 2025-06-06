use e2e_test::docker::DockerContainers;
use e2e_test::in_memory::InMemoryBlog;
use fblog_system_core::traits::QueueData;
use reqwest::{Certificate, Client};
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
fn main() {
    tokio::runtime::Builder::new_multi_thread().enable_all().build().unwrap().block_on(async {
        let client = Client::builder()
            .resolve("misskey.test", "127.0.0.1:443".parse().unwrap())
            .resolve("mastodon.test", "127.0.0.1:443".parse().unwrap())
            .resolve("sharkey.test", "127.0.0.1:443".parse().unwrap())
            .add_root_certificate(
                Certificate::from_pem(include_bytes!(concat!(
                    env!("CARGO_MANIFEST_DIR"),
                    "/../test_config/caddy-data/caddy/pki/authorities/local/root.crt"
                )))
                .unwrap(),
            )
            .build()
            .unwrap();

        let docker = DockerContainers::new(client.clone());
        let in_memory = InMemoryBlog::new(client.clone());
        let misskey = docker.misskey_client();
        let mut mastodon = docker.mastodon_client();
        let sharkey = docker.sharkey_client();

        tokio::join!(
            wait_for(async || in_memory.server_started().await),
            wait_for(async || misskey.server_started().await),
            wait_for(async || mastodon.server_started().await),
            wait_for(async || sharkey.server_started().await),
        );

        let (sharkey_note, misskey_note, mastodon_note) = tokio::try_join!(
            sharkey.get_note("https://blog.test/articles/first-post"),
            misskey.get_note("https://blog.test/articles/first-post"),
            mastodon.get_note("https://blog.test/articles/first-post"),
        )
        .unwrap();

        tokio::try_join!(
            sharkey.follow(sharkey_note["object"]["user"]["id"].as_str().unwrap()),
            misskey.follow(misskey_note["object"]["user"]["id"].as_str().unwrap()),
            mastodon.follow(mastodon_note["account"]["id"].as_str().unwrap()),
        )
        .unwrap();

        tokio::join!(
            async { assert_eq!(sharkey.fetch_timeline().await.unwrap().len(), 1) },
            async { assert_eq!(misskey.fetch_timeline().await.unwrap().len(), 1) },
            async { assert_eq!(mastodon.fetch_timeline().await.unwrap().len(), 1) },
        );

        in_memory
            .send_queue_data(QueueData::DeliveryNewArticleToAll {
                slug: "markdown-style-guide".to_owned(),
            })
            .await;

        tokio::join!(
            wait_for(async || sharkey.fetch_timeline().await.unwrap().len() == 2),
            wait_for(async || misskey.fetch_timeline().await.unwrap().len() == 2),
            wait_for(async || mastodon.fetch_timeline().await.unwrap().len() == 2),
        );

        let mut new_article_ap = serde_json::from_str::<serde_json::Value>(include_str!("../../dist/articles/markdown-style-guide.json")).unwrap();
        new_article_ap["content"] = serde_json::Value::String("Updated content".to_owned());
        new_article_ap["updated"] = serde_json::Value::String("2025-06-19T00:00:00.000Z".to_owned());
        in_memory
            .replace_article_ap("markdown-style-guide", serde_json::to_string(&new_article_ap).unwrap())
            .await;
        in_memory
            .send_queue_data(QueueData::DeliveryUpdateArticleToAll {
                slug: "markdown-style-guide".to_owned(),
            })
            .await;

        tokio::time::sleep(Duration::from_secs(10)).await;

        tokio::join!(
            async { sharkey.fetch_timeline().await.unwrap().iter().any(|note| note["text"] == "Updated content") },
            async { misskey.fetch_timeline().await.unwrap().iter().any(|note| note["text"] == "Updated content") },
            async { mastodon.fetch_timeline().await.unwrap().iter().any(|note| note["content"] == "Updated content") },
        );

        in_memory.delete_article("markdown-style-guide").await;
        in_memory
            .send_queue_data(QueueData::DeliveryDeleteArticleToAll {
                slug: "markdown-style-guide".to_owned(),
                author: "default".to_owned(),
            })
            .await;

        tokio::join!(
            wait_for(async || sharkey.fetch_timeline().await.unwrap().len() == 1),
            wait_for(async || misskey.fetch_timeline().await.unwrap().len() == 1),
            wait_for(async || mastodon.fetch_timeline().await.unwrap().len() == 1),
        );
    });
}

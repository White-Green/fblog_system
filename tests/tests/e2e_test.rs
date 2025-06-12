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

        assert!(sharkey_note["object"]["text"].as_str().unwrap().starts_with("[【First post】](https://blog.test/articles/first-post)"));
        assert!(misskey_note["object"]["text"].as_str().unwrap().starts_with("[【First post】](https://blog.test/articles/first-post)"));
        assert!(mastodon_note["content"].as_str().unwrap().starts_with("<a href=\"https://blog.test/articles/first-post\" rel=\"nofollow noopener noreferrer\" target=\"_blank\"><strong>【First post】</strong></a>"));

        let (sharkey_deep, misskey_deep, mastodon_deep) = tokio::try_join!(
            sharkey.get_note("https://blog.test/articles/2025/06/nested-post"),
            misskey.get_note("https://blog.test/articles/2025/06/nested-post"),
            mastodon.get_note("https://blog.test/articles/2025/06/nested-post"),
        )
            .unwrap();

        assert!(sharkey_deep["object"]["text"].as_str().unwrap().starts_with("[【Nested Post】](https://blog.test/articles/2025/06/nested-post)"));
        assert!(misskey_deep["object"]["text"].as_str().unwrap().starts_with("[【Nested Post】](https://blog.test/articles/2025/06/nested-post)"));
        assert!(mastodon_deep["content"].as_str().unwrap().starts_with("<a href=\"https://blog.test/articles/2025/06/nested-post\" rel=\"nofollow noopener noreferrer\" target=\"_blank\"><strong>【Nested Post】</strong></a>"));

        tokio::try_join!(
            sharkey.follow(sharkey_note["object"]["user"]["id"].as_str().unwrap()),
            misskey.follow(misskey_note["object"]["user"]["id"].as_str().unwrap()),
            mastodon.follow(mastodon_note["account"]["id"].as_str().unwrap()),
        )
        .unwrap();

        tokio::join!(
            async { assert_eq!(dbg!(sharkey.fetch_timeline().await.unwrap()).len(), 2) },
            async { assert_eq!(dbg!(misskey.fetch_timeline().await.unwrap()).len(), 2) },
            async { assert_eq!(dbg!(mastodon.fetch_timeline().await.unwrap()).len(), 2) },
        );

        tokio::try_join!(
            sharkey.renote(sharkey_note["object"]["id"].as_str().unwrap()),
            misskey.renote(misskey_note["object"]["id"].as_str().unwrap()),
            mastodon.renote(mastodon_note["id"].as_str().unwrap()),
        )
        .unwrap();

        tokio::try_join!(
            sharkey.quote_renote(sharkey_note["object"]["id"].as_str().unwrap(), "quote"),
            misskey.quote_renote(misskey_note["object"]["id"].as_str().unwrap(), "quote"),
            mastodon.quote_renote(mastodon_note["id"].as_str().unwrap(), "quote"),
        )
        .unwrap();

        tokio::try_join!(
            sharkey.reply(sharkey_note["object"]["id"].as_str().unwrap(), "reply"),
            misskey.reply(misskey_note["object"]["id"].as_str().unwrap(), "reply"),
            mastodon.reply(mastodon_note["id"].as_str().unwrap(), "reply"),
        )
        .unwrap();

        tokio::try_join!(
            sharkey.react(sharkey_note["object"]["id"].as_str().unwrap(), "👍"),
            misskey.react(misskey_note["object"]["id"].as_str().unwrap(), "👍"),
            mastodon.react(mastodon_note["id"].as_str().unwrap()),
        )
        .unwrap();

        wait_for(async || in_memory.get_comments_raw().await.as_array().unwrap().len() >= 6).await;
        assert_eq!(in_memory.get_comments_raw().await.as_array().unwrap().len(), 6);

        in_memory
            .send_queue_data(QueueData::DeliveryNewArticleToAll {
                slug: "markdown-style-guide".to_owned(),
            })
            .await;

        wait_for(async || in_memory.job_queue_len().await == 0).await;

        tokio::join!(
            wait_for(async || sharkey.fetch_timeline().await.unwrap().len() == 6),
            wait_for(async || misskey.fetch_timeline().await.unwrap().len() == 6),
            wait_for(async || mastodon.fetch_timeline().await.unwrap().len() == 6),
        );

        let mut new_article_ap = serde_json::from_str::<serde_json::Value>(include_str!("../../dist/raw__/articles/ap/markdown-style-guide.json")).unwrap();
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

        wait_for(async || in_memory.job_queue_len().await == 0).await;

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

        wait_for(async || in_memory.job_queue_len().await == 0).await;

        tokio::join!(
            wait_for(async || sharkey.fetch_timeline().await.unwrap().len() == 5),
            wait_for(async || misskey.fetch_timeline().await.unwrap().len() == 5),
            wait_for(async || mastodon.fetch_timeline().await.unwrap().len() == 5),
        );

        tokio::try_join!(
            sharkey.unfollow(sharkey_note["object"]["user"]["id"].as_str().unwrap()),
            misskey.unfollow(misskey_note["object"]["user"]["id"].as_str().unwrap()),
            mastodon.unfollow(mastodon_note["account"]["id"].as_str().unwrap()),
        )
        .unwrap();

        wait_for(async || in_memory.job_queue_len().await == 0).await;

        in_memory
            .send_queue_data(QueueData::DeliveryNewArticleToAll {
                slug: "second-post".to_owned(),
            })
            .await;

        wait_for(async || in_memory.job_queue_len().await == 0).await;

        tokio::time::sleep(Duration::from_secs(10)).await;

        tokio::join!(
            async { assert_eq!(sharkey.fetch_timeline().await.unwrap().len(), 5) },
            async { assert_eq!(misskey.fetch_timeline().await.unwrap().len(), 5) },
            async { assert_eq!(mastodon.fetch_timeline().await.unwrap().len(), 5) },
        );
    });
}

use reqwest::Client;
use reqwest::header::{ACCEPT, };
use std::error::Error;
use std::time::Duration;

/// Misskey APIクライアント
pub struct MastodonClient<'a> {
    base_url: String,
    email: String,
    password: String,
    client_key: String,
    client_secret: String,
    token: Option<String>,
    client: &'a Client,
}

impl MastodonClient<'_> {
    pub fn new<'a>(client: &'a Client, base_url: &str, email: &str, password: &str, client_key: &str, client_secret: &str) -> MastodonClient<'a> {
        MastodonClient {
            base_url: base_url.trim_end_matches('/').to_string(),
            email: email.to_owned(),
            password: password.to_owned(),
            client_key: client_key.to_owned(),
            client_secret: client_secret.to_owned(),
            token: None,
            client,
        }
    }

    pub async fn server_started(&mut self) -> bool {
        if let Ok(Ok(response)) = tokio::time::timeout(Duration::from_secs(1), self.client.get(&self.base_url).send()).await {
            if !response.status().is_server_error() {
                let response = self
                    .client
                    .post(format!("{}/oauth/token", self.base_url))
                    .header("Content-Type", "application/json")
                    .body(
                        serde_json::to_string(&serde_json::json!({
                    "client_id": self.client_key.as_str(),
                    "client_secret": self.client_secret.as_str(),
                    "grant_type": "password",
                    "username": self.email.as_str(),
                    "password": self.password.as_str(),
                    "scope": "read write follow",
                }))
                            .unwrap(),
                    )
                    .send()
                    .await
                    .unwrap();

                if !response.status().is_success() {
                    let error_text = response.text().await.unwrap();
                    panic!("Failed to get token: {}", error_text);
                }
                let response = response.json::<serde_json::Value>().await.unwrap();
                self.token = Some(response["access_token"].as_str().unwrap().to_owned());

                return true;
            }
        }
        false
    }

    pub async fn get_note(&self, uri: &str) -> Result<serde_json::Value, Box<dyn Error>> {
        let response = self
            .client
            .get(format!("{}/api/v2/search", self.base_url))
            .query(&[("q", uri), ("resolve", "true")])
            .bearer_auth(self.token.as_ref().unwrap())
            .header(ACCEPT, "application/json")
            .send()
            .await
            .unwrap();
        let succeed = response.status().is_success();
        let response_dump = format!("{response:#?}");
        if succeed {
            let body: serde_json::Value = response.json().await.unwrap();
            let statuses = &body["statuses"];
            let statuses = statuses.as_array().unwrap();
            assert_eq!(statuses.len(), 1);
            Ok(statuses[0].clone())
        } else {
            let body = response.text().await.unwrap();
            Err(format!("{response_dump}\n{}", body).into())
        }
    }

    pub async fn follow(&self, user_id: &str) -> Result<serde_json::Value, Box<dyn Error>> {
        let response = self
            .client
            .post(format!("{}/api/v1/accounts/{user_id}/follow", self.base_url))
            .bearer_auth(self.token.as_ref().unwrap())
            .header(ACCEPT, "application/json")
            .send()
            .await
            .unwrap();
        loop {
            let response = self
                .client
                .get(format!("{}/api/v1/accounts/{user_id}", self.base_url))
                .bearer_auth(self.token.as_ref().unwrap())
                .header(ACCEPT, "application/json")
                .send()
                .await
                .unwrap().json::<serde_json::Value>().await.unwrap();
            if response["followers_count"].as_i64().unwrap() == 1 {
                break;
            }
            tokio::time::sleep(Duration::from_millis(100)).await;
        }
        let succeed = response.status().is_success();
        let response_dump = format!("{response:#?}");
        if succeed {
            Ok(response.json().await.unwrap())
        } else {
            let body = response.text().await.unwrap();
            Err(format!("{response_dump}\n{}", body).into())
        }
    }

    pub async fn fetch_timeline(&self) -> Result<Vec<serde_json::Value>, Box<dyn Error>> {
        let response = self
            .client
            .get(format!("{}/api/v1/timelines/public", self.base_url))
            .bearer_auth(self.token.as_ref().unwrap())
            .header(ACCEPT, "application/json")
            .send()
            .await
            .unwrap();
        let succeed = response.status().is_success();
        let response_dump = format!("{response:#?}");
        if succeed {
            Ok(response.json().await.unwrap())
        } else {
            let body = response.text().await.unwrap();
            Err(format!("{response_dump}\n{}", body).into())
        }
    }
}

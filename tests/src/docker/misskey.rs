use reqwest::Client;
use std::error::Error;
use std::time::Duration;

/// Misskey APIクライアント
pub struct MisskeyClient<'a> {
    base_url: String,
    token: String,
    client: &'a Client,
}

impl MisskeyClient<'_> {
    pub fn new<'a>(client: &'a Client, base_url: &str, token: &str) -> MisskeyClient<'a> {
        MisskeyClient {
            base_url: base_url.trim_end_matches('/').to_string(),
            token: token.to_owned(),
            client,
        }
    }

    pub async fn server_started(&self) -> bool {
        if let Ok(Ok(response)) = tokio::time::timeout(Duration::from_secs(1), self.client.get(&self.base_url).send()).await
            && !response.status().is_server_error()
        {
            return true;
        }
        false
    }

    pub async fn get_note(&self, uri: &str) -> Result<serde_json::Value, Box<dyn Error>> {
        let response = self
            .client
            .post(format!("{}/api/ap/show", self.base_url))
            .header("Content-Type", "application/json")
            .body(serde_json::to_string(&serde_json::json!({"i": self.token, "uri": uri})).unwrap())
            .send()
            .await
            .unwrap();
        let succeed = response.status().is_success();
        let response_dump = format!("{response:#?}");
        let body = response.json().await.unwrap();
        if succeed {
            Ok(body)
        } else {
            Err(format!("{response_dump}\n{:#?}", body).into())
        }
    }

    pub async fn follow(&self, user_id: &str) -> Result<serde_json::Value, Box<dyn Error>> {
        let response = self
            .client
            .post(format!("{}/api/following/create", self.base_url))
            .header("Content-Type", "application/json")
            .body(serde_json::to_string(&serde_json::json!({"i": self.token, "userId": user_id})).unwrap())
            .send()
            .await
            .unwrap();
        loop {
            let response = self
                .client
                .post(format!("{}/api/i", self.base_url))
                .header("Content-Type", "application/json")
                .body(serde_json::to_string(&serde_json::json!({"i": self.token})).unwrap())
                .send()
                .await
                .unwrap()
                .json::<serde_json::Value>()
                .await
                .unwrap();
            if response["followingCount"].as_i64().unwrap() == 1 {
                break;
            }
            tokio::time::sleep(Duration::from_millis(100)).await;
        }
        let succeed = response.status().is_success();
        let response_dump = format!("{response:#?}");
        let body = response.json().await.unwrap();
        if succeed {
            Ok(body)
        } else {
            Err(format!("{response_dump}\n{:#?}", body).into())
        }
    }

    pub async fn unfollow(&self, user_id: &str) -> Result<serde_json::Value, Box<dyn Error>> {
        let response = self
            .client
            .post(format!("{}/api/following/delete", self.base_url))
            .header("Content-Type", "application/json")
            .body(serde_json::to_string(&serde_json::json!({ "i": self.token, "userId": user_id })).unwrap())
            .send()
            .await
            .unwrap();
        loop {
            let response = self
                .client
                .post(format!("{}/api/i", self.base_url))
                .header("Content-Type", "application/json")
                .body(serde_json::to_string(&serde_json::json!({ "i": self.token })).unwrap())
                .send()
                .await
                .unwrap()
                .json::<serde_json::Value>()
                .await
                .unwrap();
            if response["followingCount"].as_i64().unwrap() == 0 {
                break;
            }
            tokio::time::sleep(Duration::from_millis(100)).await;
        }
        let succeed = response.status().is_success();
        let response_dump = format!("{response:#?}");
        let body = response.json().await.unwrap();
        if succeed {
            Ok(body)
        } else {
            Err(format!("{response_dump}\n{:#?}", body).into())
        }
    }

    pub async fn fetch_timeline(&self) -> Result<Vec<serde_json::Value>, Box<dyn Error>> {
        let response = self
            .client
            .post(format!("{}/api/notes/global-timeline", self.base_url))
            .header("Content-Type", "application/json")
            .body(serde_json::to_string(&serde_json::json!({"i": self.token})).unwrap())
            .send()
            .await
            .unwrap();
        let succeed = response.status().is_success();
        let response_dump = format!("{response:#?}");
        let body = response.json().await.unwrap();
        if succeed {
            Ok(body)
        } else {
            Err(format!("{response_dump}\n{:#?}", body).into())
        }
    }

    pub async fn renote(&self, note_id: &str) -> Result<serde_json::Value, Box<dyn Error>> {
        let response = self
            .client
            .post(format!("{}/api/notes/create", self.base_url))
            .header("Content-Type", "application/json")
            .body(serde_json::to_string(&serde_json::json!({"i": self.token, "renoteId": note_id})).unwrap())
            .send()
            .await
            .unwrap();
        let succeed = response.status().is_success();
        let response_dump = format!("{response:#?}");
        let body = response.json().await.unwrap();
        if succeed {
            Ok(body)
        } else {
            Err(format!("{response_dump}\n{:#?}", body).into())
        }
    }

    pub async fn quote_renote(&self, note_id: &str, text: &str) -> Result<serde_json::Value, Box<dyn Error>> {
        let response = self
            .client
            .post(format!("{}/api/notes/create", self.base_url))
            .header("Content-Type", "application/json")
            .body(serde_json::to_string(&serde_json::json!({"i": self.token, "renoteId": note_id, "text": text})).unwrap())
            .send()
            .await
            .unwrap();
        let succeed = response.status().is_success();
        let response_dump = format!("{response:#?}");
        let body = response.json().await.unwrap();
        if succeed {
            Ok(body)
        } else {
            Err(format!("{response_dump}\n{:#?}", body).into())
        }
    }

    pub async fn reply(&self, note_id: &str, text: &str) -> Result<serde_json::Value, Box<dyn Error>> {
        let response = self
            .client
            .post(format!("{}/api/notes/create", self.base_url))
            .header("Content-Type", "application/json")
            .body(serde_json::to_string(&serde_json::json!({"i": self.token, "replyId": note_id, "text": text})).unwrap())
            .send()
            .await
            .unwrap();
        let succeed = response.status().is_success();
        let response_dump = format!("{response:#?}");
        let body = response.json().await.unwrap();
        if succeed {
            Ok(body)
        } else {
            Err(format!("{response_dump}\n{:#?}", body).into())
        }
    }

    pub async fn reaction(&self, note_id: &str, reaction: &str) -> Result<(), Box<dyn Error>> {
        let response = self
            .client
            .post(format!("{}/api/notes/reactions/create", self.base_url))
            .header("Content-Type", "application/json")
            .body(serde_json::to_string(&serde_json::json!({"i": self.token, "noteId": note_id, "reaction": reaction})).unwrap())
            .send()
            .await
            .unwrap();
        let succeed = response.status().is_success();
        if succeed { Ok(()) } else { Err(format!("{response:?}").into()) }
    }

    pub async fn delete_reaction(&self, note_id: &str) -> Result<(), Box<dyn Error>> {
        let response = self
            .client
            .post(format!("{}/api/notes/reactions/delete", self.base_url))
            .header("Content-Type", "application/json")
            .body(serde_json::to_string(&serde_json::json!({"i": self.token, "noteId": note_id})).unwrap())
            .send()
            .await
            .unwrap();
        let succeed = response.status().is_success();
        if succeed { Ok(()) } else { Err(format!("{response:?}").into()) }
    }
}

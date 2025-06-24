use crate::WorkerState;
use fblog_system_core::traits::{ArticleNewReaction, ArticleProvider, Env, UserProvider};
use serde_json::json;
use std::collections::HashSet;

pub async fn run_all_tests(state: WorkerState) {
    test_basic_methods(&state).await;
    test_env_trait_methods(&state).await;
    test_article_provider_methods(&state).await;
    test_user_provider_methods(&state).await;
    test_reaction_methods(&state).await;
}

async fn test_basic_methods(state: &WorkerState) {
    state.assets();
    state.r2();
}

async fn test_env_trait_methods(state: &WorkerState) {
    assert_eq!(state.url().to_string(), "https://local.test");
    state.signing_key();
}

async fn test_article_provider_methods(state: &WorkerState) {
    // Test exists_article
    assert!(state.exists_article("article1").await);
    assert!(!state.exists_article("non_existent_article").await);

    // Test get_article_html
    let article_html = state.get_article_html("article1").await;
    assert!(article_html.is_some());
    let non_existent_article_html = state.get_article_html("non_existent_article").await;
    assert!(non_existent_article_html.is_none());

    // Test get_article_ap
    let article_ap = state.get_article_ap("article1").await;
    assert!(article_ap.is_some());
    let non_existent_article_ap = state.get_article_ap("non_existent_article").await;
    assert!(non_existent_article_ap.is_none());

    // Test get_author_id
    let author_id = state.get_author_id("article1").await;
    assert!(author_id.is_some());
    let non_existent_author_id = state.get_author_id("non_existent_article").await;
    assert!(non_existent_author_id.is_none());

    // Test nested article paths
    let nested_article_html = state.get_article_html("dir0/dir1/dir2/4th-article").await;
    assert!(nested_article_html.is_some());
}

async fn test_user_provider_methods(state: &WorkerState) {
    // Test exists_user
    assert!(state.exists_user("user1").await);
    assert!(state.exists_user("user2").await);
    assert!(!state.exists_user("non_existent_user").await);

    // Test get_user_html
    let user_html = state.get_user_html("user1").await;
    assert!(user_html.is_some());
    let non_existent_user_html = state.get_user_html("non_existent_user").await;
    assert!(non_existent_user_html.is_none());

    // Test get_user_ap
    let user_ap = state.get_user_ap("user1").await;
    assert!(user_ap.is_some());
    let non_existent_user_ap = state.get_user_ap("non_existent_user").await;
    assert!(non_existent_user_ap.is_none());

    let username = "user1";
    let mut expect_all_followers_inbox = HashSet::new();

    for c in 'a'..='z' {
        let follower_id1 = format!("https://{c}.test/user1");
        let inbox_url = format!("https://{c}.test/inbox");
        let event_id = format!("https://{c}.test/follow/event-1");
        state.add_follower("user1", &follower_id1, &inbox_url, &event_id).await;

        let follower_id2 = format!("https://{c}.test/user2");
        let event_id = format!("https://{c}.test/follow/event-2");
        state.add_follower("user1", &follower_id2, &inbox_url, &event_id).await;

        expect_all_followers_inbox.insert(inbox_url);
    }

    let mut actual_all_followers_inbox = HashSet::new();
    let mut last_inbox = String::new();
    loop {
        let (inboxes, next_last_inbox) = state.get_followers_inbox_batch(username, &last_inbox).await;
        if inboxes.is_empty() {
            break;
        } else {
            actual_all_followers_inbox.extend(inboxes.iter().cloned());
            last_inbox = next_last_inbox;
        }
    }

    assert_eq!(actual_all_followers_inbox, expect_all_followers_inbox);
}

async fn test_reaction_methods(state: &WorkerState) {
    // initial count should be zero
    assert_eq!(state.reaction_count("article1").await, 0);

    // add first reaction
    let event_id1 = "https://actor1.test/events/reaction-1";
    let actor_id1 = "https://actor1.test/users/actor1";
    let raw1 = serde_json::to_string(&json!({
        "@context": "https://www.w3.org/ns/activitystreams",
        "id": event_id1,
        "type": "Like",
        "actor": actor_id1,
        "object": format!("{}/articles/{}", state.url(), "article1"),
        "content": "👍"
    }))
    .unwrap();
    let reaction1 = ArticleNewReaction {
        id: event_id1.to_owned(),
        author_id: actor_id1.to_owned(),
        reaction: "👍".to_owned(),
        proceed_at: state.timestamp_now(),
        raw: raw1,
    };
    state.add_reaction("article1", reaction1).await;
    assert_eq!(state.reaction_count("article1").await, 1);

    // add second reaction by another actor
    let event_id2 = "https://actor2.test/events/reaction-2";
    let actor_id2 = "https://actor2.test/users/actor2";
    let raw2 = serde_json::to_string(&json!({
        "@context": "https://www.w3.org/ns/activitystreams",
        "id": event_id2,
        "type": "Like",
        "actor": actor_id2,
        "object": format!("{}/articles/{}", state.url(), "article1"),
        "content": "❤️"
    }))
    .unwrap();
    let reaction2 = ArticleNewReaction {
        id: event_id2.to_owned(),
        author_id: actor_id2.to_owned(),
        reaction: "❤️".to_owned(),
        proceed_at: state.timestamp_now(),
        raw: raw2,
    };
    state.add_reaction("article1", reaction2).await;
    assert_eq!(state.reaction_count("article1").await, 2);

    // remove reactions of actor1
    state
        .remove_reaction_by("article1", "https://actor1.test/users/actor1")
        .await;
    assert_eq!(state.reaction_count("article1").await, 1);

    // removing again should not make count negative
    state
        .remove_reaction_by("article1", "https://actor1.test/users/actor1")
        .await;
    assert_eq!(state.reaction_count("article1").await, 0);
}

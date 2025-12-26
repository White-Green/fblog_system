use crate::traits::{ArticleProvider, Env, HTTPClient, Queue, UserProvider};
use axum::Router;
use axum::routing::{get, post};

mod articles;
mod users;
mod well_known;

pub fn router<E, S>(state: E) -> Router<S>
where
    E: Env + ArticleProvider + UserProvider + HTTPClient + Queue + Send + Sync + Clone + 'static,
{
    Router::<E>::new()
        .route("/.well-known/webfinger", get(well_known::webfinger::get_webfinger::<E>))
        .route("/users/{username}", get(users::user_get::<E>))
        .route("/users/{username}/inbox", post(users::inbox::user_inbox_post::<E>))
        .route("/users/{username}/outbox", get(users::outbox::user_outbox_get::<E>))
        .route("/users/{username}/accept_follow", get(users::accept_follow::user_accept_follow_get::<E>))
        .route("/articles/{*slug}", get(articles::article_or_comments_get::<E>))
        .route("/events/articles/create/{*slug}", get(articles::events::article_create_events_get::<E>))
        .route("/events/articles/update/{*slug}", get(articles::events::article_update_events_get::<E>))
        .route("/events/articles/delete/{*slug}", get(articles::events::article_delete_events_get::<E>))
        .with_state(state)
}

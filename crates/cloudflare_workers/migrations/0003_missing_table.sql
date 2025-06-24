-- Migration number: 0003 	 2025-06-24T05:19:16.871Z

CREATE TABLE reaction_actors
(
    slug        TEXT,
    actor_id    TEXT,
    reaction_id TEXT,
    PRIMARY KEY (slug, actor_id)
);

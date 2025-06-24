-- Migration number: 0001 	 2025-06-22T04:48:10.690Z

CREATE TABLE followers
(
    username    TEXT,
    follower_id TEXT,
    inbox       TEXT,
    event_id    TEXT
);

CREATE TABLE comments
(
    slug  TEXT PRIMARY KEY,
    count INTEGER DEFAULT 0
);

CREATE TABLE reactions
(
    slug  TEXT PRIMARY KEY,
    count INTEGER DEFAULT 0
);

-- Migration number: 0002 	 2025-06-24T05:17:37.646Z

CREATE INDEX idx_followers_user_event ON followers (username, event_id);
CREATE INDEX idx_followers_user_follower ON followers (username, follower_id);
CREATE INDEX idx_followers_user_inbox ON followers (username, inbox);

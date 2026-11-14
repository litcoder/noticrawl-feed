-- Create table if it is not exist.
CREATE TABLE IF NOT EXISTS announcements (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    title TEXT NOT NULL,
    content TEXT NOT NULL,
    ann_hash TEXT NOT NULL UNIQUE,
    notified INTEGER DEFAULT 0
);
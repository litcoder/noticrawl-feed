-- Insert an item with notified true if it doesn't exist.
-- Update the notified flag to true if it already in the table.
INSERT INTO announcements (title, content, ann_hash, notified)
VALUES (?1, ?2, ?3, 1)
ON CONFLICT(ann_hash)
DO UPDATE SET notified = 1
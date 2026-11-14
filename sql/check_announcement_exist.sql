-- Find the same announcement hash entry in the table.
SELECT EXISTS(SELECT 1 FROM announcements WHERE ann_hash = ?1)
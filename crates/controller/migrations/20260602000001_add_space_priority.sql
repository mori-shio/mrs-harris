ALTER TABLE spaces
    ADD COLUMN priority INT NULL AFTER description;

UPDATE spaces
SET priority = id
WHERE priority IS NULL;

ALTER TABLE spaces
    MODIFY COLUMN priority INT NOT NULL;

CREATE INDEX idx_spaces_priority ON spaces(priority, id);

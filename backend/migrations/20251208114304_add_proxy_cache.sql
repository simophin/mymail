CREATE TABLE external_cache(
    account_id INTEGER NOT NULL,
    url TEXT NOT NULL COLLATE NOCASE,
    mime_type TEXT,
    value BLOB NOT NULL,
    last_accessed DATETIME NOT NULL,
    FOREIGN KEY (account_id) REFERENCES accounts(id) ON DELETE CASCADE,
    PRIMARY KEY (account_id, url)
);

CREATE INDEX idx_external_cache_account_id ON external_cache(account_id);
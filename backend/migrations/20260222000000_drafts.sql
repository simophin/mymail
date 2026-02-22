CREATE TABLE drafts (
    id TEXT PRIMARY KEY NOT NULL,
    account_id INTEGER NOT NULL REFERENCES accounts(id) ON DELETE CASCADE,
    -- Server-assigned JMAP email ID; NULL until the draft has been synced to the server
    jmap_email_id TEXT,
    -- Full JSON-serialized EmailDraft (to, cc, subject, body, attachments, etc.)
    data TEXT NOT NULL,
    updated_at INTEGER NOT NULL DEFAULT (unixepoch()),
    created_at INTEGER NOT NULL DEFAULT (unixepoch())
);

CREATE INDEX idx_drafts_account_id ON drafts(account_id);

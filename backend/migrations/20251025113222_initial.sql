CREATE TABLE accounts (
    id INTEGER NOT NULL PRIMARY KEY AUTOINCREMENT,
    url TEXT NOT NULL,
    credentials TEXT NOT NULL,
    name TEXT NOT NULL,
    mailboxes_sync_state TEXT
);

CREATE TABLE identities (
    account_id INTEGER NOT NULL REFERENCES accounts(id) ON DELETE CASCADE,
    id TEXT NOT NULL,
    email TEXT NOT NULL,
    name TEXT NOT NULL,
    signature TEXT,
    PRIMARY KEY (account_id, id)
);

CREATE INDEX idx_identities_account_id ON identities(account_id);

CREATE TABLE mailboxes (
    account_id INTEGER NOT NULL REFERENCES accounts(id) ON DELETE CASCADE,
    id TEXT NOT NULL,
    jmap_data TEXT NOT NULL,
    PRIMARY KEY (account_id, id)
);
CREATE INDEX idx_mailboxes_account_id ON mailboxes(account_id);

CREATE TABLE emails(
    account_id INTEGER NOT NULL REFERENCES accounts(id) ON DELETE CASCADE,
    id TEXT NOT NULL,
    jmap_data TEXT,
    thread_id TEXT GENERATED ALWAYS AS (jmap_data->>'$.threadId') VIRTUAL,
    sent_at TEXT GENERATED ALWAYS AS (jmap_data->>'$.sentAt') VIRTUAL,
    received_at TEXT GENERATED ALWAYS AS (jmap_data->>'$.receivedAt') VIRTUAL,
    subject TEXT GENERATED ALWAYS AS (jmap_data->>'$.subject') VIRTUAL,
    text_body TEXT GENERATED ALWAYS AS (jmap_data->'$.textBody') VIRTUAL,
    html_body TEXT GENERATED ALWAYS AS (jmap_data->'$.htmlBody') VIRTUAL,
    `from` TEXT GENERATED ALWAYS AS (jmap_data->'$.from') VIRTUAL,
    `to` TEXT GENERATED ALWAYS AS (jmap_data->'$.to') VIRTUAL,
    `cc` TEXT GENERATED ALWAYS AS (jmap_data->'$.cc') VIRTUAL,
    PRIMARY KEY (account_id, id)
);

CREATE TABLE blobs(
    account_id INTEGER NOT NULL,
    email_id TEXT NOT NULL,
    id INTEGER NOT NULL PRIMARY KEY AUTOINCREMENT,
    blob_id TEXT, -- The ID on the server
    mime_type TEXT,
    name TEXT,
    data BLOB NOT NULL,
    FOREIGN KEY (account_id, email_id) REFERENCES emails(account_id, id) ON DELETE CASCADE
);

CREATE TABLE mailbox_emails (
    account_id INTEGER NOT NULL,
    mailbox_id TEXT NOT NULL,
    email_id TEXT NOT NULL,
    PRIMARY KEY (account_id, mailbox_id, email_id),
    FOREIGN KEY (account_id, mailbox_id) REFERENCES mailboxes(account_id, id) ON DELETE CASCADE,
    FOREIGN KEY (account_id, email_id) REFERENCES emails(account_id, id) ON DELETE CASCADE
);
CREATE INDEX idx_mailbox_emails_account_mailbox ON mailbox_emails(account_id, mailbox_id);
CREATE INDEX idx_mailbox_emails_account_email ON mailbox_emails(account_id, email_id);

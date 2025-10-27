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
    name TEXT NOT NULL,
    role TEXT,
    sort_order INTEGER NOT NULL,
    total_emails INTEGER NOT NULL,
    unread_emails INTEGER NOT NULL,
    total_threads INTEGER NOT NULL,
    unread_threads INTEGER NOT NULL,
    my_rights TEXT,
    parent_id TEXT,
    email_sync_state TEXT DEFAULT NULL,
    PRIMARY KEY (account_id, id)
);
CREATE INDEX idx_mailboxes_account_id ON mailboxes(account_id);

CREATE TABLE emails(
    account_id INTEGER NOT NULL REFERENCES accounts(id) ON DELETE CASCADE,
    id TEXT NOT NULL,
    subject TEXT,
    "from" TEXT,
    "to" TEXT,
    cc TEXT,
    bcc TEXT,
    reply_to TEXT,
    date_sent TIMESTAMP,
    date_received TIMESTAMP,
    downloaded BOOLEAN NOT NULL DEFAULT FALSE,
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

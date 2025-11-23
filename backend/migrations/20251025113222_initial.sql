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
) WITHOUT ROWID;

CREATE INDEX idx_identities_account_id ON identities(account_id);

CREATE TABLE mailboxes (
    account_id INTEGER NOT NULL REFERENCES accounts(id) ON DELETE CASCADE,
    id TEXT NOT NULL,
    jmap_data TEXT NOT NULL,
    email_sync_state TEXT,
    PRIMARY KEY (account_id, id)
) WITHOUT ROWID;

CREATE INDEX idx_mailboxes_account_id ON mailboxes(account_id);

CREATE TABLE emails(
    account_id INTEGER NOT NULL REFERENCES accounts(id) ON DELETE CASCADE,
    id TEXT NOT NULL,
    jmap_data TEXT NOT NULL,
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
) WITHOUT ROWID;

CREATE INDEX idx_emails_account_thread ON emails(account_id, thread_id);

CREATE TABLE blobs(
    account_id INTEGER NOT NULL,
    id TEXT NOT NULL, -- The ID on the server
    mime_type TEXT,
    name TEXT,
    data BLOB NOT NULL,
    last_accessed DATETIME NOT NULL,
    FOREIGN KEY (account_id) REFERENCES accounts(id) ON DELETE CASCADE,
    PRIMARY KEY (account_id, id)
) WITHOUT ROWID;

CREATE INDEX idx_blobs_account_id ON blobs(account_id);
CREATE INDEX idx_blobs_account_id_last_accessed ON blobs(account_id, last_accessed);

CREATE TABLE mailbox_emails (
    account_id INTEGER NOT NULL,
    mailbox_id TEXT NOT NULL,
    email_id TEXT NOT NULL,
    thread_id TEXT NOT NULL,
    received_at TEXT NOT NULL,
    PRIMARY KEY (account_id, mailbox_id, email_id),
    FOREIGN KEY (account_id, mailbox_id) REFERENCES mailboxes(account_id, id) ON DELETE CASCADE,
    FOREIGN KEY (account_id, email_id) REFERENCES emails(account_id, id) ON DELETE CASCADE
) WITHOUT ROWID;
CREATE INDEX idx_mailbox_emails_account_mailbox ON mailbox_emails(account_id, mailbox_id);
CREATE INDEX idx_mailbox_emails_account_email ON mailbox_emails(account_id, email_id);
CREATE INDEX idx_mailbox_emails_account_mail_time ON mailbox_emails(account_id, mailbox_id, received_at);

-- Trigger to update mailbox_emails table on email insert/update/delete
CREATE TRIGGER trg_update_mailbox_emails_after_email_insert
AFTER INSERT ON emails
BEGIN
    INSERT INTO mailbox_emails (account_id, mailbox_id, email_id, thread_id, received_at)
        SELECT NEW.account_id, mb.key, NEW.id, NEW.thread_id, NEW.received_at
        FROM json_each(NEW.jmap_data->'$.mailboxIds') AS mb
        WHERE mb.value == true;
END;

CREATE TRIGGER trg_update_mailbox_emails_after_email_changed
AFTER UPDATE ON emails WHEN
    (OLD.id == NEW.id AND OLD.account_id == NEW.account_id) AND
    (OLD.jmap_data->'$.mailboxIds' != NEW.jmap_data->'$.mailboxIds') OR
                            (OLD.thread_id != NEW.thread_id)

BEGIN
    INSERT OR REPLACE INTO mailbox_emails (account_id, mailbox_id, email_id, thread_id, received_at)
        SELECT NEW.account_id, mb.key, NEW.id, NEW.thread_id, NEW.received_at
        FROM json_each(NEW.jmap_data->'$.mailboxIds') AS mb
        WHERE mb.value == true;

    DELETE FROM mailbox_emails
    WHERE account_id = OLD.account_id AND email_id = OLD.id AND mailbox_id NOT IN
    (SELECT mb.key
     FROM json_each(NEW.jmap_data->'$.mailboxIds') AS mb
     WHERE mb.value == true);
END;


-- Trigger to update threads table on email insert/update/delete
-- CREATE TRIGGER trg_update_threads_after_email_insert
-- AFTER INSERT ON emails
-- BEGIN
--     INSERT INTO threads (account_id, id, mailbox_id, last_updated_at)
--         SELECT NEW.account_id, NEW.thread_id, mb.key, NEW.received_at
--         FROM json_each(NEW.jmap_data->'$.mailboxIds') AS mb
--         WHERE mb.value == true
--         ON CONFLICT(account_id, id) DO UPDATE SET last_updated_at = NEW.received_at WHERE last_updated_at < NEW.received_at;
--     END;
-- END;
--
-- CREATE TRIGGER trg_update_threads_after_email_thread_id_changed
-- AFTER UPDATE ON emails WHEN (OLD.thread_id IS NOT NULL AND OLD.thread_id != NEW.thread_id) OR
--                             (OLD.received_at IS NOT NULL AND OLD.received_at != NEW.received_at)
-- BEGIN
--     SELECT RAISE ( ABORT, 'Changing thread_id/received_at is not supported' );
-- END;
--
-- CREATE TRIGGER trg_update_threads_after_thread_deleted_from_email
-- AFTER DELETE ON emails WHEN NOT EXISTS (
--     SELECT 1 FROM emails WHERE account_id = OLD.account_id AND thread_id = OLD.thread_id)
-- BEGIN
--     DELETE FROM threads WHERE account_id = OLD.account_id AND id = OLD.thread_id;
-- END;
--
-- CREATE TRIGGER trg_recalculate_updated_after_message_deleted
-- AFTER DELETE ON emails WHEN NOT EXISTS (
--     SELECT 1 FROM threads WHERE account_id = OLD.account_id AND id = OLD.thread_id AND last_updated_at > OLD.received_at
-- )
-- BEGIN
--     UPDATE threads
--     SET last_updated_at = (SELECT MAX(received_at)
--                            FROM emails
--                            WHERE account_id = OLD.account_id AND thread_id = OLD.thread_id)
--     WHERE threads.account_id = OLD.account_id AND threads.id = OLD.thread_id;
-- END;
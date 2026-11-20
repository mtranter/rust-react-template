-- The assistant's Conversations (Spec 0012), each owning its messages and their tool calls:
-- children of their conversation, saved with it under its version, gone with it. Runs with
-- search_path set to "assistant".
CREATE SCHEMA IF NOT EXISTS assistant;

CREATE TABLE assistant.conversation (
    id         uuid        PRIMARY KEY,
    -- The subject of the token that started it: no foreign key, as no module's table holds
    -- every subject.
    owner_id   text        NOT NULL,
    -- Empty until its first message.
    title      text        NOT NULL,
    created_at timestamptz NOT NULL,
    version    bigint      NOT NULL
);

CREATE INDEX conversation_owner_idx ON assistant.conversation (owner_id, created_at DESC);

CREATE TABLE assistant.message (
    conversation_id uuid        NOT NULL REFERENCES assistant.conversation (id) ON DELETE CASCADE,
    id              uuid        NOT NULL,
    -- The order they were posted in.
    position        integer     NOT NULL,
    posted_at       timestamptz NOT NULL,
    kind            text        NOT NULL CHECK (kind IN ('user', 'assistant', 'tool_calls')),
    -- The words of a user's or the assistant's message; a message of tool calls has none.
    text            text        CHECK ((kind = 'tool_calls') = (text IS NULL)),
    PRIMARY KEY (conversation_id, id),
    UNIQUE (conversation_id, position)
);

CREATE TABLE assistant.tool_call (
    conversation_id uuid    NOT NULL,
    message_id      uuid    NOT NULL,
    id              uuid    NOT NULL,
    -- The order the model asked for them in, across the conversation.
    position        integer NOT NULL,
    -- The operation's id, which is its tool's name.
    tool            text    NOT NULL,
    arguments       jsonb   NOT NULL,
    status          text    NOT NULL
        CHECK (status IN ('ran', 'awaiting_approval', 'approved', 'rejected')),
    -- The operation's answer once it has one: its body, or its error body.
    outcome         text    CHECK (outcome IN ('ok', 'error')),
    result          jsonb   CHECK ((outcome IS NULL) = (result IS NULL)),
    PRIMARY KEY (conversation_id, id),
    UNIQUE (conversation_id, position),
    FOREIGN KEY (conversation_id, message_id)
        REFERENCES assistant.message (conversation_id, id) ON DELETE CASCADE
);

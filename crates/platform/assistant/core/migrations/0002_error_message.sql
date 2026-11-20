-- A turn that ends in failure leaves a message saying why (Spec 0012): shown to its owner, never
-- sent to the model. Runs with search_path set to "assistant".
ALTER TABLE assistant.message DROP CONSTRAINT message_kind_check;
ALTER TABLE assistant.message
    ADD CONSTRAINT message_kind_check CHECK (kind IN ('user', 'assistant', 'tool_calls', 'error'));

-- Publishes every user property but the audit ones (spec D8): consumers, not
-- users, decide what they need. Appending a column is additive.
CREATE OR REPLACE VIEW users_api.user_v1 AS
SELECT id, display_name, email
FROM users.account;

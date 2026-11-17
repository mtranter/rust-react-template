-- Published view (spec D8): the read contract other modules' SQL may use, so
-- `users.account` stays private and free to change. Evolve it only additively
-- (`CREATE OR REPLACE VIEW` may append columns); a breaking change ships as
-- `user_v2` alongside this one until every reader has moved.
CREATE SCHEMA IF NOT EXISTS users_api;

CREATE VIEW users_api.user_v1 AS
SELECT id, display_name
FROM users.account;

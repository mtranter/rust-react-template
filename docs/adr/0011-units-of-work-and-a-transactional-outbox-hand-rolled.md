# Writes run in a Unit of Work, and Events go through a hand-rolled transactional outbox

A use case that writes does so in one Unit of Work: an opaque `app_persistence::Tx`, begun from the Service's `Arc<dyn Transactions>` and committed at the end. Everything written through it commits together or not at all; dropping it uncommitted (as `?` does) rolls it back. Above `db/` it is opaque: repositories reach the connection with `app_db::conn(tx)`, so the Application Layer still never names the driver. The Service publishes its Events through its own port beside the Repository, `Arc<dyn EventPublisher>`, which takes the `Tx` as a repository write does; `app-db`'s `PgEventPublisher` writes each to the shared `messaging.outbox` at once, in that transaction. The `worker` bin relays each Event to its Subscribers' queues and delivers it (ADR 0013).

Atomicity of "save, then do something" is the database's to give, and only a shared transaction can give it. So anything that must follow a write goes into its Unit of Work, as an Event, and happens if and only if the write commits; nothing calls an external system or another Module after `commit`. A Module never writes another Module's data: it publishes, and the owner reacts in a Subscriber.

Publishing is a port beside the Repository, not a method on the `Tx`, so the Service publishes deliberately, and the `Tx` stays a plain transaction that knows nothing of Events. The outbox is one shared `messaging` schema rather than one per Module: one relay query, one place for Dead Letters, and no Module writing another's schema when an Event fans out into a Subscriber's queue. It belongs to no Module, so `app-db` owns its migrations and applies them before any Module's; Events are namespaced by Topic.

## Considered options

- **`sqlxmq` 0.6.** On sqlx 0.8: a second sqlx and a second pool.
- **`pgmq`.** Needs a Postgres extension the stock image lacks.
- **`apalis-postgres` 1.0.** On sqlx 0.9 and the one to revisit, but a job runner, not an outbox: fan-out to Subscribers, enqueueing in the use case's own transaction and the injected clock would still be ours, around its schema and worker model. What's left to hand-roll is a few tables and statements (relay, claim with `FOR UPDATE SKIP LOCKED`, complete or fail), tested here.
- **A `Tx`-taking Module API trait, so Modules share a transaction.** Couples their transactions, and can't survive lifting one out.
- **Passing `PgPool` into Services.** Defeats the clippy tripwire (ADR 0017).
- **A closure-scoped `transaction(|tx| ..)`.** Async closures over `dyn` Services don't compose, and a dropped `Tx` already rolls back.

## Consequences

- Cross-module consistency is eventual: the author check in `notes`' create and a deletion in `users` are separate transactions.
- A Subscriber runs in its own `Tx`, which also deletes its delivery, so a handler that only writes to the database takes effect exactly once; anything external is at least once.
- A handler holds a transaction open while it runs, so a slow external call holds a connection.
- The outbox keeps dispatched rows until someone prunes them; nothing does yet.
- Lifting a Module out (ADR 0001) means relaying its Events to a broker instead, which is what an outbox is for.

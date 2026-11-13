# Subscriptions are rows, registered by `migrate`

A Subscriber is an `app_persistence::Handle<E>` for one Event, or `Handle<XTopic>` for a whole Topic (matching its event with no wildcard arm), registered in its Module's `handlers(..)` under a stable queue name. Each Subscription is its own queue, so one failing Subscriber neither blocks nor retries another. A queue is of one Topic and may list several of its Event types.

Subscriptions live in the database, in `messaging.subscription`, and the relay fans out by joining it, so every build relays the same way whatever handlers it was built with. They are registered when migrating, never when a Worker starts: the `migrate` bin, after the migrations, calls `app_db::register_subscriptions` with every Module's `Handlers`. Migrations run before a rollout (ADR 0021), so a new build's Subscriptions exist before any of its processes publishes or relays, and no Event goes out without its new Subscribers.

A Worker claims only what it can decode: each job records its Event type, and a claim takes only the `(queue, event type)` pairs its own `Handlers` list. During a rolling deploy, a job for a Subscription only the new build has waits for a new Worker rather than an old one failing it and spending an attempt. A Worker never writes the table: it refuses to start if one of its own pairs is missing or registered under another Topic, warns of each live Subscription it doesn't serve, and still runs the pending jobs of a retired one it serves.

## Considered options

- **Registering when a Worker starts.** Old and new builds would relay differently during a rolling deploy, and Events published before the first new Worker started would miss its Subscriptions.

## Consequences

- A Subscription receives every Event relayed after its registration commits, and no history: one added to a running system starts empty.
- A queue is bound to one Topic forever: registering it under another fails `migrate`, so moving Topic means a new queue name.
- A retired queue's name is never reused: registering one fails `migrate`, so a rollback or an old `migrate` can't revive it.
- Retiring a Subscription is a migration, never a side effect of deploying code without its handler. It stops relaying to the queue and dead-letters what it hadn't run, where it stays visible:

  ```sql
  UPDATE messaging.subscription SET retired_at = now() WHERE queue = 'notes.archive_deleted_authors_notes';
  UPDATE messaging.job SET dead_at = now(), last_error = 'subscription retired'
  WHERE queue = 'notes.archive_deleted_authors_notes' AND dead_at IS NULL;
  ```
- The release that brought the claim filter wasn't protected by it: the build it replaced claimed any queue.

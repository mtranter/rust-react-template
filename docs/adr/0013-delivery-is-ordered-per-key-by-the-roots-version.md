# Delivery is at least once, ordered per key by the root's version

The model is Kafka's: a Topic per root, the root's id as the message key, a Subscription as a consumer group, and a `messaging.job` row per message still to deliver in place of an offset. Order is per key, by the root's `Version`, which is finer than Kafka's per partition, so a failing Event holds up only its own key.

It works because commit order is version order per key. Every write to an Aggregate, a child's included (ADR 0030), is a version-guarded `UPDATE` or `DELETE` of its root's row (ADR 0016), so v4's transaction waits on the row lock until v3's commits, and the outbox row is written in the same transaction. Every Event carries its root's version: on the outbox row (`topic`, `key`, `version`, unique together, so a second Event for one save fails) and in the envelope as `entityversion`. A save publishes the version `save` returned; a deletion publishes the version it removed plus one (`Version::next()`), so it is strictly the newest of its key rather than tying with the last update. Two mechanisms carry that order to the Subscriber:

- **One relay at a time.** The relay takes `pg_try_advisory_xact_lock`, reads a batch in outbox order, and in one transaction marks it dispatched and fans it out into one job per Subscription, each carrying its key and version. So a key's v3 job exists, ran or was filtered out before its v4 job does. Workers still claim jobs in parallel, with `FOR UPDATE SKIP LOCKED`.
- **A claim waits for earlier versions of the same key in the same queue** that aren't dead. A job's row lasts until the transaction handling it commits its deletion, so while v3 runs or backs off, v4 can't be claimed.

A failing handler rolls back, backs off (10 s, doubling, at most an hour) and is retried; after 5 attempts it is a Dead Letter, kept in `messaging.job` with its last error until someone reruns it. A claim is a five-minute lease, so a delivery whose Worker died runs again. The claim counts as an attempt, so one whose Worker dies (or whose handler outruns the lease) on every attempt is dead-lettered at the claim after its last, without running again (ticket 0016). A run that outlives its lease while another completes the same delivery commits nothing: the job's removal commits with the handler's writes, and a run that finds the job already gone rolls them back (ticket 0018).

## Considered options

- **Partitions.** A failing Event would hold up its whole partition.
- **Offsets.** They need the outbox in commit order, which its sequence isn't; per-key order doesn't.
- **CloudEvents' `sequence` for the version.** A string that must sort lexicographically, with no stated scope. `entityversion` is an integer scoped to one `topic` and `partitionkey`.

## Consequences

- No order across keys, Topics or Subscriptions: `UserDeleted` and a racing `NoteCreated` by that user stay unordered.
- Gaps are normal: a filtered Subscription, or one registered after some of a key's Events, sees only some versions.
- A failing job holds up only its own key in its own queue, about 2.5 minutes over 5 attempts, then the key moves on.
- Rerunning a Dead Letter after later versions ran delivers it out of order, and a lease that expires mid-run can run a job beside its successor. So every handler is idempotent, and a Subscriber keeping a copy of another Module's state stores the version and applies each Event in one `INSERT … ON CONFLICT (id) DO UPDATE … WHERE copy.version < EXCLUDED.version`, with a tombstone for a deletion. Reactions to facts (mail, archiving) don't compare versions.
- The Worker only polls (`APP_WORKER_POLL_MS`, default 1000). It runs as its own deployment from the same image and config as the Server, scales by replicas sharing the queue, and stops on SIGTERM after the job in hand. Its time is the injected Clock (ADR 0020), so tests control backoff.

# Instances broadcast over a shared bus, required only when a module broadcasts

Behind a load balancer, a client holding an SSE stream or a WebSocket is pinned to one server instance, while what it waits to hear of may happen on another instance or in the worker. The Outbox can't tell it: its queues are competing consumers, each job handled by one worker, and a stream needs every instance to hear every message. So the platform has a broadcast, the common crate `app-bus`, brought in from an app built on this template that needed it first.

- **`Bus`** is the port: `publish(channel, bytes)` and `subscribe(channel)`, reporting `Listening`, `Heard(bytes)` and `Lost`. Its adapters are `RedisBus` and `NoBus`, which carries nothing.
- **`Channel<T>`** is one named channel carrying `T` as JSON. Instances on different releases share it during a deploy, so `T` changes only compatibly, as an Event does, and a name is never reused for another `T`. A module prefixes the names it owns. A message an instance can't read as `T` is heard as `Hearing::Unreadable`, never skipped silently.
- **`Hub<T>`** subscribes to a channel once per instance, when something first follows it, and fans each message out to every local follower: the streams the instance holds. A follower that begins by fetching the truth sets `with_listening_within(ZERO)`, so it never waits on an unreachable bus.
- **The bus reports gaps; each consumer decides what one means.** A follower receives `Received::Message(T)` or `Received::Gap`: the subscription reconnected, a message couldn't be read, the follower fell behind the hub's backlog (1024), or it began following before the subscription listened. The bus is best effort, with no durability, order or delivery promised: a consumer that must not lose anything keeps the truth elsewhere, usually the database, and treats a gap as the cue to recheck it.

**`RedisBus` is Redis/Valkey pub/sub**: AWS ElastiCache Serverless for Valkey in production, docker compose's `redis` (a Valkey image) in development, a Valkey testcontainer per test (`app_bus::testing::Valkey`). It uses cluster-wide `PUBLISH`/`SUBSCRIBE`, never sharded `SPUBLISH`, so a subscriber on any node hears everything. Valkey Serverless requires TLS (`rediss://`); credentials come in the URL, and the certificate is checked against the public roots or a private CA given as `APP_REDIS_CA`, with no way to skip the check. It connects lazily, so a server that can't reach Redis starts anyway; a subscription is a dedicated connection that reconnects and pings every 15 s, so a connection cut without a word is noticed. Publishing shares one connection.

**The bus is required only when a module broadcasts.** A module that publishes or follows a channel says so in its registration (`Module::broadcasts()`), and the server and worker then refuse to start without `APP_REDIS_URL`, naming it. Set, it is used whether or not anything needs it; unset in an app whose modules don't broadcast, the `Context` carries `NoBus`. The `Context` carries one `Arc<dyn Bus>` (`with_bus`, `bus()`), so every module shares one publishing connection.

## Considered options

- **Always required**, as the app it came from does. Uniform, but every app built from the template would run and pay for Valkey (about $6 a month on ElastiCache Serverless at its minimum) whether or not anything broadcasts.
- **Optional, falling back to `NoBus` when unset.** A module that broadcasts would then work on one instance and quietly not across several: locally fine, broken behind a load balancer, and saying so nowhere. Requiring the URL exactly when a module needs it rules that out.
- **Postgres `LISTEN`/`NOTIFY`.** No second dependency, but it ties the broadcast to a database that offers it (not Aurora DSQL, for one), and a lost listener hears nothing, with no fallback.
- **Kafka, Kinesis or SQS.** Durable, ordered delivery is exactly what the bus doesn't need, since the database holds the truth. SNS with a queue per instance would make an autoscaled fleet own a queue's lifecycle per instance.
- **The Outbox's queues.** Competing consumers: each job runs on one worker, and a stream is pinned to one instance.

## Consequences

- A new broadcast is a `Channel`, and a `Hub` where the instance holds clients, built from `ctx.bus()`, with its module registered as `broadcasts()`: no new adapter, connection or configuration.
- `redis` may be depended on only by `app-bus`, enforced by `deny.toml`. No other crate reaches Redis directly, its tests included.
- `app-bus` proves itself end to end over a real Valkey (`crates/common/bus/tests/`), which needs Docker wherever its tests run; its hub's own rules (a lag, a follower that stopped waiting) are tested inline over a bus the test drives by hand.
- `make dev` starts docker compose's Valkey whether or not a module broadcasts, so `APP_REDIS_URL`, set in `.env.example`, always reaches one locally.

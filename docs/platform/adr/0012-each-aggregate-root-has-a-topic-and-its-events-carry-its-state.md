# Each Aggregate Root has a Topic, and its Events carry the root's whole state

Every state change publishes an Event, in the Unit of Work that saves it, one per root changed. An Event is a fact in the past tense (`UserRegistered`), never a command in disguise (`SendWelcomeEmail`), and its publisher doesn't know who listens.

**The Domain decides what happened.** Each Aggregate has a plain `#[must_use]` enum of what can happen to it (`users::domain::UserEvent`), knowing nothing of `app_events`, and every transition returns one. `application/events.rs` maps each variant, exhaustively, onto its Topic's event enum in the Module API. That match is the one place Domain names meet the published contract, so the Domain can rename or split its events without touching `events::v1`: the path from Domain to wire has two exhaustive matches, one on each side.

**Each Aggregate Root has a Topic**, as Kafka has topics: one stream of its Events, keyed by the root's id, declared in the Module API with `app_events::topic!`. It generates the Topic's event enum, a marker type per Event, and its encoding and decoding, and fails to compile if two Events share a type. The enum is deliberately not `#[non_exhaustive]`: a new Event breaks every whole-topic consumer's build in the same change, rather than falling into a wildcard arm.

**An Event carries an Event Payload** `{ entity, context }`: event-carried state transfer, so a Subscriber never calls back. `entity` is the root's whole state as it now is, every property but the audit ones (created/updated at/by), or as it was last for a deletion. It is the Topic's own state type, kept apart from the Published Record: today they hold the same fields, but a view column and a Topic version change on their own schedules. `context` says what the state can't (which child was removed, ADR 0030); none where the entity says it all. More fields are more coupling, accepted so that consumers, not the publisher, decide what they need.

**Versioning is the Topic's.** Event types are `<module>.<event>`, unique within the Topic. Contexts and the entity only gain fields, each `#[serde(default)]`, and a new Event is additive. Removing one, or changing the entity incompatibly, is a new Topic (`users.user.v2`) published beside the old until its consumers have moved.

**Every Event travels in a CloudEvents 1.0 Event Envelope**, in its JSON format, with extension attributes `topic`, `partitionkey` (the key, from CloudEvents' partitioning extension) and `entityversion` (ADR 0013). The publisher stamps the id and time; Subscribers receive the id to deduplicate on. `time` is when the publishing transaction recorded it, by that instance's clock: it doesn't order Events.

## Considered options

- **Opaque JSON Events with a schema registry.** No compiler between publisher and Subscriber. A typed Event in `-api` is the same kind of small, versioned, compiled contract as a trait or a Published Record.
- **Thin Events carrying ids, Subscribers calling back.** Couples Subscribers to the publisher's Module API and to its state at delivery time rather than at the change.
- **A serde-tagged enum for decoding.** `type` and `context` sit at different depths of the envelope, so decoding is two steps: the envelope with its context as raw JSON, then `XEvent::decode(type, context)`.

## Consequences

- A Topic's entity grows with the root; Subscribers ignore fields they don't use.
- Each publisher's tests assert every state-changing behaviour's Event and version, and that a refused one announced nothing.

# No jobs or scheduled work

A Module that has to do something does it in reaction to an Event, its own or another Module's. A Module may subscribe to its own Events: `users` welcomes a registered user by reacting to its `UserRegistered`, rather than the registration ordering the email.

A job queue for a Module's own deferred or scheduled work was built and removed. Its only use, a welcome email, was a command dressed as an Event (`UserRegistered` replaced it), and nothing needed scheduling.

## Consequences

- Scheduled and recurring work ("do X at time T") waits for a Module that needs it. Rows in `messaging.job` with a future `run_at` already wait their turn, so it would be a `Tx` method and a `Handlers` registration on the existing queue.

# Learnings: 0042, a Spec for a human on hosting environments on AWS Lambda

- Spec 0010 follows Specs 0006–0009: `status: needs-info` only, the body assumes each recommendation with "(question N)" markers, and it ends in `## Needs Human Input (factory)`. Eight questions plus a cost table only fit in about 150 lines with one line per option.
- **A sixth stateful part the Ticket didn't list:** `main.rs` spawns `rules.watch(access::POLL_EVERY)`, which re-polls grants every 5 s. A frozen Lambda runs no spawned task, so anything Lambda-hosted must refresh on invocation instead. Check `crates/server/src/main.rs` for other `tokio::spawn` loops before assuming a list of stateful parts is complete.
- `worker --drain` (Spec 0004) wasn't built as of 2026-11-19, and live topics (ticket 0024) are `wontfix`, so nothing streams today.
- Prices checked 2026-11-19 (us-east-1), which may since have changed:
  - Aurora Serverless v2 pauses at 0 ACU and resumes in about 15 s, but **an attached RDS Proxy, or any open connection, stops it pausing**.
  - Neon cut its prices: Launch is $0.106 per compute-hour, Scale $0.222, with no monthly minimum.
  - A NAT gateway costs $32.85 a month, a public IPv4 address $3.65.
  - Lambda bills start-up time since August 2025.
  - AWS's official price-list JSON files (`pricing.us-east-1.amazonaws.com/offers/v1.0/aws/<Service>/current/us-east-1/index.json`) were the most reliable source; the marketing pages are rendered by JavaScript.
- A periodic worker tick on Lambda would keep a scale-to-zero database awake; wake on commit and at the next due time instead.
- Return the structured output only once the work is committed: a background research agent finishing after it means redoing the wrap-up.

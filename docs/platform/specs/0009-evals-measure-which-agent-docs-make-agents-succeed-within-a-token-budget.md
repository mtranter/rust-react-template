---
status: needs-info
---

# Evals measure which agent docs make agents succeed within a token budget

## Problem

Two plans for the agent docs compete: condense the ADRs and rules into one strong `CLAUDE.md` with first-class reference code to copy, or keep `CLAUDE.md` short, turn every rule that can be checked into a check, and load task skills on demand. Nothing measures either:

- The docs are large: `crates/AGENTS.md` is 45 KB (about 11K tokens) and `docs/platform/adr/` 117 KB over 41 files (about 29K). Nobody knows which parts agents read, or which help.
- The same slips recur. The `check-init` word ban appears in five Learnings (`.factory/learnings/0026-…`, `0028-…`, `0032-…`, `0033-…`, `0038-…`) and `scripts/init.sh`'s copy of the router in two (`0031-…`, `0033-…`), though `crates/AGENTS.md` and the Learnings already said so.
- The factory (`../trizzoop`) records no tokens or dollars: its Implementor passes only `model` and `maxTurns` to the Agent SDK (`../trizzoop/src/implementor.ts`).
- An eval runs an agent through whole tasks, many times, per variant, so it costs real money. Cost has to be designed in.

## Solution overview

- **A variant** is a git ref of this repo branched from a fixed **eval base** commit, changing only what agents read or run: `AGENTS.md`, `CLAUDE.md`, the area `AGENTS.md`s, `docs/`, `.claude/skills/`, `crates/reference/`, and checks in `scripts/` and the `Makefile`.
- **A case** is a task on the eval base: a Ticket's text, and graders the agent never sees. A **suite** is every case, run a fixed number of times on one variant (question 4).
- **The harness is `factory eval <variant ref>`** in the factory, reusing its Implementor prompt, worktrees and check runner; cases live in this repo under `evals/` (question 8).
- **Every run has a dollar cap, every suite a cap, and spend is reported per run, per suite and per comparison** (questions 1, 2). A run that hits its cap fails.
- **Graders are deterministic**: the variant's `make check-changed` and `make check-init`, and the case's hidden tests (question 6).

## Behaviour

### Cases

- `evals/cases/<nnnn>-<slug>/` holds `ticket.md` (what the Implementor is given), `hidden/` (test files copied in only after the run) and `case.toml`: its source, a size, and an optional extra grader command.
- Cases come from three sources (question 4): **reverted features**, a recent merged Ticket's PR reverted on the eval base, its tests hidden, its Ticket the task; **synthetic tasks** on the reference modules, such as a new module through `make new-module` (`scripts/new-module.sh`) taken through to a screen; and **Learnings**, a slip turned into a task whose grader catches it (question 7).
- The eval base moves only when a human re-pins the cases to a newer commit; results from different bases are never compared.

### A run

- A fresh worktree of the variant, with the case's `ticket.md`, given to the factory's Implementor exactly as the factory would give it, on one model (question 3), with no reviewers (question 6).
- The SDK's `maxBudgetUsd` caps the run (question 1): its `error_max_budget_usd` result is a fail marked "over budget" (`../trizzoop/node_modules/@anthropic-ai/claude-agent-sdk/sdk.d.ts`). `maxTurns` stays as the factory sets it.
- The main conversation's cache TTL is 1 hour (`CLAUDE_CODE_PROMPT_CACHE_TTL=1h`), since `make check-changed` can outlast the 5-minute default an API key gets ([cost tracking](https://code.claude.com/docs/en/agent-sdk/cost-tracking), checked 2026-11-19).
- Each run gets its own Postgres (its own compose project), since worktrees fight over port 5432 (`.factory/learnings/0028-…`, `0033-…`); two runs at a time.
- After the agent stops, the hidden tests are copied in and the graders run. A run **passes** only if all pass.
- Recorded per run: pass and each grader's result, turns, input, cache-read, cache-write and output tokens, estimated dollars (`total_cost_usd` and `modelUsage`, an estimate, not a bill), wall-clock, and which agent-docs files the agent read, from its tool calls.

### Suites and comparisons

- `factory eval` runs every case N times (question 4) and stops starting runs once the suite's cap is spent; unrun cases are reported as such, not as fails.
- Results go to `evals/results/<date>-<variant>-<model>.json`, committed, so a later comparison reuses a stored baseline instead of re-running it (question 5).
- `factory eval --compare <a> <b>` prints, per case and overall: pass rate, dollars per run, dollars per **passed** run, tokens per run, and docs read. It calls a difference real only when every case either agrees or favours one side, and otherwise says "no detectable difference".
- `make init` deletes `evals/`, as the template-only part it is (`scripts/init.sh`, `scripts/check-init.sh`).

### Cost model

Prices for Claude Opus 5.5 per million tokens: $4 input, $8 a 1-hour cache write, $0.20 a cache read, $20 output; Sonnet 5.5 half of each ([pricing](https://platform.claude.com/docs/en/about-claude/pricing), checked 2026-11-19). The Batch API's 50% discount doesn't apply: an agent's turns can't wait for a batch.

Measured on the factory's own Opus 5.5 sessions for Tickets 0026–0041 (its Claude Code transcripts, read on 2026-11-19): an Implementor on a code Ticket took 66–130 turns and $4–11, on a docs Ticket $0.4–1.7; 95–99% of its input came from the cache. A reviewer cost a median $0.34 (40 sessions), a review round about $0.90 with its coordinator.

Assumed **one run**: an S-sized code task, 60 turns at a mean context of 120K tokens (7.2M input tokens), 98% of them cache reads and 2% cache writes, 60K output tokens, reviewers off:

| | Tokens | Opus 5.5 | Sonnet 5.5 |
|---|---|---|---|
| Cache reads | 7.06M | $1.41 | $0.71 |
| Cache writes (1 h) | 0.14M | $1.15 | $0.58 |
| Output | 0.06M | $1.20 | $0.60 |
| **One run** | 7.26M | **$3.76** | **$1.88** |
| One suite: 8 cases × 3 (question 4) | 174M | $90 | $45 |
| One comparison, baseline stored | 174M | $90 | $45 |
| One comparison, both run | 348M | $180 | $90 |

- **Cache hits matter most**: at 90% instead of 98%, the writes alone cost $5.76 and a run about $8.30.
- **Reviewers on** add about $0.90 a round, and a revision of $1–1.50 after a failed one: $1–3 a run, $24–72 a suite (question 6).
- **The docs' own size is cheap; turns are not.** 10K tokens read at turn 5 of 60 cost about $0.19 for the rest of the run. Docs change the bill mainly through how many turns, and retries, an agent needs.

## Out of scope

- Changing the agent docs themselves: each plan is a variant, made by whoever proposes it.
- Gating merges on an eval, and evals of the factory's own prompts or of the Delegator, TicketCreator or Compounder.
- Evals in an app made by `make init`.

## Open risks

- **Small samples.** 24 runs per variant detect only large differences, about 25 points of pass rate; smaller ones read as noise, and the report says so.
- **Leaks.** A reverted feature may still be described in the docs, ADRs or Learnings of the eval base, which makes it easier than a new one. Writing a case checks for this.
- **Fairness.** A variant may weaken a check the graders run; the hidden tests are the part it can't touch, and a human reviews each variant's diff.
- **Dollars are estimates.** `total_cost_usd` comes from a price table in the SDK ([cost tracking](https://code.claude.com/docs/en/agent-sdk/cost-tracking), checked 2026-11-19); the Console's bill is the truth (question 2).

## Needs Human Input (factory)

1. **What is the monthly budget, and the cap per run and per suite?**
   - *Why it matters:* it decides how many variants can be measured a month, and when a runaway run is stopped.
   - *Options:*
     - **$100 a month**, $10 a run: one Opus suite a month, so one new variant against a stored baseline. Slow to decide anything.
     - **$300 a month, $10 a run, $150 a suite:** three Opus suites, so both plans and one follow-up in a month.
     - **$1,000 a month:** about eleven suites, or more cases and repeats for smaller differences. Three times the money for finer answers.
   - *Recommendation:* $300, $10 and $150. $10 is nearly three times an expected run, so it stops only runaways.
   - *Depends on it:* "A run" (`maxBudgetUsd`), "Suites and comparisons" (the suite cap), the cost model.

2. **Who pays: the Claude subscription the factory runs on, or an API key?**
   - *Why it matters:* the factory signs in with a subscription token, not an API key (`../trizzoop/docs/research/agent-sdk-spike.md`).
   - *Options:*
     - **The subscription.** No extra bill, but evals use up the same plan limits as the factory, so a suite can stall tickets; spend is only the SDK's estimate.
     - **An API key in its own Console workspace**, with a monthly spend limit set to question 1's budget ([workspaces](https://platform.claude.com/docs/en/manage-claude/workspaces), checked 2026-11-19). A real bill, enforced by Anthropic, apart from the factory; about $3.76 an Opus run.
   - *Recommendation:* the API key: the budget is enforced outside the harness, and the bill is exact.
   - *Depends on it:* "A run" (the cache TTL setting), the cost model, the estimates risk.

3. **Which model, or models, do evals run on?**
   - *Why it matters:* docs that help one model may not help another, and the price differs by about 2 times between Opus and Sonnet.
   - *Options:*
     - **Opus 5.5 only**, the factory's model (its transcripts), at its default effort. $90 a suite.
     - **Sonnet 5.5 only.** $45 a suite; measures a model the factory doesn't run.
     - **Sonnet to screen, Opus to confirm** the winner. $45 per screened variant, plus $90 for the finalist; two results to reconcile when they disagree.
   - *Recommendation:* Opus 5.5 only: the question is what helps the factory, which runs Opus.
   - *Depends on it:* "A run", the cost model, the results' file name.

4. **How many cases, how many repeats, and which?**
   - *Why it matters:* the same agent passes a task some times and not others, so each case runs more than once; cost grows with cases times repeats.
   - *Options:*
     - **5 cases × 3:** $56 a suite; too few to cover both backend and frontend.
     - **8 cases × 3:** $90; three reverted features, three synthetic tasks (a new module to a screen, a field across backend and frontend, an operation change needing `make generate`), two from Learnings.
     - **15 cases × 5:** $282, a month's budget for one suite; detects differences of about 15 points.
   - *Recommendation:* 8 × 3, as listed, all S-sized: enough to see a large effect, and both plans fit one month.
   - *Depends on it:* "Cases", "Suites and comparisons", the cost model, the small samples risk.

5. **How often do evals run?**
   - *Why it matters:* a suite costs about $90, so frequency sets most of the bill.
   - *Options:*
     - **On demand**, a human running `factory eval` against a variant. Spend only when a decision needs it.
     - **On every change to the agent docs**, such as each Compounder PR, about every five Tickets. Catches a docs change that hurts; at the factory's pace, several suites a week.
     - **Nightly.** About $2,700 a month; most nights nothing changed.
   - *Recommendation:* on demand, against a stored baseline. Per-change runs can follow once the suite is trusted.
   - *Depends on it:* "Suites and comparisons" (stored results).

6. **Do the graders include the LLM reviewers, or only checks and hidden tests?**
   - *Why it matters:* the factory merges only after the `standards` and `spec` reviewers pass, so a run that passes the checks may not pass review. Reviewers judge, and vary between runs.
   - *Options:*
     - **Checks and hidden tests only** (`make check-changed`, `make check-init`, the case's tests). No tokens; exact and repeatable; misses conventions no check covers.
     - **Plus the reviewers, as in the factory**, with revision rounds. $1–3 a run, $24–72 a suite; nearer the factory, noisier.
     - **Reviewers as a score only**, on a sample of runs. Some insight, about $0.90 a reviewed run.
   - *Recommendation:* checks and hidden tests only. A convention worth measuring can become a check, as one of the two plans proposes; reviewers can be compared later with a `--review` flag.
   - *Depends on it:* Solution overview's last bullet, "A run", the cost model.

7. **How do factory Learnings become eval cases?**
   - *Why it matters:* each Learning is a real slip, but the Compounder folds them into the docs, so a case is the only way to keep checking the slip stays fixed.
   - *Options:*
     - **A human writes each case by hand.** Best cases, slowest; few will get written.
     - **A human picks the Learning; an agent drafts the case** (task and hidden grader) as a normal Ticket, and the human approves it. One small Ticket per case.
     - **Every Learning becomes a case automatically.** The most coverage, but many Learnings are notes about the environment (a busy port, zsh quoting), not slips an agent could avoid.
   - *Recommendation:* a human picks, an agent drafts. A case names its Learning in `case.toml`'s source.
   - *Depends on it:* "Cases" (the third source).

8. **Where does the harness live?**
   - *Why it matters:* the eval must give the agent the same prompt as the factory, or it measures something else.
   - *Options:*
     - **In the factory, as `factory eval`**, with cases in this repo. Reuses the Implementor prompt and worktrees; the factory gains a command.
     - **In this repo, as a script driving Claude Code headless.** No factory change, but a copy of the factory's prompt that drifts from it.
   - *Recommendation:* the factory: one prompt, and other repos the factory runs on get evals too.
   - *Depends on it:* Solution overview, "A run", "Suites and comparisons".

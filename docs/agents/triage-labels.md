# Triage Labels

The skills speak in terms of five canonical triage roles. This file maps those roles to the `status:` values used in this repo's tickets (`docs/tickets/`, see `issue-tracker.md`).

| Label in mattpocock/skills | `status:` in our tickets | Meaning                                  |
| -------------------------- | ------------------------ | ---------------------------------------- |
| `needs-triage`             | `needs-triage`           | Maintainer needs to evaluate this ticket |
| `needs-info`               | `needs-info`             | Waiting on reporter for more information |
| `ready-for-agent`          | `ready-for-agent`        | Fully specified, ready for an AFK agent  |
| `ready-for-human`          | `ready-for-human`        | Requires human implementation            |
| `wontfix`                  | `wontfix`                | Will not be actioned                     |

Beyond triage, a ticket is `in-progress` while being worked and `done` once it lands.

When a skill mentions a role (e.g. "apply the AFK-ready triage label"), set the ticket's `status:` to the corresponding string from this table.

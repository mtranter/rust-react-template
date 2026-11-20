---
id: 0069
name: "The SPA's Assistant page lists, starts and deletes conversations, shown only when the assistant is on"
status: ready-for-agent
spec: docs/platform/specs/0012-users-chat-with-an-assistant-that-acts-in-the-app-as-them.md
blocked_by: [docs/platform/tickets/0065-users-start-list-get-and-delete-their-own-conversa.md]
blocks: []
---

# The SPA's Assistant page lists, starts and deletes conversations, shown only when the assistant is on

Add `frontend/src/features/assistant` and the route `/assistant`, linked from the header only when the status operation says the assistant is on (as `useManagesAccess` learns from a `403`). The page lists the user's conversations newest first, with a "new conversation" button and delete. The chat pane comes in the next ticket.

## Acceptance criteria
- [ ] A hook reads the status operation; when off (or `chat` not granted) the header link is hidden and `/assistant` is not reachable.
- [ ] The page lists the caller's conversations newest first, starts a new one, and deletes one (with `If-Match`), through the generated client.
- [ ] Tests in the feature's style (`frontend/AGENTS.md`): assistant off hides link and page; list, create, delete.
- [ ] `scripts/init.sh` keeps the page and link; `make check-init` passes; `make e2e` unchanged.
- [ ] `frontend/AGENTS.md` describes the feature; `frontend/src/api/` is not hand-edited; `make check-changed` passes.

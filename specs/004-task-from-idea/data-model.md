# Data model: Create a task from an idea, with Claude

Migration `crates/delilah-daemon/migrations/0014_idea_conversations.sql`. The shipped
ones are not edited.

## `sessions` (changed)

| Column | Change | Notes |
|---|---|---|
| `conversation` | **new**, `INTEGER NOT NULL DEFAULT 0` | 1 = an idea conversation: shown in the conversation view, history recorded, launched with `dontAsk` (not `plan`), the plugin and the idea system prompt. Only for project sessions (`project IS NOT NULL`). |

`SessionRecord` gains `conversation: bool` and `SessionView` exposes it. Existing rows
are 0, so they keep their terminal.

Invariant: `conversation = 1` ⇒ `project IS NOT NULL AND kind = 'claude'`. It is checked
in `open_in_project`, not by a `CHECK` (that would need a table rebuild as in 0007).

## `conversation_entries` (new)

```sql
CREATE TABLE conversation_entries (
    id         INTEGER PRIMARY KEY AUTOINCREMENT,
    session_id TEXT NOT NULL REFERENCES sessions (id) ON DELETE CASCADE,
    at         TEXT NOT NULL,           -- RFC 3339, when it was created
    kind       TEXT NOT NULL,           -- 'user' | 'claude' | 'card'
    body       TEXT NOT NULL            -- JSON, by kind (below)
);
CREATE INDEX conversation_entries_by_session ON conversation_entries (session_id, id);
```

Order is `id`. Bodies:

- **user**: `{ "text": "…", "state": "queued" | "sent" | "delivered", "from": "view" | "terminal" }`
  - Sent from the view while the session is working → `queued`. Otherwise it is typed at
    once → `sent`.
  - `queued` → `sent` when the daemon types it (the turn ended, or Detener). Several
    queued entries are typed together, as one prompt.
  - `sent` → `delivered` when a `UserPromptSubmit` whose `prompt` contains the text arrives.
  - A `UserPromptSubmit` that matches no `sent` entry is stored as a new
    `{ from: "terminal", state: "delivered" }` entry.
  - Only `queued` entries can be deleted (`DELETE …/messages/{id}`).
- **claude**: `{ "text": "…" }`, from `Stop`'s `last_assistant_message`. Nothing is stored
  when that is empty.
- **card**: `{ "ask": "<ask id>", "questions": [...], "proposal": { "title", "brief" } | null, "answer": { ... } | null, "outcome": "pending" | "answered" | "withdrawn" | "terminal", "task": "LIBRE-7" | null, "error": "…" | null }`
  - Stored when the ask arrives, as `pending`.
  - `answered`: carries the answers map, plus `task` or `error` for a proposal.
  - `withdrawn`: after Detener.
  - `terminal`: the ask was released to the terminal.

## Live session (in memory, changed)

`Live` gains `activity: Option<String>`. It is set from `PostToolUse`
(`"Leyendo api.rs"`, `"Buscando \"create_task\""`, `"git log"`) and cleared on `Stop`. It
is never stored. It also gains `needs_terminal: bool`, set by a `Notification` of type
`permission_prompt` and cleared by the next prompt or tool.

## `Ask` (in memory, changed)

Gains `proposal: Option<Proposal { title, brief }>`. It is set when all of these hold:
- the owner is `@project`;
- there is exactly one question, with `header == "Nueva tarea"`;
- its options include `Crear`, whose `preview` is the brief;
- `question` is the title.

Otherwise it is `None`, an ordinary question.

## Task (existing, used)

A task created from a proposal is built by the same core as `POST /api/v1/tasks`:
- `free: true` → `LIBRE-n` via `next_free_key`;
- `project` = the session's project;
- `workflow` = the project's;
- `in_checkout: None`;
- `notes` = `> Brief escrito por Claude en la conversación «<label>», <date>.\n\n<brief>`;
- `status` = `Queued` (backlog).

## Task history (changed)

`delilah_core::task::Event` gains
`CreatedFromConversation { session: String, label: String }`. It is valid only on a
`Queued` task and changes neither status nor phase. It is written in the same
transaction that creates the task (`Store::create_task_from_conversation`). The UI shows
it as "Creada desde la conversación «label»".

## Lifecycle of an idea conversation

```text
open (POST …/projects/{p}/sessions {conversation:true}) → starting → waiting
waiting --message--> working --Stop--> waiting   (queued messages typed → working)
working --Detener--> waiting   (asks withdrawn; queue typed if any)
any --card--> waiting on card --answer--> working
any --permission_prompt--> needs terminal --next prompt/tool--> working
process ends → stopped (live = false) --Reabrir--> starting (--resume)
close → closed (hidden; entries kept)
```

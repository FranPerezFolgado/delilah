# Data model: Saving mode

Migration `crates/delilah-daemon/migrations/0011_saving_mode.sql`:

```sql
-- Saving mode: each phase of the task runs in a fresh Claude session.
ALTER TABLE tasks ADD COLUMN saving_mode INTEGER NOT NULL DEFAULT 0;
-- The handoff under way, as JSON ({"to":"implement","stage":"offered"}); NULL if none.
ALTER TABLE tasks ADD COLUMN handoff TEXT;
-- The step of the saving-mode chain this session runs: plan, implement, review.
ALTER TABLE sessions ADD COLUMN step TEXT;
```

## Task (delilah-core `Task`)

| Field | Type | Notes |
|---|---|---|
| `saving_mode` | `bool` | default `false`; set at creation, changeable while running |
| `handoff` | `Option<Handoff>` | the handoff under way |

## Handoff

| Field | Type | Notes |
|---|---|---|
| `to` | `Step` | `implement` or `review` |
| `stage` | `offered` \| `saving` \| `starting` \| `failed` \| `skipped` | |
| `error` | `Option<String>` | for `failed` |

Transitions (`handoff.rs`):

```
(none) ──plan turn ends, tasks ready──► offered ──confirm──► saving ──turn ends──► starting ──launched──► (none)
                                          │  └─decline──► (none), saving_mode = false   │             └─fails──► failed ──retry──► starting
                                          │                                             └─(save-notes off)──────► starting
(none) ──implement turn ends, all checked──► saving | starting   (no offer: automatic)
(none) ──code edit by the plan session──► skipped(to: implement)  (no offer; review still applies)
restart: saving → offered (implement) | starting (review); starting → starting (retried)
```

## Step

`plan` ("Spec y plan") → `implement` ("Implementación") → `review` ("Revisión"). Labels translated with `t()` / `lang.pick`.

## Session (`SessionRecord`, API `Session`)

| Field | Type | Notes |
|---|---|---|
| `step` | `Option<Step>` | `None` for non-chain sessions (shells, extra Claudes, tasks without saving mode) |

Invariant: a running task has exactly one session with `primary = true`; at most one session per `step`.

## Event (delilah-core)

`StepStarted { step: String, session: String }`: recorded in `task_events`, status unchanged (like `PhaseReached`); the history shows "Paso a Implementación (sesión nueva)".

## Preferences

| Key | Type | Default |
|---|---|---|
| `saving-mode` | bool | `false` |
| `save-notes` | bool | `true` |

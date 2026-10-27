# Phase 3 contract review (163c0ea..HEAD)

[BREAKING] 409 refusal body ↔ app/src/api.ts request() — producer crates/delilah-daemon/src/api.rs:1067 (ApiError::refused, body {error: <reason>, message, ...detail}) ↔ consumer app/src/api.ts:846-847
Problem: CONFIRMED. request() builds ApiError from `data?.error` only, so a refusal shows the raw code ("nesting", "not-a-parent-task", "parent-task-not-started"...) instead of the localised `message`; `ApiError.reason` reads `data.reason`, which refusals never set, so the UI cannot branch on the reason; detail fields (key, branch, project, subtasks) are dropped. Affects create (api.rs:555-585) and start (run.rs:559, 677).
Fix: in request(): `const msg = data?.message ?? data?.error`; set reason = data?.reason ?? (data?.message ? data.error : undefined) (a refusal's `error` is the reason, and old errors never have `message`); keep detail (e.g. `readonly detail?: unknown`). Add a vitest case for {error:"nesting",message:"..."}. The CLI already does this (crates/delilah-cli/src/main.rs:743-750).

[OK] TaskDto / NewTask — api.rs:478 (flatten + skip_serializing_if) ↔ api.ts:105-109,237-239
Additive; kind/parent_task/subtask_progress optional on the TS side, `taskKind()` defaults to normal. Core Task serde of kind/parent_task is old-client safe (api tests cover list/get). POST /tasks returns the task (tests assert kind/parent_task); verify it also uses TaskDto if progress is wanted there (parent has 0 subtasks at creation, so harmless).

[OK] GET /tasks/{key}/subtasks, POST push-branch — api_hierarchy.rs ↔ api.ts:120-128,863,868
Fields match contract (key,title,status,archived,dropped,pr{url,state}); 404 for non-parent. SubtaskPrSummary never emits "draft" (TS allows it): harmless. push-branch returns {branch}; TS types unknown.

[RISK/info] api.ts declares calls with no daemon route yet: make-parent-task, attach, prs/{id}/ready, empty_commit, confirm_live_subtasks (silently ignored by serde), live-subtasks refusals, base-status additions. These belong to later stories (tasks.md phase 3 = US1 only), so not a phase 3 defect, but the UI must not ship wired to them before those land (404).

[OK] CLI — main.rs: `--parent-task` is a bool flag + positional title (contract writes it with a title: equivalent), `--parent KEY`, mutually exclusive (clap conflicts_with); refusals print `message` and exit 1; ls/show tolerate old daemons (kind optional). make-parent-task and attach --parent subcommands absent: later story.

[OK] app/src/api.ts:164 parent_task_merge made optional: correct tolerance for older daemons.

VERDICT: ❌ BREAKING (one HIGH: api.ts request() ignores refusal message/reason)

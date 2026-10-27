# Phase 2 contract review (0ecfef3..HEAD)

No CRITICAL or HIGH findings.

[OK] Task serde (task.rs) <-> api.ts Task: `kind` (snake_case values match TaskKind) and `parent_task` are `#[serde(default)]` on read, skipped on write when normal/None; TS has them optional with `taskKind()` fallback. Old JSON round-trip test present. `subtask_progress` is not on the core Task yet (computed, daemon DTO, later WP).

[OK] Migration 0025: additive, NOT NULL DEFAULT 'normal'.

[OK] 409 refusals (lang.rs): all 13 reasons from contracts/api.md present, kebab-case reason tested against serde name; body {error,message,...detail} tested; detail keys (key/project/branch/subtasks) match the contract.

[RISK, MEDIUM] ProjectSettings.parent_task_merge — project.rs:~128 <-> api.ts:~164
Problem: TS field is required, but its comment says absent (older daemon) reads as "merge"; the Rust side has `deny_unknown_fields`, so a new UI saving settings to an old daemon would be rejected.
Fix: make it `parent_task_merge?: MergeMethod` (like ci_autofix) and read with `?? "merge"`; ship daemon and UI together.

[RISK, LOW] Contract says Task items "gain kind"; the core omits `kind` for normal tasks. Fine for the optional TS type; make sure the daemon DTO and CLI treat absent as normal. api.ts `SubtaskProgress` renames contract `Progress` (name clash), harmless.

[OK] Not yet verifiable (endpoints not in this diff): subtasks, make-parent-task, attach, ready, push-branch, empty_commit, confirm_live_subtasks, base-status `base`/`subtask_commits` (TS has `base` already). TS client types for them are present.

VERDICT: COMPATIBLE (one MEDIUM to tidy)

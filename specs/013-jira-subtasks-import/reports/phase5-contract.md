# Phase 5 contract review: skip/unskip

Contract: POST/GET /api/v1/jira/issues/{key}/import
Producer: crates/delilah-daemon/src/jira_import.rs (ImportRequest ~l.504, ImportResult ~l.522, SubtaskRow ~l.65)
Consumers: crates/delilah-cli/src/main.rs (ImportOutcome ~l.875, PreviewSubtask ~l.840, import_body ~l.935), app/src/api.ts (l.385-430), SubtaskList.tsx, JiraImportDialog.tsx

[OK] Request fields: add/skip/unskip are `#[serde(default)] Vec<String>` in the daemon. The CLI omits empty skip/unskip. The TS client always sends all three. Names and types match.
[OK] Response fields: skipped/unskipped Vec<String> match the CLI (`#[serde(default)]`) and TS (string[]). Row `skipped: bool` is always serialized and matches TS `skipped: boolean` and CLI `#[serde(default)] bool`.
[OK] Status codes: unknown key in add/skip/unskip gives 400 `not-a-subtask`, the same code for all three lists. `project-required` is unchanged. The TS client and CLI surface the error message and nothing branches on the code.
[OK] Semantics: a key in both `add` and `skip` is not added, and skip wins over unskip in the transaction (remove, then add). The dialog and SubtaskList never send overlapping sets by construction.
[RISK] Result `skipped`/`unskipped` echo the requested keys, not the keys that changed. Harmless, but the CLI prints "skipped: X" even if X was already skipped.
[RISK] CLI `--skip "A, B"` stores " B" in args; `import_body` trims it, so the wire is correct. Only the arg struct holds untrimmed values (the test pins this).
[RISK] `jiraImportPreview` in TS takes no `project` although the daemon accepts `?project=`. Not related to skip; no drift.

No CRITICAL or HIGH findings.
VERDICT: ✅ COMPATIBLE

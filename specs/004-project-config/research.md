# Research: Per-project configuration

## Where the settings are stored

- **Decision**: one `settings` column on `projects` (JSON text, default `'{}'`), added by
  migration `0016_project_settings.sql`. Missing keys mean "default".
- **Rationale**: renames and deletes already handle the `projects` row, so the settings
  follow for free (spec edge cases). JSON leaves room for `pr` later (FR-014) without
  another migration. Nothing is written into the repo (decision 50).
- **Alternatives**: a `project_settings` table with one column per setting (a migration
  per new setting, and rename/delete to keep in step); a `delilah.toml` in the repo
  (ruled out by the user, decision 50).

## Types and validation

- **Decision**: `ProjectSettings` in `delilah-core::project` (serde, every field
  optional or defaulted), with pure functions `validate()` and `branch_name()`. The
  submodule check (does the parent have it?) runs in the daemon, which can list
  submodules.
- **Rationale**: the core is where the task and project types and their pure rules live
  and are unit tested; the CLI and the daemon share them.
- **Alternatives**: validating only in the app (the CLI would bypass it, FR-005).

## Branch names and the preview in the new-task form

- **Decision**: `branch_name(template, key, title)` in Rust is the source of truth. The
  app has a small TS mirror (`branchName.ts`) for the live preview. Both are tested
  against one shared table of cases, `crates/delilah-core/tests/branch_names.json`.
  Slug: NFKD, strip accents, lower-case, non-alphanumerics → `-`, collapse and trim
  hyphens, cut at 40 characters at the last hyphen.
- **Rationale**: the preview must update on every keystroke with no request; the shared
  table keeps the two in step.
- **Alternatives**: a preview endpoint (a request per keystroke, debounce, latency).

## Shared entries

- **Decision**: in `delilah-git` (new `share.rs`), after the worktree exists:
  copy with `cp -cR` (APFS clones, near-instant even for `node_modules`), link with an
  absolute symlink to the checkout's entry. An entry already in the worktree is left
  alone; a missing source is skipped. Both are reported back for the task's log.
- **Rationale**: `cp -c` uses `clonefile`, so copying a large folder costs almost no
  time or space on APFS; macOS is the only target.
- **Alternatives**: a Rust recursive copy (slow and full-size on large folders);
  hard links (break on folders).

## Running the setup command

- **Decision**: run it as a session of a new kind `setup` in a daemon PTY, in the
  worktree, with `delilah_agents::env::agent_env()` and the user's login shell
  (`$SHELL -lc <command>`). Its output goes to the session's log in `tasks/<KEY>/`
  (FR-011) and streams to the app like any terminal (FR-016). Cancelling kills the PTY.
  A 10-minute timer kills it too.
- **Rationale**: sessions already give live output, scrollback, a log file and a tab in
  the task screen. Only the exit code is missing: `PtySession` waits on the child but
  drops the status; it will keep it (`exit_code()`).
- **Alternatives**: a plain `Command` with piped output (needs its own streaming and
  UI); running it inside the Claude session (the agent would see it, and a failure could
  not stop the launch).

## Not blocking the start request

- **Decision**: `start_run` returns once the workspace exists. With a setup command, the
  task stays in `PreparingWorkspace` and a background job runs the setup; on success it
  applies `WorkspaceReady` and launches the agent (the code that follows today, moved
  into a function both paths call).
- **Rationale**: the setup can take up to 10 minutes; the HTTP request and the app's
  dialog cannot wait that long.

## Failed setup

- **Decision**: a new status `SetupFailed { reason }`, not terminal, with events
  `SetupFailed { reason }` (from `PreparingWorkspace`), `RetrySetup` (→
  `PreparingWorkspace`, runs it again) and `SkipSetup` (→ `PreparingWorkspace`, then
  `WorkspaceReady` at once). `Fail` and `MarkDone` also apply. It is stored like every
  status, so it survives a restart (FR-010). At daemon start, a task left in
  `PreparingWorkspace` with a setup session that is not running becomes `SetupFailed`
  ("interrupted").
- **Rationale**: `Failed` is terminal; the worktree must stay usable and the two actions
  must stay available. `StartAborted` would delete the worktree (the user chose to keep
  it).
- **Alternatives**: reusing `Failed` with a marker in the reason (fragile, and terminal).

## Contracts

- **Decision**: `Project` gains `settings` (always the effective settings, defaults
  filled, FR-017). New `PUT /api/v1/projects/{name}/settings` (whole object; 422 with
  per-field errors). New `POST /api/v1/tasks/{key}/setup/retry` and `/setup/skip`. The
  run request is unchanged: `in_checkout` and `branch` already exist; the daemon fills
  them from the settings when absent, and merges submodules.
- **CLI**: `delilah project show <name>`, `delilah project set <name> <setting>
  <value>`, `delilah project unset <name> <setting>`, and `delilah setup retry|skip
  <KEY>`.

# Research: Task branch names that describe the task

## R1. Asking Claude for the name

- **Decision**: a headless one-shot like the submodule suggestion
  (`crates/delilah-agents/src/suggest.rs`): `claude -p <prompt> --model haiku
  --output-format json --no-session-persistence --strict-mcp-config --tools ""
  --setting-sources user`. No tools, so it answers from the title and notes alone, in
  one turn. It runs in a neutral folder (the daemon's temp dir), not the project, so no
  project `CLAUDE.md`, settings or hooks load. Clean env from `suggest::env()`
  (the agent's env, `TERM=dumb`). The prompt asks for JSON `{"name": "<2-4 English
  words, kebab-case>"}`. The answer goes through the same normalisation as any name.
- **Rationale**: the pattern is already shipped and tested (envelope parsing,
  `is_error`, JSON in prose). Haiku is fast and cheap enough for a short summary.
  `--no-session-persistence` keeps it out of sessions and tasks (FR-013).
- **Alternatives**: the Anthropic API directly (needs a key Delilah doesn't store;
  the CLI uses the user's own login). The task's agent naming its own branch
  (the branch has to exist before the agent starts, and renaming it later breaks
  the stored run).

## R2. Time budget and fallback

- **Decision**: the start dialog asks for a proposal 1 s after the title stops
  changing, with a 10 s limit. The developer can type a name at any moment; the
  proposal never overwrites a name the developer edited. When the start request
  carries no branch (CLI, API, or the proposal didn't arrive), `start_run` asks
  Claude itself with a 5 s limit. On a timeout, error, `is_error` (usage limit,
  signed out) or an empty answer: slug of the title. If that is empty too:
  `feature/<KEY>` (lowercased for free tasks).
- **Rationale**: in the dialog the name is ready before the developer clicks
  "Empezar", so starting is no slower (SC-004). The no-dialog path waits 5 s at most.
- **Alternatives**: one fixed 3 s limit everywhere (the CLI starts in about 2–4 s
  and haiku takes 1–3 s more, so most calls would fall back).
- **Note**: the claude binary is `Config::claude_bin` (env `DELILAH_CLAUDE_BIN`,
  default `claude`), so the daemon tests and the preview can use a fake script,
  like `DELILAH_GH_BIN`.

## R3. Normalising a name (pure, in `delilah-core`)

- **Decision**: `delilah_core::branch`:
  - `slug(text, max)`: transliterate (`á à ä â ã å`→`a`, `é è ë ê`→`e`, `í ì ï î`→`i`,
    `ó ò ö ô õ ø`→`o`, `ú ù ü û`→`u`, `ñ`→`n`, `ç`→`c`, `ß`→`ss`, uppercase too),
    lowercase, drop anything else not in `a-z0-9`, use `-` between words, collapse
    runs, trim `-`, cut at the last word boundary that fits in `max` (50).
  - `title_slug(title)`: `slug` after dropping Spanish/English filler words (`de la el
    los las un una y o en con por para que del al a the of to for and or in on with an`)
    and keeping the first 6 words. If nothing is left once the filler words are gone,
    the filler words are kept.
  - `compose(key, short, free)`: `feature/` + (`<KEY>-` for Jira) + `short`.
  - `custom(input)`: a typed or API name; strips any number of leading `feature/`,
    `slug`s the rest (`/` becomes `-`) and returns `feature/<slug>`, or `None` when
    nothing is left (start refused with 400, FR-008).
  - `fallback(key, free)`: `feature/<KEY>` (Jira) or `feature/<key lowercased>` (free).
- **Rationale**: the same rules for Claude's answer, titles and typed names. Pure
  functions are easy to unit-test. No new crate for a dozen letters.
- **Alternatives**: the `deunicode` or `slug` crates (a new dependency to cover what
  a small table already does).

## R4. Unique names

- **Decision**: `delilah_git::free_branch(dirs, base) -> String`. A name is taken in a
  folder if `refs/heads/<name>` exists or `refs/remotes/*/<name>` does
  (`for-each-ref`). Suffixes are tried `-2` … `-99`; past that, the base with the
  current time. Which folders: the repo (a `repo` project, worktree or checkout), or
  each chosen submodule in the project's folder (a `parent` project; each submodule is
  its own repo, where the branch is pushed from). Applied in `start_run` to every name,
  proposed, typed or automatic.
- **Rationale**: FR-006. Checking remotes avoids a push collision with a branch
  someone else made.
- **Behaviour change**: today a typed `--branch` in checkout mode switches to an
  existing branch. Now it gets a suffix (the spec says a name is never reused). The app
  never sent `branch`; only `delilah run --branch` did.

## R5. Where the branch is stored and shown

- **Decision**: no migration. The branch is already stored as `workspaces.branch`
  (one row per started task). `Task` gains `branch: Option<String>`
  (`#[serde(default, skip_serializing_if = "Option::is_none")]`), filled by a
  `LEFT JOIN workspaces` in the three task `SELECT`s of `store.rs`. Archived tasks
  lose their workspace row and no longer show a branch. The task view shows it under
  the title. Existing tasks keep their stored branch (FR-010).
- **Alternatives**: a `branch` column on `tasks` (duplicates `workspaces.branch`).

## R6. Start dialog

- **Decision**: `NewTaskDialog` (one dialog for new, backlog "Empezar" and
  "Convertir en tarea") gets a "Rama" field: `feature/` as a fixed prefix and an
  editable input. It shows the proposal and its source ("propuesta por Claude" / "a
  partir del título"), "Proponiendo…" while waiting, and is left empty (automatic)
  if nothing arrives. The proposal comes from `POST /api/v1/projects/{name}/branch-name`.
  On submit, a non-empty field is sent as `branch`.

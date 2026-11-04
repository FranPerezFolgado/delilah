# Research: Track every PR a task opens

No open clarifications (autopilot); these are the design choices and what was ruled out.

## R1. Identity of a PR link

- **Decision**: a link is unique per `(task_key, url)` (URL in canonical form from
  `parse_pr_url(..).url()`); `repo` becomes a label.
- **Rationale**: the spec allows several PRs per repo (FR-001) and repos outside the
  workspace (FR-007); the URL is already canonical and already keys `pr_unlinked`.
- **Alternatives**: `(task_key, host, owner_name, number)` — equivalent but needs three
  columns in every lookup; keeping `(task_key, repo)` with synthetic labels — breaks FR-001.

## R2. Getting the agent's PRs

- **Decision**: parse PR URLs from `tool_response` of the `PostToolUse` Bash hook when the
  command contains `gh pr create`.
- **Rationale**: Claude Code's `PostToolUse` payload carries the tool's output; `gh pr
  create` prints the PR URL on stdout (and on stderr for "a pull request already exists").
  It covers any head branch and any repo, with no extra polling.
- **Alternatives**: `gh pr list --author @me` per repo — misses repos outside the workspace
  and catches PRs from other work; scanning the transcript — slower and duplicates the hook;
  `gh search prs --head` across GitHub — needs the head branch, which is the gap itself.
- **Shape confirmed (WP-01, T001)**: for a Bash tool the payload carries
  `tool_response = {stdout, stderr, interrupted, isImage, noOutputExpected}`; the same
  object appears as `toolUseResult` in Claude Code transcripts (checked on real session
  transcripts). Fixtures: `crates/delilah-daemon/tests/fixtures/hooks/post_tool_use_gh_pr_create.json`
  (URL on stdout) and `..._exists.json` (URL on stderr, stdout empty). They are
  hand-assembled from that shape with placeholder paths and `gh`'s documented output, not
  captured live (no Claude session was run from the package).
- **Differences**: (1) success: stdout holds only the URL (+ newline); stderr carries
  progress text ("Creating pull request ...") and must not be mistaken for a URL source
  beyond URL tokens. (2) already exists: stdout is empty and the URL is the last line of
  stderr, after the message. (3) `tool_response` is an object, not a string, so read
  `.stdout` and `.stderr` and fall back to nothing when either is missing.

## R3. Parent's own repo

- **Decision**: add `Workspace::pr_repos(opts)` instead of changing `repos()`.
- **Rationale**: `repos()` feeds archive checks and setup that rely on "submodules only for a
  parent"; `parent_has_branch` needs git, so a separate method keeps `repos()` pure.
- **Alternatives**: always search the parent repo — finds PRs of an unrelated branch of the
  same name (the doc comment of `parent_has_branch` warns about exactly this).

## R4. Not archiving early

- **Decision**: before archiving, one forced detection pass; never while the primary
  session is `Working`.
- **Rationale**: closes the window where PR A merged and PR B was not linked yet, without a
  timer. Turn-end already leads to a check.
- **Alternatives**: a fixed grace period after the last merge — slower and still racy;
  requiring the user to confirm — defeats the automatic archive.

## R5. Closed PRs

- **Decision**: keep the PR-loop rule (closed unmerged blocks the archive and notifies) but
  mark a closed PR `replaced_by` when a newer PR from the same repo and head branch is linked.
- **Rationale**: today's replace-in-place hid that case; with several PRs per repo the old
  row would block forever.

## R6. Archive verification of PRs not from the task's branch

- **Decision**: only PRs from the task's branch in a workspace repo go into `heads` for
  `check_merged`/branch deletion; others are trusted from the host's merged state.
- **Rationale**: there is no local branch to compare for them; FR-037 forbids touching remotes.
- **Needs**: `head_branch` in the snapshot (`headRefName`), `#[serde(default)]`.

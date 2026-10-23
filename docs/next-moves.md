# Next moves

Findings on what to build next, as of 2026-10-20 (version 0.3.0). The
recommendation: fix the two checkout-and-archive problems first, then
implement the task phase header (already planned), then per-project settings
once the PR work lands.

## Where we are

MVP-0 and MVP-1 are done (see [status.md](status.md)). Work in flight:

| Task | What | State | Branch |
|---|---|---|---|
| LIBRE-1 | PR handling: open or link PRs, a PR tab with CI and review state, archive when merged | Running | `feature/LIBRE-1` |
| FEAT-11 | The task's phase in the task screen header | Spec and plan done (`specs/003-task-phase-header`), no tasks yet | `feature/FEAT-11` |
| FEAT-4 | Task View | Waiting for the user; 9 files, +793/−298 uncommitted, on top of 0.2.2 | `feature/FEAT-4` |
| FEAT-5 | Project view left | Waiting for the user; no changes | `feature/FEAT-5` |
| FEAT-9 | Tasks | Failed: `workspace: . has uncommitted changes…` | none |

FEAT-4 and FEAT-5 lost their live sessions when the dev daemon was killed.
FEAT-9 hit the archive refusal that status.md lists as a known problem.

## Candidates

| # | Move | Value | Effort | Clashes with the PR branch |
|---|---|---|---|---|
| A | **Checkout friction:** ignore SpecKit's CLAUDE.md block in the clean-checkout check; make the archive refusal name the task and the files | High: it already broke FEAT-9 and blocks checkout tasks after every `/speckit-plan` | S (`delilah-git`) | No |
| B | **Task phase header** (FEAT-11) | Medium: asked for, small | S (planned; tasks + implement) | Light: both touch `TaskScreen.tsx` |
| C | **Triage FEAT-4 / FEAT-5:** salvage or drop the stale work | Medium: clears the board, may hold UI work worth keeping | S | Possibly (task screen) |
| D | **Per-project `delilah.toml`:** workspace mode, branch template, shared folders (`.env`, `node_modules`), setup script, submodule include/exclude, PR options | High: worktree tasks need setup to be usable; the PR options belong here | M | Yes for the PR options: start after LIBRE-1 lands |
| E | **Concurrency and budget:** max agents at once, per-task token/time budget with an 80 % warning | Medium, rising with the number of parallel tasks | M | No |
| F | **Preview button per task** and a browser MCP so the agent can see the preview UI | Medium: speeds up building Delilah with Delilah | M | Light |
| G | **Gates and declarative workflows** | High long-term | L | Yes: the PR flow is the last phase of the workflow |
| H | **More agents** (Codex, Pi) behind `AgentAdapter` | Low today: only Claude is used | L | No |
| I | **Mobile through Tailscale** | Unknown | L | No; blocked on the user's answers (see [vision.md](vision.md)) |
| J | **Real-ticket test on the work Mac**, including the usage-limit payload | High: the main flow has never run end to end on a real ticket | Not code; needs the work Mac | No |

## Recommendation

1. **A, checkout friction.** Small, independent of the PR branch, and it
   removes a failure that happens daily. Lives in `crates/delilah-git`.
2. **B, task phase header.** The plan exists; generate tasks and implement in
   the FEAT-11 task. Rebase over LIBRE-1 if it lands first.
3. **C, triage FEAT-4 / FEAT-5** alongside, in a few minutes: diff FEAT-4
   against main and decide.
4. **D, per-project settings** (decided: kept in Delilah with a settings screen, not a `delilah.toml`; decision 50), as the next real feature once LIBRE-1 is merged, so
   the PR options go straight into it.

After that, E or F depending on how many tasks run in parallel day to day.
Wait on G until the PR flow is in use; H and I stay parked.

## Open questions

- Is FEAT-4's uncommitted Task View work still wanted, or superseded by
  later task screen changes?
- When is the next work-Mac session (J)? It would reorder D and E if it finds
  problems.
- After D: concurrency and budget (E) or the preview button (F)?

# Implementation Plan: Portfolio README and personal-project tone

**Branch**: `014-portfolio-readme` | **Date**: 2026-11-01 | **Spec**: [spec.md](spec.md)

**Input**: Feature specification from `specs/014-portfolio-readme/spec.md`

## Summary

Rewrite `README.md` as a portfolio page following [outline.md](outline.md), moving install detail to `docs/usage.md` with no change in meaning. Add a Playground capture for the hero, as a GIF if a timeboxed attempt passes, stills otherwise. Soften support wording in `CONTRIBUTING.md` and one sentence of `SECURITY.md`, and record the new positioning as a decision. Deliver [dependabot-proposal.md](dependabot-proposal.md) without changing the configuration. Two approvals stay with the owner: the outline (FR-019) and any dependabot change (FR-017).

## Technical Context

**Language/Version**: Markdown; TypeScript only for the capture code in `app/e2e/screens.spec.ts`

**Primary Dependencies**: Playwright 1.61 (already in `app/`); Pillow 12.3 from the system Python for a scratch script that isn't committed

**Storage**: N/A

**Testing**: `quickstart.md` checks; `npx tsc --noEmit` in `app/` for the spec change. `screens.spec.ts` is skipped unless `DELILAH_SCREENS` is set, so the e2e suite isn't affected.

**Target Platform**: GitHub's README rendering, light and dark

**Project Type**: Documentation, in a desktop app repository

**Performance Goals**: Hero GIF under 5 MB

**Constraints**: README about 200 lines at most; install and release tooling, product code and version untouched; captures only from the spec's own daemon, never port 7420

**Scale/Scope**: About 8 tracked files changed, plus images

## Constitution Check

`.specify/memory/constitution.md` is the unfilled template, so it sets no gates. Against the ground rules in `CLAUDE.md`:

| Rule | How the plan meets it |
|---|---|
| Nothing from private repos | Demo data in captures; generic problem statement (FR-004, spec edge case) |
| Never touch the installed daemon | Captures use `e2e/setup.ts`, which reserves 7420 and the preview ports |
| No version bump | No bump, by FR-020 |
| Confirm the `gh` account | Active account is `FranPerezFolgado`; confirm again before pushing |
| PR squash-merged with a Conventional title | `docs: rewrite README as a portfolio page and set personal-project tone` |
| Docs stay true | `docs/decisions.md`, `docs/status.md`, `CLAUDE.md` updated (below) |

## Project Structure

### Documentation (this feature)

```text
specs/014-portfolio-readme/
├── spec.md
├── plan.md                  # this file
├── research.md
├── outline.md               # for the owner's approval
├── dependabot-proposal.md   # for the owner's decision
├── quickstart.md
└── tasks.md
```

No `data-model.md` or `contracts/`: the feature has no data and no interface.

### Files changed in the repository

```text
README.md                    # rewritten
CONTRIBUTING.md              # stance in the opening; "maintainer" wording
SECURITY.md                  # line 9 only: best-effort response
docs/usage.md                # new Install and Getting started sections; Contents updated
docs/decisions.md            # new decision on positioning; how it relates to 82
docs/status.md               # feature recorded; line 18 no longer current
CLAUDE.md                    # line 24: "short newcomer page" no longer true
docs/images/                 # Playground hero; board shots removed only if unreferenced
app/e2e/screens.spec.ts      # Playground screen and a motion switch, capture only
```

Unchanged unless approved: `.github/dependabot.yml`. Unchanged in all cases: `scripts/`, `.github/workflows/`, `crates/`, `app/src/`, `app/src-tauri/`.

## Design notes

**Order.** The outline gate comes first, because its result shapes everything after it. Captures and the rewrite can run in parallel after that, since they share no files; the README takes the image path as a known name (`docs/images/playground-{light,dark}.*`), and the hero markup is settled once the capture result is known.

**Decision record.** Add a new decision ("Portfolio positioning"): it replaces what decision 82 says about the README's purpose and contents (newcomers, quickstart), while the rest of 82 stands. Decision 82 gets an inline italic note, the file's style (*Superseded in part by N*). The next free number is checked then; 104 if none has been added.

**Capture.** A new serial test at the end of `screens.spec.ts`, so it sees the seeded sessions. Stills go through the existing `shot()`. Add one env var, `DELILAH_SCREENS_MOTION=1`, read by `open()` to set `reducedMotion: "no-preference"`. The GIF uses its own multi-frame screenshot loop rather than `shot()`, which disables animations and waits to settle. Without the env var, current behaviour is the same for every existing screen. English depends on `DELILAH_SCREENS_LANG=en`. Research R1-R3 hold the reasoning and the fallback.

**Install anchor.** Both the README and `docs/usage.md` keep an `## Install` heading, so `#install` still lands in either place (R6).

**The `despues` image path.** `DELILAH_SCREENS_OUT` already redirects output, so nothing lands in `docs/redesign/`.

## Complexity Tracking

None. No new scripts or dependencies in the repo.

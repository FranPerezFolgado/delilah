# Research: Portfolio README and personal-project tone

## R1. Featured visual: the Playground

- **Decision**: Feature the Playground (`app/src/playground/`), the pixel-art building of live sessions. Add a `Playground` screen to `app/e2e/screens.spec.ts`, which does not capture it today, reached only through `DELILAH_SCREENS` as the rest of that file.
- **Rationale**: It is the most distinctive view, and the brief names it. The spec's own daemon comes from `e2e/setup.ts`, which reserves port 7420 and the preview ports, so the installed app is never touched.
- **Alternatives**: The board shots already in the README (`docs/images/board-*.png`): accurate, but they look like any kanban tool. They stay available as a secondary still.

## R2. Filling the floors

- **Decision**: First try the sessions the spec already seeds (LIBRE-1 to LIBRE-8, among them live, running and paused tasks). If they don't show enough states (typing, reading, terminal, waiting, paused), fill the gap with `page.route` on `GET /api/v1/activity` inside the capture code only, with demo project names.
- **Rationale**: Activity is in-memory and rebuilt from hooks (decision 90), so a seeded fake session may show too little; a routed response is confined to the capture and changes no product code. Either way the content is demo data (FR-004).
- **Not verified yet**: whether the seed alone is enough. T003 settles it.

## R3. GIF viability

- **Findings**:
  - `screens.spec.ts` sets `reducedMotion: "reduce"` at line 176, and characters then stay in still poses, per `docs/usage.md`. Its screenshot call also passes `animations: "disabled"`. A GIF needs both off, behind a capture-only switch.
  - `ffmpeg` and `gifski` are not installed. Playwright's `ffmpeg-1011` has only `png` and `libvpx` encoders, so it can't write a GIF.
  - Pillow 12.3.0 is available in the system Python and can assemble a GIF or APNG from PNG frames.
- **Decision**: Viable in principle but untested: capture a sequence of screenshots of the running animation, then assemble them with Pillow in a scratch script that isn't committed. Timebox it (T004). Accept only if it's under 5 MB, the characters visibly move, and it loops cleanly.
- **Fallback**: Still images in both themes, plus recording instructions (`gh` release asset or `docs/images/`, using macOS screen recording) in `quickstart.md`, handed to the owner. This is a spec edge case, not a failure.
- **Alternatives**: `brew install ffmpeg` or `gifski`, ruled out because the spec assumes no new tools on the machine. A `.webm` from Playwright's `recordVideo`, ruled out because a relative video path doesn't play in a GitHub README.

## R4. Third-party art in the visual

- **Finding**: The sprites come from pixtuoid (MIT), credited in `app/src/playground/CREDITS.md`. The image at the top of the README shows that art.
- **Decision**: The README carries a one-line credit with links to pixtuoid and to `CREDITS.md`, next to the licence. Claims about the Playground must not imply the art is original. Decision 89 is not one of the five chosen, partly because #92 replaced its sprite-sheet ruling.

## R5. Linking decisions

- **Finding**: Decisions are bold paragraphs without anchors; numbers are not in file order (26 sits after 58) and 49 and 50 are each used twice.
- **Decision**: Link to the section anchor (`docs/decisions.md#pull-requests`) with the number in the text; avoid 49 and 50. Adding anchors to `decisions.md` or renumbering is out of scope.
- **Chosen decisions**: see `outline.md`. Each was checked against the file, including what it ruled out and whether it was later superseded.

## R6. What depends on the README

- **Finding**: `git grep` finds no link to a `README.md#…` anchor outside `specs/` and the archives, and no script, CI job or test reads the README's content. CI tests `scripts/install.sh` directly. `ProjectView` reads a project's own README at runtime, unrelated to this file.
- **Decision**: Sections can move freely inside the repo. External links to old anchors can't be checked; `#install` is kept as a heading, the only one likely to be linked from outside.

## R7. Where install detail goes

- **Decision**: `docs/usage.md` gets a new `## Install` section (manual download, why unsigned, verify a download) and a `## Getting started` section (the quickstart). `Building from source`, `Uninstall` and `Updates` are already there. Text moves unchanged in meaning; the README keeps requirements, the one-line command and one link.
- **Rationale**: `docs/usage.md` is the reference by decision 82, and already has those sibling sections.
- **Check**: `scripts/install.sh`, `SHA256SUMS`, attestations and auto-update stay untouched; only the prose describing them moves.

## R8. Support wording outside CONTRIBUTING

- **Finding**: Only `SECURITY.md:9` promises something ("acknowledge a report within about 7 days and will keep you informed"). The issue templates and `CODE_OF_CONDUCT.md` promise no response times. `CONTRIBUTING.md:3` ("Thanks for helping") and `:73` ("A maintainer reviews and squash-merges") read as a staffed project.
- **Decision**: Change that sentence in `SECURITY.md` to best effort, keeping the private reporting route and the threat model. Leave `CODE_OF_CONDUCT.md` as is. Update the comment at `.github/pull_request_template.md:15` only if it stops being true (it stays true, so no change).

## R9. Figures

- **Decision**: Numbers are computed by the commands in `quickstart.md`, at a stated revision, and recomputed if `main` has moved. No script is added to the repo: the figures are a snapshot, and a tool to maintain them isn't asked for.
- **Caveats shown or respected on the page**: lines are `wc -l` on tracked files, blanks and comments included, lockfiles and images excluded. Test counts are static counts of test declarations, not runs. Decision count is 105 entries, while numbering reaches 103.

## R10. Constitution

`.specify/memory/constitution.md` is the unfilled template, so there are no gates from it. The repo's ground rules in `CLAUDE.md` apply instead.

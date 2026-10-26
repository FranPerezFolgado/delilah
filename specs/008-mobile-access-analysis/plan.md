# Implementation Plan: Mobile access analysis

**Branch**: `008-mobile-access-analysis` | **Date**: 2026-10-26 | **Spec**: [spec.md](spec.md)

**Input**: Feature specification from `specs/008-mobile-access-analysis/spec.md`

## Summary

An **analysis-only** feature (FR-009): no production code, migration or API contract changes.
The deliverable is [research.md](research.md), the decision-ready write-up. It is confirmed by a
throwaway experiment ([quickstart.md](quickstart.md)) and summarised into the project docs.

**Recommendation** (R1): reach the personal Mac's daemon through **Tailscale `serve`**
(tailnet-only HTTPS, never Funnel). The phone uses the existing React UI as an **iPhone
home-screen web app**: a phone layout, Web Push, and the UI served same-origin by the daemon.
Ruled out: same-Wi-Fi LAN, Cloudflare Tunnel, sideloaded native wrapper (7-day/3-app limits,
no push), Funnel.

**Blocking finding** (R3): behind `serve`, every request arrives from 127.0.0.1, so loopback no
longer says who calls. Today the `Host` guard still fails closed. The hole would open when the
tailnet `Host` is accepted, so code ties the two together: the tailnet `Host` is accepted only
while device-token enforcement is on. Step 1 brings per-device tokens (QR pairing, revocation),
one shared auth layer for HTTP and the three sockets, and a permission class per route.
Remote callers are always tokened. Local callers are unchanged (status.md:150 stays separate).

**Scope on the phone** (R6): see, answer, launch with confirm. Destructive and config actions
are refused server-side for device tokens. Terminal is read-only and never resizes.

**Work Mac** (R5): only if IT approves Tailscale. There is no acceptable fallback.

## Technical Context

**Language/Version**: n/a for this feature (Markdown only). Subject of the analysis: Rust
daemon (axum), React + TS UI (Vite), Tauri shell.

**Primary Dependencies**: none added. Analysed: Tailscale `serve` (external tool, user-installed),
iOS Safari home-screen web apps (iOS 16.4+).

**Storage**: none. Proposed for later: a devices table ([data-model.md](data-model.md)).

**Testing**: no code tests. Claims are checked by the experiment (quickstart Q1–Q6). Facts not
verified there stay marked "from docs, unverified" in research.md.

**Target Platform**: macOS host (personal Mac; work Mac conditional), iPhone client.

**Project Type**: desktop app (daemon + CLI + web UI); this feature is documentation.

**Performance Goals**: from the spec. SC-004 (answer in < 30 s from a notification) is
estimated; SC-005 (setup < 15 min) is measured during the experiment.

**Constraints**: FR-009 (no shipped code); previews only, never 7420 or the installed app;
nothing from work repos; English docs.

**Scale/Scope**: single user, 1–2 paired devices.

## Constitution Check

`.specify/memory/constitution.md` is the unfilled template. The project's rules in `CLAUDE.md`
act as the constitution:

| Rule | Status |
|---|---|
| Try changes in a preview, never 7420 / installed daemon | ✅ experiment uses `scripts/preview.sh` |
| Never reference work repos | ✅ work Mac is analysed as policy only |
| Confirm git/gh account before remote ops | ✅ no remote ops; user pushes |
| Migrations never edited | ✅ none |
| Contracts `api.rs` ↔ `api.ts` | ✅ unchanged; [contracts/device-auth.md](contracts/device-auth.md) is a proposal |
| Update status/decisions/vision when deciding | ✅ that is phase 3 of this plan |
| Version bump per Spec Kit task | ✅ **patch** (docs only, no new behaviour) |

Post-design re-check: no violations.

## Work phases

1. **Write-up** (done in this plan step): `research.md` R1–R8, `data-model.md`,
   `contracts/device-auth.md`, `quickstart.md`.
2. **Verify** (needs the user: Tailscale installed on Mac and iPhone): run the quickstart and
   record Q1–Q6 answers in research.md, flipping each **[verify in Qn]** to verified or
   corrected. If a result contradicts the recommendation (e.g. Q3 shows no recovery after lock),
   revise R1–R7 before phase 3. If the user skips it, the claims stay marked unverified and
   phase 3 says so.
3. **Docs**:
   - `docs/vision.md` "Mobile, through Tailscale": replace the sketch with the decided approach,
     link to research.md, and remove the answered open questions (iPhone; both Macs, work
     conditional on IT; launch allowed).
   - `docs/decisions.md`: new **#96 "Mobile through Tailscale serve and a home-screen web app;
     device tokens first"**, with ruled-out options. Amend #12's last line to point to #96.
   - `docs/status.md`, the "Mobile access" line (edit by content, not line number): "analysed
     (#96); next is roadmap step 1, device auth".
   - `docs/architecture.md`: no change (nothing built); the "(later: mobile web)" box stays.
   - `README.md`: no change (no user-facing behaviour).
4. **Close**: version bump (patch), PR.

## Project Structure

### Documentation (this feature)

```text
specs/008-mobile-access-analysis/
├── spec.md
├── plan.md               # this file
├── research.md           # the analysis (deliverable)
├── data-model.md         # proposed entities for step 1
├── quickstart.md         # the experiment
├── contracts/device-auth.md   # proposed API for step 1
├── experiment/           # screenshots/notes from the experiment (phase 2)
└── checklists/
```

### Source Code

No source changes. Places the roadmap would touch, for the later features:
- `crates/delilah-daemon/src/config.rs` (mobile switch, tailnet name), `api.rs` (guards,
  token middleware, static UI, device routes), a new migration (devices).
- `app/src/api.ts` (token header, same-origin default URL), `app/src/live.ts` and the
  terminal/live sockets (token on WS), `app/src/useWidth.ts` (phone breakpoint), a
  manifest and a service worker.
- Desktop: a "Devices" panel in settings.

**Structure Decision**: documentation only, under `specs/008-mobile-access-analysis/` plus
the four doc files named in phase 3.

## Complexity Tracking

None.

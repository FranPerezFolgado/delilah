# Tasks: Mobile access analysis

**Input**: Design documents from `specs/008-mobile-access-analysis/`
**Prerequisites**: plan.md, spec.md, research.md, data-model.md, contracts/device-auth.md, quickstart.md

**Analysis only (FR-009)**: no production code, migration, API contract or test changes. Every
task edits Markdown under `specs/008-mobile-access-analysis/` or `docs/`, plus the version bump.
Tests: none (not requested; nothing executable ships).

**Format**: `- [ ] [ID] [P?] [Story?] Description with file path`

---

## Phase 1: Setup

- [X] T001 Create `specs/008-mobile-access-analysis/experiment/` with a `README.md` stating it holds the experiment's notes and screenshots (throwaway, not shipped)

---

## Phase 2: Foundational — the write-up is complete (US1, US2 base)

**Goal**: research.md answers every FR before anything is verified or summarised.

- [X] T002 [US1] Check `specs/008-mobile-access-analysis/research.md` R1, R5, R6 against FR-001/FR-002/FR-004/FR-006/FR-007/FR-011/FR-012: every option row has all columns (cost, account/store, third parties, reach, HTTPS, push, setup, Mac asleep, work Mac); one recommendation; each ruled-out option has its reason; R5 says what data leaves the work Mac per option; R6 lists phone vs desktop-only actions; code facts cite file:line; fill any gap
- [X] T003 [US1] Add to `specs/008-mobile-access-analysis/research.md` a short "Against the vision sketch" note under R1: what is kept (serve, PWA, token pairing, push, see/answer/approve), changed (launch allowed with confirm; work Mac conditional), dropped (nothing) — US1 scenario 3
- [X] T004 [US2] Check `specs/008-mobile-access-analysis/research.md` R3 against FR-003 and the security checklist (`checklists/security.md` CHK007–CHK016): reachability, pairing, revocation, lifetime, local vs remote rule, launch guard, agent content, lock screen, stolen phone, Host/Origin coupling; fill any gap
- [X] T005 [US2] Check `specs/008-mobile-access-analysis/research.md` R7 against FR-005: steps ordered, sized, each states "mobile off = today's behaviour", step 1 specifiable on its own (SC-003); add an "Input for /feature" one-liner under step 1

- [X] T006 [US3] Add to `specs/008-mobile-access-analysis/research.md` an "R9. Journeys on the phone" section: for US3 (answer a waiting agent: notification → answer, taps and estimated seconds against SC-004 < 30 s), US4 (catch-up after lock), US5 (follow a session: conversation vs terminal) and US6 (launch: create task + start agent, steps and confirm), state for the recommended option what works, the limits, and the tap count

**Checkpoint**: research.md is decision-ready with claims still marked `[verify in Qn]`.

---

## Phase 3: Verify — experiment (US3–US6 yardsticks) ⚠️ needs the user

**Goal**: confirm or correct the `[verify in Qn]` claims on the real iPhone.
**Precondition**: the user has installed Tailscale on the personal Mac and the iPhone, with HTTPS certificates on (quickstart "Prerequisites"). **If the user skips this phase**, do T013 only.
**Rules**: preview only (`scripts/preview.sh`), never port 7420 or the installed app; screenshots go to `experiment/`, viewed by a subagent, not in the main thread; `tailscale serve reset` at the end.

- [X] T007 Run the quickstart "Run" block (preview with `DELILAH_UI_ORIGIN=https://<mac>.<tailnet>.ts.net`, static build, two `tailscale serve` mounts); note the setup time for SC-005 in `specs/008-mobile-access-analysis/experiment/README.md`
- [X] T008 [US4] Q1 + Q3: load on Wi-Fi and on mobile data; lock 10 min and unlock; record catch-up behaviour and time in `specs/008-mobile-access-analysis/experiment/README.md`
- [X] T009 [US2] Q2: record which `Host` reaches the daemon and the result of `curl` with no `Origin` from another tailnet device in `specs/008-mobile-access-analysis/experiment/README.md`
- [X] T010 [US3] Q4: add to Home Screen, record full-screen launch and iOS version in `specs/008-mobile-access-analysis/experiment/README.md`
- [X] T011 [US5] Q5 + Q6: conversation and terminal at phone width; whether opening the terminal on the phone resizes the Mac's view; record in `specs/008-mobile-access-analysis/experiment/README.md`
- [X] T012 Tear down: `tailscale serve reset`, stop the preview (Ctrl-C on its process, never by name)
- [X] T013 Update `specs/008-mobile-access-analysis/research.md`: turn each `[verify in Qn]` into "verified (Qn)" or a correction; if the phase was skipped, turn them into "from docs, unverified". If a result contradicts R1–R9, revise them and say so at the top of research.md

**Checkpoint**: no `[verify in` marker remains in research.md.

---

## Phase 4: Docs (US2 scenario 3, FR-008)

- [X] T014 [P] [US2] Rewrite the "Mobile, through Tailscale" section of `docs/vision.md` with the decided approach (serve + home-screen web app, device tokens first, scope see/answer/launch, work Mac only if IT allows), link `../specs/008-mobile-access-analysis/research.md`, and remove the answered open questions (which Mac/IT, iPhone or Android, answer-only vs launching); keep "Proposed but not decided" only for what is still open
- [X] T015 [P] [US2] Add decision **79** at the end of `docs/decisions.md`: "Mobile through Tailscale serve and a home-screen web app; device tokens first (2026-10-26)", with why, the local-vs-remote rule, and ruled out (LAN, Cloudflare Tunnel, sideloaded wrapper, Funnel, identity headers alone); change the last line of decision 12 to point to 79
- [X] T016 [P] [US2] In `docs/status.md`, replace the line starting `- **Mobile access:**` (find it by content) with "analysed (decision 79, [research](../specs/008-mobile-access-analysis/research.md)); next: roadmap step 1, device auth"
- [X] T017 Check that `docs/architecture.md` and `README.md` need no change (nothing built) and that links added in T014–T016 resolve

---

## Phase 5: Close

- [X] T018 Mark `specs/008-mobile-access-analysis/checklists/security.md` items still true after T013; note any changed by the experiment
- [ ] T019 Bump the version, **patch** (docs only, no new behaviour): `scripts/bump-version.sh patch --commit` (from `origin/main`; commit `chore: bump version to X.Y.Z`)
- [ ] T020 Draft the PR text (pr-writer), in English, stating the bump level and why; the user pushes and opens it

---

## Dependencies

- T001 → Phase 3.
- Phase 2 (T002–T006) → Phase 3 → T013 → Phase 4 → Phase 5.
- If Phase 3 is skipped: Phase 2 → T013 → Phase 4.
- T014, T015, T016 touch different files: parallel.
- T008–T011 share one session on the phone and one notes file: sequential.

## Parallel example

```text
After T013: T014 (docs/vision.md) ∥ T015 (docs/decisions.md) ∥ T016 (docs/status.md)
```

## Implementation strategy

- **MVP**: Phases 1, 2, T013 (skipped path) and 4. That already gives a decision recorded in the
  docs, with unverified claims marked.
- **Full**: add Phase 3 when Tailscale is installed. It can also run later as a follow-up that
  only edits research.md and, if something changed, decision 79.

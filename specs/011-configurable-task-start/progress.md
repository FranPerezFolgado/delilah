# Progress — 011-configurable-task-start

Branch: 011-configurable-task-start · Base: 633dd6e · Started: 2026-10-26
Mode: format=markdown commits=yes autonomy=auto

| WP | Phase | Group | Status | Attempts | Commit | Notes |
|----|-------|-------|--------|----------|--------|-------|
| WP-01 | 1 | 1A | done | 1 | a1a0733 | run with WP-02 (same file) |
| WP-02 | 2 | 2A | done | 1 | a1a0733 | run with WP-01; store.rs literal allowed |
| WP-03 | 2 | 2B | done | 1 | 07b53e1 | set_start_message instead of BacklogChange field; review text duplicated from handoff::step_prompt (WP-10 to dedupe) |
| WP-04 | 2 | 2C | done | 1 | cb1d2c2 | scope widened to all daemon src (user); API requests sending "feature" not normalised yet → WP-05 |
| WP-04F | 2 | fix | done | 1 | b0a55dc | review fix: feature alias normalisation, projects.workflow migration; unknown workflow values now 400 |
| WP-04F2 | 2 | fix | done | 1 | a5ba4b6 | 2nd fix round: UI autopilot check accepts "project" (interim until T037) |
| WP-05 | 3 | 3A | done | 1 | 951e664 | +core workflow.rs, autopilot/api.rs; e2e autopilot+chat_permissions set preset per test (user); prompt+autopilot now 409 |
| WP-06 | 4 | 4A | verified | 1 | 77f3857 | settings errors stay 422 (contract says 400); CLAUDE_CONFIG_DIR not honoured |
| WP-07 | 4 | 4A | verified | 1 | | +legacy projects.workflow fallback fix (run.rs) |
| WP-08 | 4 | 4A | verified | 1 | ecb0950 | SkillPicker {project,value,onChange}; start_message optional in TS |
| WP-09 | 4 | 4B | verified | 1 | f1b269f | +SkillPicker onSkills callback |
| WP-10 | 5 | 5A | pending | 0 | | |
| WP-11 | 5 | 5A | pending | 0 | | |
| WP-12 | 6 | 6A | pending | 0 | | |
| WP-13 | 6 | 6A | pending | 0 | | |
| WP-14 | 6 | 6A | pending | 0 | | |
| WP-15 | 7 | 7A | pending | 0 | | |
| WP-16 | 7 | 7B | pending | 0 | | |

## Open findings

| Phase | Reviewer | Severity | Finding | File |
|-------|----------|----------|---------|------|
| 1-2 | architecture | MEDIUM | ResolvedStart.workflow / Task.workflow stringly typed; parse once, match on TaskWorkflow (→ WP-05 brief) | crates/delilah-core/src/workflow.rs:253 |
| 1-2 | architecture | MEDIUM | resolve() takes whole &Task; Inherit silently → nothing on parse error/missing message (→ WP-05 brief) | crates/delilah-core/src/workflow.rs:302 |
| 1-2 | architecture | MEDIUM | ProjectSettings::workflow() clones on every call; is_phased vs ResolvedStart.phased two decision paths | crates/delilah-core/src/workflow.rs:228,277 |
| 1-2 | architecture/spec | MEDIUM | review text duplicated lang.rs ↔ handoff::step_prompt; test checks prefix/suffix only (→ WP-10) | crates/delilah-daemon/src/lang.rs |
| 1-2 | migration | MEDIUM | unset pref:language → Spanish preset, but runtime falls back to system language (English Mac user gets Spanish review text); per data-model, document in decisions.md | migrations/0022_workflow_commands.sql:5 |
| 1-2 | contract/spec | MEDIUM | UI checks workflow === "feature" for autopilot; migrated tasks now "project" → toggle hidden until UI phase (→ WP-08/11) | app/src/components/BacklogTaskView.tsx:57, NewTaskDialog.tsx:88 |
| 1-2 | contract | MEDIUM | TaskView shows raw workflow string ("project") | app/src/components/TaskView.tsx:151 |
| 1-2 | contract | LOW | ProjectSettings.workflow serialises as null; PUT without the field resets to builtin | crates/delilah-core/src/project.rs |
| 1-2 | spec | LOW | task.rs JSON fixtures still "feature"; {task} rule only tested via hand-built Vars; replay doesn't re-check task workflows | crates/delilah-core/src/task.rs:899 |
| 1-2 | architecture | LOW | render/validate duplicate brace scanning; run.rs still hard-codes /feature until resolve wired | crates/delilah-core/src/workflow.rs:84-205 |
| 1-2 | migration | LOW | 0022 header comment does not mention projects.workflow rewrite / prompt fallback | migrations/0022_workflow_commands.sql:1-3 |
| 1-2 | contract | MEDIUM | PATCH backlog task stores change.workflow raw (no normalise_workflow) (→ WP-12) | crates/delilah-daemon/src/api.rs:540-550 |
| 1-2 | contract | LOW | build_task 400 on unknown project workflow vs run.rs fallback to Prompt; autopilot regression test only asserts not-409 | crates/delilah-daemon/src/api.rs:474 |
| 1-2 | spec | MEDIUM | run.rs feature+autopilot regression test only asserts not-409 | crates/delilah-daemon/src/run.rs:3524 |
| 3 | contract | HIGH (scheduled) | UI offers autopilot on fresh (non-phased) projects → 409; fixed by T037 (WP-11), same release | app/src/components/NewTaskDialog.tsx:90, BacklogTaskView.tsx:58 |
| 3 | contract | HIGH (scheduled) | POST /tasks prompt / PATCH start_message not implemented → T039-T040 (WP-12); api.ts Task lacks start_message → WP-08 | crates/delilah-daemon/src/api.rs:315-342 |
| 3 | contract | MEDIUM | new tasks fall back to legacy projects.workflow; "prompt" there → start with nothing instead of built-in (→ WP-07) | run.rs start, api.rs build_task |
| 3 | contract | MEDIUM | PATCH writes workflow before the autopilot 409 (partial apply, pre-existing) (→ WP-12) | crates/delilah-daemon/src/api.rs:~560 |
| 3 | contract | MEDIUM | prompt+autopilot now 409 (by contract); stored workflow becomes "prompt" when a prompt is given | crates/delilah-daemon/src/run.rs |
| 3 | contract | LOW | blank prompt silently → Inherit; build_task stores saving_mode for non-phased backlog tasks; CLI --prompt help still says /feature (→ WP-13) | run.rs, api.rs, cli main.rs:222 |
| 3 | spec | MEDIUM | data-model table says request workflow project/feature → task's stored choice; code maps to Project (matches FR-007 "follow the project's workflow"); fix the table (→ WP-15) | data-model.md, run.rs ~L334 |
| 3 | spec | LOW | backlog/CLI free LIBRE-n task now gets /feature <title> instead of /feature LIBRE-n (R4, deliberate); note in docs (→ WP-15) | run.rs |
| 3 | spec | LOW | workflow prompt without prompt accepted if stored message; typed prompts now rendered as templates; misplaced doc comment | run.rs ~L335, e2e chat_permissions.rs:21 |

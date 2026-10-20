---
description: "Task list for Subtareas y sprint actual en la vista Jira"
---

# Tasks: Subtareas y sprint actual en la vista Jira

**Input**: Design documents from `specs/001-jira-subtasks-sprints/`

**Prerequisites**: plan.md, spec.md, research.md, data-model.md, contracts/jira-issues.md, quickstart.md

**Tests**: included. The plan asks for tests first in `jira.rs` (parsing, JQL, sample `list`) and the daemon endpoint, following the inline `#[cfg(test)]` style already in both crates.

**Organization**: grouped by user story (US1 sub-tasks, US2 sprint filter, US3 sprint name).

## Format: `[ID] [P?] [Story] Description`

- **[P]**: Can run in parallel (different files, no dependencies on incomplete tasks)
- **[Story]**: US1, US2, US3 (spec.md)

## Path Conventions

- Integration: `crates/delilah-integrations/src/jira.rs`, fixture `crates/delilah-integrations/fixtures/jira-search.json`
- Daemon: `crates/delilah-daemon/src/jira.rs`
- App: `app/src/api.ts`, `app/src/components/JiraView.tsx`, `app/src/styles.css`, `app/src/i18n/en.ts`

---

## Phase 1: Setup

**Purpose**: confirm the baseline is green before touching anything.

- [X] T001 Run `cargo test -p delilah-integrations jira`, `cargo test -p delilah-daemon jira` and `cd app && npx tsc --noEmit`; note any pre-existing failure before starting (no files changed)

---

## Phase 2: Foundational (Blocking Prerequisites)

**Purpose**: the new types and field discovery that both US1 and US2/US3 rely on. Every task here touches `crates/delilah-integrations/src/jira.rs`, so they run in sequence.

- [X] T002 Add `JiraParent { key: String, summary: Option<String> }` and `JiraSprint { name: String, state: String }` (both `Debug, Clone, PartialEq, Eq, Serialize, Deserialize`) and the fields `parent: Option<JiraParent>`, `subtask: bool`, `sprint: Option<JiraSprint>` on `JiraIssue`, each `#[serde(default)]`, with doc comments in the file's style, in crates/delilah-integrations/src/jira.rs
- [X] T003 Replace `epic_link_field()` with `custom_fields() -> CustomFields { epic_link: Option<String>, sprint: Option<String> }`, read from one `GET field` (`schema.custom` == `EPIC_LINK_SCHEMA` / new const `SPRINT_SCHEMA = "com.pyxis.greenhopper.jira:gh-sprint"`), called by `search` on **both** Cloud and Data Center; append the found ids to `fields`; change `map_issue(issue, site, epic_field)` to `map_issue(issue, site, &CustomFields)` and update the `JiraClient::map_issue` wrapper, `issue()` and existing tests, in crates/delilah-integrations/src/jira.rs
- [X] T004 Make `search` take `max_results: &str` (keep `MAX_RESULTS = "50"` for the main query, add `MAX_SUBTASKS = "100"`), updating every caller, in crates/delilah-integrations/src/jira.rs — done as a private `query(jql, max_results, &CustomFields)`; public `search(jql)` kept
- [X] T005 Add to `app/src/api.ts`: `JiraParent`, `JiraSprint` interfaces and `subtask: boolean; parent: JiraParent | null; sprint: JiraSprint | null` on `JiraIssue` (contracts/jira-issues.md)

**Checkpoint**: `cargo test -p delilah-integrations` and `tsc --noEmit` pass with the new fields empty everywhere.

---

## Phase 3: User Story 1 - See a ticket's sub-tasks nested under it (Priority: P1) 🎯 MVP

**Goal**: every sub-task of the listed tickets appears indented under its parent, inside the parent's epic or status group; orphans show "↳ KEY".

**Independent Test**: sample data, group by Épica → the sub-tasks hang under PROJ-1242 inside "Pagos sin sobresaltos", Ana's sub-task has avatar "A", the orphan shows "↳ …", none in "Sin épica" (quickstart.md, manual steps 2–4 and 6).

### Tests for User Story 1

- [X] T006 [US1] Extend `finds_the_epic_in_cloud_and_data_center` (or add `maps_the_parent_of_a_subtask`) so a sub-task with a story `parent` (`issuetype.subtask: true`, `hierarchyLevel: -1`) yields `subtask == true`, `parent == Some(JiraParent { key: "PROJ-1", summary })`, `epic == None`, and an epic `parent` yields `parent == None`, in crates/delilah-integrations/src/jira.rs
- [X] T007 [US1] Add `the_sample_nests_subtasks` tokio test: `list(Assigned, Open)` contains Ana's sub-task with `parent.key == "PROJ-1242"` and `epic.key == "PROJ-1200"`; the orphan sub-task (assigned to Fran, parent absent) is listed with its parent's epic; a sub-task of an unlisted parent assigned to someone else is **not** listed, in crates/delilah-integrations/src/jira.rs

### Implementation for User Story 1

- [X] T008 [P] [US1] Add sub-tasks to the fixture: two under PROJ-1242 (one assigned to "Fran" `indeterminate`, one to "Ana" `new`), one `done` under another listed story, and one assigned to "Fran" whose parent key (e.g. PROJ-1190, an epic-bearing story not in the fixture, assigned to "Ana") is absent, each with `issuetype { name: "Subtarea", subtask: true, hierarchyLevel: -1 }` and `parent { key, fields { summary, issuetype { name: "Historia", hierarchyLevel: 0 } } }`; add one top-level "Ana" ticket for PROJ-1190 only if needed by T007, in crates/delilah-integrations/fixtures/jira-search.json
- [X] T009 [US1] Implement `parent(fields) -> Option<JiraParent>` (the `parent` that `epic()` rejects) and `subtask` (`issuetype.subtask == true || issuetype.hierarchyLevel == -1`) in `map_issue`, in crates/delilah-integrations/src/jira.rs
- [X] T010 [US1] In `JiraClient::list` (remote): after the main search, collect keys of non-sub-task rows; if any, `search("parent in (K…) AND issuetype in subTaskIssueTypes()", MAX_SUBTASKS)` and append rows whose key isn't already listed; if Jira answers `JiraError::Http { status: 400, .. }`, retry once with `parent in (K…)` and keep only `subtask` rows (research R1), in crates/delilah-integrations/src/jira.rs
- [X] T011 [US1] Add `inherit_epics(&self, issues: &mut Vec<JiraIssue>)`: for each sub-task with `epic == None`, copy the parent's epic from the list; for parents not in the list, one `search("key in (…)", MAX_RESULTS)` and copy their epic (and fill `parent.summary` if missing); call it in `list` before `name_epics`, in crates/delilah-integrations/src/jira.rs
- [X] T012 [US1] Sample `list`: apply `filter.matches` to non-sub-tasks, add every fixture sub-task whose parent passed, keep sub-tasks that pass `matches` themselves (orphans), then run the same epic inheritance over the fixture (share the pure part of `inherit_epics` as a sync helper so Sample needs no I/O), in crates/delilah-integrations/src/jira.rs
- [X] T013 [US1] Update the daemon sample test `connects_to_the_sample_and_lists_its_tickets` to assert a row with `"subtask": true` and `"parent": {"key": "PROJ-1242", …}` is returned, in crates/delilah-daemon/src/jira.rs
- [X] T014 [US1] In `JiraView.tsx`, add `nest(issues)` returning top-level rows `{ issue, children }[]` (top level = no `parent` or parent not in the list; children keep API order); make `groupIssues` group the top-level rows only (child group = parent's), counts = top-level, and the epic-mode in-progress-first sort applies to top-level and to each children list, in app/src/components/JiraView.tsx
- [X] T015 [US1] Make the text filter tree-aware: keep a top-level row if it or any child matches; if the parent matches keep all children, otherwise only matching children; include `parent.key`/`parent.summary` in the haystack, in app/src/components/JiraView.tsx
- [X] T016 [US1] Render children after their parent with class `jrow child`; on the parent row a `.jsubs` badge with the child count; on an orphan a `jtag` `↳ {parent.key}` with `title={parent.summary}`; selection, `task_status` pill and "Analizar" work unchanged for children, in app/src/components/JiraView.tsx
- [X] T017 [US1] Detail panel: add a "Padre" row (`summary` + mono key; a button that selects it when it's in the list) shown only for sub-tasks, in app/src/components/JiraView.tsx
- [X] T018 [P] [US1] Add `.jrow.child { padding-left: 58px; }` (no epic tag needed) and `.jsubs` (small muted count badge, matching `.jgroup-count`) in app/src/styles.css
- [X] T019 [P] [US1] Add English translations for "Padre", "{n} subtarea"/"{n} subtareas" (if `tn` is used) in app/src/i18n/en.ts

**Checkpoint**: US1 fully working with sample data; `cargo test` green.

---

## Phase 4: User Story 2 - Filter by the current sprint (Priority: P1)

**Goal**: "Sprint: Todos | Actual" in the filter bar narrows to open sprints, combined with who/status and the saved query.

**Independent Test**: sample data, Sprint → Actual leaves only the "Sprint 42" tickets (and their sub-tasks); combine with Reportados/Hechos (quickstart.md, step 5).

### Tests for User Story 2

- [X] T020 [US2] Extend `builds_the_jql_for_each_filter` with `sprint: SprintFilter::Current` cases: assigned/open → `"assignee = currentUser() AND statusCategory != Done AND sprint in openSprints() ORDER BY updated DESC"`, query with saved `order by priority desc` kept, empty saved query → `"statusCategory != Done AND sprint in openSprints() ORDER BY updated DESC"`; existing cases get `sprint: All` and stay identical, in crates/delilah-integrations/src/jira.rs
- [X] T021 [US2] Add to `the_sample_answers_each_filter`: `list(Assigned, Open, Current)` keeps only tickets whose fixture sprint is `active` (plus their sub-tasks) and drops backlog/closed ones, in crates/delilah-integrations/src/jira.rs
- [X] T022 [US2] Add daemon test: `GET /api/v1/jira/issues?sprint=current` with the sample → 200 and every row has `sprint.state == "active"`; `?sprint=nope` → 400, in crates/delilah-daemon/src/jira.rs

### Implementation for User Story 2

- [X] T023 [P] [US2] Add a `customfield_10020` sprint value to the fixture: `[{ "id": 42, "name": "Sprint 42", "state": "active" }]` on most open tickets (incl. PROJ-1242), `[{ "id": 41, "name": "Sprint 41", "state": "closed" }]` on one open ticket, none on at least one open ticket (backlog); sub-tasks carry their parent's sprint, in crates/delilah-integrations/fixtures/jira-search.json
- [X] T024 [US2] Add `enum SprintFilter { #[default] All, Current }` (`kebab-case` serde) and `#[serde(default)] pub sprint: SprintFilter` on `Filter`; rewrite `Filter::jql` to join `[who?, progress, sprint?]` with ` AND ` then the order (drops the `who.is_empty()` branch), in crates/delilah-integrations/src/jira.rs
- [X] T025 [US2] Extend `Filter::matches` with `self.sprint == Current ⇒ issue.sprint.state == "active"` (needs T026's sprint parsing in sample mode; Sample's `CustomFields.sprint = Some("customfield_10020")`), in crates/delilah-integrations/src/jira.rs
- [X] T026 [US2] Implement minimal `sprint(fields, &CustomFields) -> Option<JiraSprint>` for the Cloud object array (active > future > last) so US2 works standalone; US3 adds the DC string format, in crates/delilah-integrations/src/jira.rs
- [X] T027 [US2] Update the doc comment of `issues` to `?who=…&progress=…&sprint=all|current` in crates/delilah-daemon/src/jira.rs
- [X] T028 [US2] Add `export type JiraSprintFilter = "all" | "current"` and the `sprint` param to `api.jiraIssues(who, progress, sprint)` in app/src/api.ts
- [X] T029 [US2] Add `const [sprint, setSprint] = useState<JiraSprintFilter>("all")`, pass it to `api.jiraIssues`, add it to `load`'s deps, and a `<Segs label={t("Sprint")} options={[{id:"all",label:t("Todos")},{id:"current",label:t("Actual")}]} …/>` after "Estado" in `.jbar`, in app/src/components/JiraView.tsx
- [X] T030 [P] [US2] Add translations for "Sprint", "Todos", "Actual" in app/src/i18n/en.ts

**Checkpoint**: US1 + US2 work together; an error from a site without Jira Software shows in the existing `.error` banner.

---

## Phase 5: User Story 3 - See which sprint each ticket is in (Priority: P2)

**Goal**: sprint name on the row and in the detail panel, for Cloud and both Data Center formats.

**Independent Test**: sample data, select PROJ-1242 → "Sprint: Sprint 42"; a backlog ticket → "—" (quickstart.md, step 6).

### Tests for User Story 3

- [X] T031 [US3] Add `reads_the_sprint_in_each_format`: Cloud object array with closed+active → active; future only → future; closed+closed → last; DC string array `"com.atlassian.greenhopper.service.sprint.Sprint@1a2b[id=7,rapidViewId=3,state=ACTIVE,name=Sprint 42, pagos,startDate=…]"` → `{ name: "Sprint 42, pagos", state: "active" }`; missing field / `null` → `None`, in crates/delilah-integrations/src/jira.rs

### Implementation for User Story 3

- [X] T032 [US3] Extend `sprint()` to parse DC strings: `state=` value lower-cased; `name=` up to `,startDate=` / `,endDate=` / `,goal=` or the closing `]`; same selection rule, in crates/delilah-integrations/src/jira.rs
- [X] T033 [US3] Row: when `sprint === "all"` and `i.sprint`, render a `jtag` with `i.sprint.name` (skip on child rows), in app/src/components/JiraView.tsx
- [X] T034 [US3] Detail panel: a "Sprint" row showing `issue.sprint?.name ?? "—"`, in app/src/components/JiraView.tsx

**Checkpoint**: all three stories done.

---

## Phase 6: Polish & Cross-Cutting Concerns

- [X] T035 Run `cargo fmt`, `cargo clippy -p delilah-integrations -p delilah-daemon -- -D warnings`, `cargo test` (workspace) and `cd app && npx tsc --noEmit`; fix anything they report
- [ ] T036 Walk quickstart.md "Manual (sample data)" steps 1–6 in the running app (`/run`) and capture any mismatch
- [X] T037 [P] Update README.md's Jira section (if it lists the filters) to mention sub-tasks and the Sprint filter — N/A: the README does not list the filters
- [ ] T038 Decide with the user whether to keep or revert the `<!-- SPECKIT -->` block that `specify init` added to CLAUDE.md before committing

---

## Dependencies & Execution Order

### Phase Dependencies

- **Setup (T001)** → **Foundational (T002–T005)** → user stories.
- **US1 (Phase 3)** and **US2 (Phase 4)** both depend only on Foundational; they share `jira.rs`, `JiraView.tsx` and the fixture, so in practice run them one after the other (US1 first, MVP).
- **US3 (Phase 5)** depends on T026 (sprint parsing, US2).
- **Polish** after the stories you ship.

### Within each story

- Tests (T006–T007, T020–T022, T031) are written first and should fail.
- Fixture → mapping → `list` → daemon → api.ts → JiraView.

### Parallel Opportunities

- T005 (api.ts) in parallel with T002–T004 (Rust).
- US1: T008 (fixture), T018 (CSS), T019 (i18n) in parallel with the Rust work.
- US2: T023 (fixture), T030 (i18n) in parallel; T028–T029 (app) in parallel with T024–T026 (Rust) once the contract is fixed.

## Parallel Example: User Story 1

```text
Task: "T008 Add sub-tasks to crates/delilah-integrations/fixtures/jira-search.json"
Task: "T018 Add .jrow.child / .jsubs to app/src/styles.css"
Task: "T019 Add translations to app/src/i18n/en.ts"
```

## Implementation Strategy

### MVP First (User Story 1 only)

1. T001–T005 → 2. T006–T019 → 3. **Stop and validate** with sample data (quickstart steps 2–4, 6) → usable on its own: sub-tasks under their parent.

### Incremental Delivery

1. + US2 (T020–T030) → current-sprint filter.
2. + US3 (T031–T034) → sprint names, DC string format.
3. Polish (T035–T038), then try on the work Mac against the real Jira (quickstart "Manual (real Jira)").

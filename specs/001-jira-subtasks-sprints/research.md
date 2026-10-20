# Research: Subtareas y sprint actual en la vista Jira

## R1 — Bringing in every sub-task of the listed tickets

- **Decision**: after the main search, a **second search** with
  `parent in (K1,…,Kn) AND issuetype in subTaskIssueTypes()` over the keys of the
  top-level tickets that came back (≤ 50), with the same `FIELDS`.
  The results already in the list are dropped (dedup by key).
- **Rationale**: the `subtasks` field on the parent only brings key, summary, status,
  priority and type: **no assignee** (FR-002/scenario 1.2 needs it) or
  description (needed for "Analizar"). `parent in (…)` works on Cloud and on Data
  Center. `subTaskIssueTypes()` is needed because on Cloud `parent in (EPIC)`
  also returns an epic's stories.
- **Alternatives**: (a) `subtasks` field + `issue/{key}` per sub-task → N
  requests; (b) adding `OR parent in (…)` to the main JQL → mixes up the
  filter (the sub-tasks would have to pass "who") and the 50-item limit.
- **Limit**: paged, up to 500 sub-tasks per listing. Only real issue keys go into the JQL (`keys_jql`).
- **The second search is skipped** when there are no top-level tickets, or every one
  is itself a sub-task.

## R2 — A sub-task's epic

- **Decision**: `map_issue` keeps the `parent` that is **not** an epic as
  `JiraIssue.parent { key, summary }`. After assembling the list, a sub-task with no
  epic inherits its parent's: from the list if the parent is in it, otherwise from a
  `key in (…)` search over the parents that are missing (one request, like
  `name_epics`). `name_epics` runs afterwards so the inherited epics also get their title.
- **Rationale**: Cloud gives the sub-task's `parent` (the story), not the epic;
  on Data Center sub-tasks don't carry the epic link either. The epic lives
  only on the parent.
- **Alternative**: resolving it in the UI → impossible when the parent is not in
  the list (edge case FR-004).
- The missing parents are **not** added to the list as rows (they'd break the
  "who" filter); only their key/title is kept for the "↳ KEY title" hint.

## R3 — Filtering by the current sprint

- **Decision**: `Filter.sprint: SprintFilter { All (default), Current }`. With
  `Current`, `Filter::jql` adds `sprint in openSprints() AND sprint not in futureSprints()`
  (`openSprints()` alone also returns sprints that haven't started, found in review) alongside the "who" and
  progress conditions, keeping the saved `ORDER BY`.
- **Rationale**: `openSprints()` exists on Cloud and on Data Center (Jira
  Software) and covers every open sprint the user can see (edge case of
  parallel boards). It doesn't need the board id.
- **Without Jira Software**: Jira answers 400 "Field 'sprint' does not exist"; it
  already becomes `ApiError::unprocessable` with a readable message (`error_message`
  already has that exact test case). Turning the filter off restores the list.

## R4 — Reading the ticket's sprint

- **Decision**: discover the field through `GET field` with
  `schema.custom == "com.pyxis.greenhopper.jira:gh-sprint"` — **on Cloud and DC** —
  in the same call that finds the epic link (one `custom_fields()` that returns
  both). If the site doesn't have it, there is no sprint and nothing fails.
- **Value format**:
  - Cloud and recent DC: an array of objects `{ id, name, state, … }`, `state` ∈
    `active|closed|future`.
  - Older DC: an array of strings
    `com.atlassian.greenhopper.service.sprint.Sprint@…[id=1,…,state=ACTIVE,name=Sprint 42,…]`;
    `name=` and `state=` are pulled out with a split on `,` / `=` (the name can
    contain commas → take from `name=` up to `,startDate=` or `]`).
- **Which one is shown**: the `active` one; otherwise `future`; otherwise the last one in the
  array (the most recent `closed`).
- **Alternative**: hard-coding `customfield_10020` → it varies by site.
- **Cost**: on Cloud there's one more call (`field`) per listing. It's acceptable
  (the DC client already does it); it could be cached in `JiraClient` later.

## R5 — Where the tree is built

- **Decision**: the daemon returns a **flat list** (parents and sub-tasks) with
  `parent` filled in; the UI builds the nesting in `groupIssues`.
- **Rationale**: keeps the `GET /jira/issues` contract (only fields are added),
  the text filter and `task_status` keep working row by row, and the grouping
  is already done in the UI.

## R6 — Sample data

- Add to `fixtures/jira-search.json`: 3–4 sub-tasks (`issuetype.subtask: true`,
  `hierarchyLevel: -1`, `parent` → story), one assigned to "Ana", one whose parent
  is not in the sample; a `customfield_10020` field with sprints `active`,
  `closed` and tickets with no sprint.
- `JiraClient::Sample` emulates R1 (adds the sub-tasks of the filtered parents),
  R2 (inherits the epic) and R3 (`matches` checks for an `active` sprint).
  In sample mode the sprint field is fixed at `customfield_10020`.

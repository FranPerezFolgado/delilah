# Code Researcher Map: WP-08 US1 UI

Tasks T018–T019 (tests) and T024–T026 (implementation).

## Files

| Path | Role | Why it matters |
|------|------|---|
| `app/src/api.ts:1–250` | API client types | Add `JiraImportPreview`, `JiraImportRow`, `JiraImportRequest`, `JiraImportResult` types matching `contracts/api.md` |
| `app/src/api.ts:1500+` | API functions (after existing Jira ones) | Add `jiraImportPreview(key)` and `jiraImport(key, req)` calling GET/POST `/api/v1/jira/issues/{key}/import` |
| `app/src/api.test.ts:100–200` | API test pattern | Shows how to stub fetch, test calls to new endpoints; add test case for `jiraImportPreview` and `jiraImport` |
| `app/src/components/JiraImportDialog.tsx` | NEW dialog | Create mode: rows with key/title/status/assignee, default ticks from `selected`, project picker required; "Crear" calls `jiraImport`, shows `left_out` with reasons, retries on `unreachable` |
| `app/src/components/JiraImportDialog.test.tsx` | NEW test | Follows `NewTaskDialog.test.tsx` pattern: render → mock API → fireEvent interactions → assert render/calls; test create mode rows, default selection, toggling changes request, project required, result display, error retry |
| `app/src/components/JiraView.tsx:400–500` | "Create parent task" action | Add button near subtask listing (~L404–494) that opens dialog if `subtask_count > 0 && !is_subtask` |
| `app/src/components/JiraView.test.tsx:100+` | JiraView test | Add test: "Create parent task" shows only for ticket with `subtask_count > 0` that is not `subtask`, opens dialog |
| `app/src/i18n/en/JiraImportDialog.ts` | NEW translations | Export `EN` object: Spanish → English for all dialog strings (copied from component, follow `NewTaskDialog.ts` structure) |
| `app/src/i18n/en/JiraView.ts` | Extend translations | Add strings for "Crear tarea padre" button and dialog trigger logic |
| `app/src/i18n.ts:1–10` | i18n loader | AUTO-registers `JiraImportDialog.ts` via `import.meta.glob("./en/*.ts")`; no manual change needed |

## Patterns

**Dialog structure** (see `NewTaskDialog.tsx`): Props interface with callbacks → React hooks for state → form with fields → event handlers. Tests mock API, render, then fireEvent + assert.

**API client**: Types are interfaces matching the contract. Functions call `fetch(DAEMON_URL + path)` with body/headers. Test stubs `fetch`, checks URL and body with `path()` helper.

**i18n registration**: No Spanish file needed (Spanish text lives in components). Create `app/src/i18n/en/JiraImportDialog.ts` exporting `EN` object with entries (Spanish → English). The `i18n/en.ts` glob auto-collects it; `t("Spanish")` in component looks it up in `EN`.

**Tests**: `@vitest-environment jsdom` → `beforeEach(() => setLanguage("es"))` → render → mock API (vi.spyOn) → fireEvent interactions → waitFor → assert. Use `screen.getByText()` / `screen.getByRole()` for queries.

## Tests

- `app/src/components/JiraImportDialog.test.tsx`: T018 — rows with key/title/status/assignee; default ticks follow `selected`; toggling changes request's `add`; project picker required; result shows `left_out` with reasons; errors show retry
- `app/src/components/JiraView.test.tsx`: T019 — "Create parent task" visible only for `subtask_count > 0` and not a subtask; clicking opens the dialog

Both follow the vitest + React Testing Library style in `NewTaskDialog.test.tsx`, `JiraView.test.tsx` (mock → render → fireEvent → assert).

## Watch out

- `jiraImportPreview` and `jiraImport` functions must match the route paths exactly: `/api/v1/jira/issues/{key}/import` (GET/POST)
- Dialog must be imported and opened from `JiraView.tsx` when conditions met; state lifted to parent or passed as prop
- Test project picker: it is required when `parent.action === "create"` (per contract); disable confirm if missing
- i18n: `import.meta.glob` only picks files matching `./en/*.ts` pattern; new file must be in that folder
- `SubtaskList.tsx` and `SubtaskList.test.tsx` are used by US3 (T034, T038) so avoid changes here for WP-08

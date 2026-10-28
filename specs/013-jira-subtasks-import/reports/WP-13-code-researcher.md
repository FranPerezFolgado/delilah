# WP-13 Code Researcher — US3 UI: Adding and skipping sub-tasks

## Map for WP-13 US3 UI
Tasks: T034, T035, T037, T038 from specs/013-jira-subtasks-import/tasks.md (Phase 5: User Story 3)

### Files: where and why

**app/src/components/SubtaskList.tsx** (existing)
- L27–66: React component showing parent task's subtasks with PR state and dropped badge
- T038 adds: loading jiraImportPreview on mount (without blocking list), banner showing count of new sub-tasks (create/attach rows), quiet note for Jira states (not-connected/not-found), "Skipped (n)" section with Undo per row

**app/src/components/SubtaskList.test.tsx** (existing test file)
- T034: Tests for banner count (non-skipped create/attach), hiding at 0; list renders before preview resolves; error states (not-connected quiet, not-found/unreachable silent); "Skipped (n)" with Undo per row; "Add" button opens JiraImportDialog in add mode

**app/src/components/JiraImportDialog.tsx** (from WP-08, extended in WP-11)
- L7: mode prop `JiraImportMode = "create"` (will become "create" | "add" in T037)
- L27–57: Dialog state and load callback fetching preview + projects
- L83: needsProject logic (only applies when mode="create" and parent.action="create", per WP-11)
- T037 adds: "add" mode renders only create/attach subtask rows (filters out linked/conflict), includes skipped rows with Undo per row, per-row Skip button

**app/src/components/JiraImportDialog.test.tsx** (from WP-08, extended in WP-11)
- T035: Tests for add mode — lists only create/attach + skipped; per-row Skip goes to request.skip; skipped rows have Undo

**app/src/api.ts** (types from WP-08 + contracts/013-jira-subtasks-import/api.md)
- L374–428: Types JiraImportTask, JiraImportRow, JiraImportParent, JiraImportPreview, JiraImportRequest, JiraImportResult
- JiraImportRow has action (create|attach|linked|conflict), skipped, selected boolean
- JiraImportPreview.subtasks: array of rows; gone: array of keys no longer in Jira

**app/src/i18n/en/SubtaskList.ts** (new for T038)
- Strings: banner "{count} new Jira sub-tasks · Add", quiet note for not-connected, "Skipped ({n})", Undo button label

**app/src/i18n/en/JiraImportDialog.ts** (extended in T037 for add mode)
- L2–17: Existing strings for create mode
- T037 adds: "Skip" button label, mode-conditional copy ("Adds {n} new" for add mode)

### Patterns

**Dialog mode prop** (WP-08 model)
- `type JiraImportMode = "create" | "add"` (T037)
- T037: add mode filters subtasks: show only canTick(r) = r.action === "create" || r.action === "attach", plus skipped rows
- Create mode shows project picker (needsProject check), add mode doesn't
- Both modes: preview loaded on open, rows ticked per `selected` default, ticked set synced to request

**Preview loading without blocking** (T038 pattern)
- useEffect in SubtaskList: call api.jiraImportPreview(task.key), set state to null first, return cleanup
- List renders subtasks ?? [] (empty until preview comes) in parallel
- Banner/notes only show when preview is resolved

**Skip/Unskip rows** (T035, T037 contract)
- Each skipped row shows original selection state (was ticked, user skipped it)
- Undo button: toggles that key out of request.skip back to request.add (or back to not-skipped via unskip array per contract)
- T037: canTick filters include skipped rows when mode="add"

### Tests

**Where tests live**
- SubtaskList: `app/src/components/SubtaskList.test.tsx` (T034)
- JiraImportDialog add mode: `app/src/components/JiraImportDialog.test.tsx` (T035)
- i18n: existing vitest run includes i18n translation keys

**Test pattern**
- Render component with mock preview (linked:true with subtasks)
- Assert UI labels match contract rows
- Modal/dialog: check footer button states, form submission

### Watch out

**Parent task detail exposure**
- SubtaskList is rendered when `taskKind(task) === "parent_task"` (TaskScreen.tsx:750)
- Parent task's jira key comes from task.key (e.g., "PROJ-10") when imported
- T038 uses that key to call jiraImportPreview(task.key)
- No new field needed; the key itself is the Jira link

**Mode prop boundaries**
- T037: add mode must not show project picker (contract: project ignored when parent exists)
- T035: "Skipped (n)" only appears in add mode (create mode has skip:[] sent)
- WP-11 changed needsProject logic; T037 must respect that

**API contract alignment**
- POST request shape: `{ project?, add, skip, unskip }` (contracts/api.md)
- add mode: skip array is populated by user; unskip reverses skips (contract L48)
- left_out response in result must handle attach targets that started between preview and write

## Map for WP-11 US2 UI (T030, T032)

**Files:**
- `app/src/components/JiraImportDialog.tsx` — extend parent action rendering (lines 78–131) for `convert`/`existing` + row action rendering (lines 140–152) for `attach`/`conflict` — T032 impl
- `app/src/components/JiraImportDialog.test.tsx` — add test cases for parent line text (convert/existing/refused), attach badges, and conflict rows (unticked, disabled, with reason) — T030 already drafted
- `app/src/i18n/en/JiraImportDialog.ts` — add strings for `"becomes the parent task"`, `"adds to the parent task"`, `"already a task, will be attached"` and reason rendering
- Spanish i18n (new or under `app/src/components/` per WP-08 notes)

**Patterns:**
- Parent action is `linked.parent.action` ∈ {`"create"`, `"convert"`, `"existing"`, `"refused"`} (api.ts:399–401)
- Row action is `r.action` ∈ {`"create"`, `"attach"`, `"linked"`, `"conflict"`} (api.ts:391)
- Refused parent has `code` + `message` (already shown at line 131); `convert`/`existing` are plain objects
- `conflict` rows have `r.reason` (similar to `left_out[].reason` in results, lines 110–112)
- Row rendering mirrors result summary: `"ya es una tarea"` (WP-08 line 150) → `attach` = `"already a task, will be attached"`
- Disabled checkbox pattern: `canTick(r)` at line 22 checks `action ∈ {create, attach}` → extend to exclude `conflict`
- Conditional badges at lines 148–150: add `attach` and `conflict` cases

**Tests:**
- T030 (`JiraImportDialog.test.tsx`): 
  - Parent line renders "becomes the parent task" for `convert`, "adds to the parent task" for `existing`
  - `attach` rows (action="attach") render "already a task, will be attached"
  - `conflict` rows (action="conflict" with reason) are unticked and disabled, showing the reason
  - `refused` parent (already in WP-08) shows refusal message + disables confirm
- Existing tests continue to pass (create mode, create action rows, linked rows)

**Watch out:**
- `mode` prop is still `"create"` only; the same dialog handles all parent actions on first open
- `canTick` function (line 22) is the guard for checkbox enable/disable — extend it for US2
- Parent action rendering at line 80 sets `refused` — add branches for `convert`/`existing` nearby
- `linked.parent` is guaranteed when `linked` is truthy (api.ts:403–410) — no null check needed
- T032 must complete before T031 is testable (both edit `apply_jira_import` — one for backend, this for frontend)


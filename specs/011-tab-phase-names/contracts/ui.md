# UI contract: session names on the task screen

This extends the "Task tabs" row of `specs/007-ui-v2/contracts/ui.md`.

| Session | Tab label (ES / EN) | Sessions list row | Tooltip |
|---|---|---|---|
| step `plan` | «Spec y plan» / «Spec and plan» | same as tab | stored label |
| step `implement` | «Implementación» / «Implement» | same as tab | stored label |
| step `review` | «Revisión» / «Review» | same as tab | stored label |
| primary Claude, no step | «Claude principal» / «Main Claude» | same as tab | stored label |
| any other session | its stored label | same as tab | stored label |

- Tabs keep `role="tab"`. A closable tab's close button is named «Cerrar <label>» /
  «Close <label>».
- A primary tab has no close button, whatever its name.
- E2E selectors that look for «Claude principal» / «Main Claude» still match tasks
  that are not in saving mode.

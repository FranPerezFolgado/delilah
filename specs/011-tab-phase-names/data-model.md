# Data model: Session tabs named after their phase

No schema, API or stored-data change. The feature reads fields the UI already receives.

## Session (app/src/api.ts, unchanged)

| Field | Use here |
|---|---|
| `label` | Shown when there is no step and the session is not the primary Claude; always the tooltip |
| `primary` | With no step, a primary Claude reads «Claude principal» / «Main Claude» |
| `kind` | Only a `claude` primary gets «Claude principal» |
| `step` | `plan` / `implement` / `review` → «Spec y plan» / «Implementación» / «Revisión» (EN «Spec and plan» / «Implement» / «Review») |

## Derived: displayed session name

```
sessionName(s) =
  s.step                          → stepLabel(s.step)
  s.primary && s.kind == claude   → t("Claude principal")
  otherwise                       → s.label
```

No state transitions: the name changes only when the session's fields change, for
example when a handoff makes a new session primary.

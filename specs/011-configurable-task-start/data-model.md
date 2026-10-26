# Data model: Configurable workflow commands per project

## ProjectSettings.workflow (core, `crates/delilah-core/src/project.rs` + new `workflow.rs`)

```rust
#[serde(tag = "kind", rename_all = "kebab-case")]
pub enum ProjectWorkflow {
    Single { start: Start },
    Phased { plan: String, implement: String, review: String },
}

#[serde(tag = "type", rename_all = "kebab-case")]
pub enum Start {
    Builtin,
    Message { template: String },
    Nothing,
}
```

- `ProjectSettings.workflow: Option<ProjectWorkflow>` (`#[serde(default)]`); `None` ⇒
  `Single { start: Builtin }`. Helper `ProjectSettings::workflow() -> ProjectWorkflow`.
- JSON examples:
  - `{"kind":"single","start":{"type":"builtin"}}`
  - `{"kind":"single","start":{"type":"message","template":"/fix {ticket}"}}`
  - `{"kind":"phased","plan":"/feature {task}","implement":"/orchestrate {spec_dir}","review":"/review …"}`
- Validation (`settings::validate` calls `workflow::validate`):

| Field | Rule | Code |
|---|---|---|
| `workflow.start.template` | not blank | `workflow.blank` |
| `workflow.plan` / `.implement` / `.review` | not blank | `workflow.blank` |
| any template | only `{ticket}` `{title}` `{task}` (+ `{spec_dir}` in `implement`) | `workflow.unknown_placeholder` / `workflow.placeholder_not_allowed` |
| any template | `{`/`}` balanced, `{{` `}}` literal | `workflow.unbalanced_brace` |

## Template rendering (core `workflow.rs`)

```rust
pub struct Vars<'a> { ticket: &'a str, title: &'a str, task: &'a str, spec_dir: Option<&'a str> }
pub fn render(template: &str, vars: &Vars) -> String;          // assumes validated
pub fn validate(field: &str, template: &str, allow_spec_dir: bool) -> Vec<FieldError>;
pub enum TaskWorkflow { Project, Prompt, None }                 // parse("feature") = Project
pub fn is_phased(task: TaskWorkflow, project: &ProjectSettings) -> bool;
pub fn resolve(req, task, project, builtin_text: &str) -> ResolvedStart;
// feature_preset(Lang) and builtin_start(Lang) live in the daemon (lang.rs): core has no Lang.
```

`task` = ticket for a Jira key, title for a free key (`is_free_key`).

## Task (core `task.rs`, table `tasks`)

| Field | Type | Change |
|---|---|---|
| `workflow` | `String` | values now `project` \| `prompt` \| `none`; migration maps `feature`→`project`, anything else→`prompt` |
| `start_message` | `Option<String>` | **new** column; the task's own template when `workflow = prompt` |

Start resolution (`run.rs`):

```
req.workflow/prompt ─┬─ prompt given, workflow absent → prompt(prompt)
                     ├─ "prompt" + prompt → prompt(prompt)   ("prompt" without prompt → 400)
                     ├─ "none" → nothing
                     ├─ "project"/"feature" → follow the project's workflow (stored as `project`)
                     └─ absent → task's stored choice
task stored        ─┬─ prompt + start_message → prompt(start_message)
                     ├─ prompt without message (legacy) → nothing
                     ├─ none → nothing
                     └─ project → project's workflow
project workflow   ─┬─ single(builtin) → builtin_start      (not phased)
                     ├─ single(message) → render(template)  (not phased)
                     ├─ single(nothing) → no prompt         (not phased)
                     └─ phased → render(plan)               (phased)
no project         → builtin_start
```

The resolved choice is stored on the task (`workflow` + `start_message`) so a backlog start and
a later relaunch see the same thing. Not phased ⇒ `saving_mode=false`, `autopilot=None`.

## Skill (read-only, daemon `skills.rs`)

```rust
pub struct Skill { name: String, description: String, source: SkillSource, plugin: Option<String> }
pub enum SkillSource { User, Project, Plugin }
```

Not stored. Sorted by source (project, user, plugin), then name.

## Migration `0022_workflow_commands.sql`

```sql
UPDATE projects SET settings = json_set(settings, '$.workflow', json('{"kind":"phased",...}'))
  WHERE json_extract(settings, '$.workflow') IS NULL;
ALTER TABLE tasks ADD COLUMN start_message TEXT;
UPDATE tasks SET workflow = 'project' WHERE workflow = 'feature';
UPDATE tasks SET workflow = 'prompt'  WHERE workflow NOT IN ('project', 'prompt', 'none');
```

Tests: a migration test on a DB seeded with pre-upgrade rows (`feature`, `prompt`, `bugfix`
tasks; projects with `{}` and with other settings), and a test that the SQL's preset JSON equals
`feature_preset(Lang::Es)` / `(Lang::En)`.

The preset's review text is today's `/review …` message in the language of the
`pref:language` setting (`CASE` on `settings`: `"en"` → English, anything else, including
unset, → Spanish). Only the agent's instruction language depends on it, and the user can edit
it in the settings afterwards. A Rust-side backfill with `app.lang()` was considered and ruled
out: it cannot tell a project registered before the upgrade from one registered after without an
extra marker.

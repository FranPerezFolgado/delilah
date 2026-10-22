# CLI contract: Per-project configuration

```text
delilah project show <name>                    # the effective settings, defaults marked
delilah project set <name> workspace checkout|worktree
delilah project set <name> branch 'feature/{ticket}-{slug}'
delilah project set <name> setup 'npm ci'
delilah project set <name> shared add <path> [--copy|--link]
delilah project set <name> shared rm <path>
delilah project set <name> submodules always|never add|rm <path>
delilah project unset <name> <setting>         # back to the default
delilah new <KEY> --title T --project P [--checkout | --worktree]   # neither: follows the project's workspace setting
delilah run <KEY> [--checkout | --worktree]    # neither: the backlog task's choice, else the project's setting
delilah setup retry <KEY>
delilah setup skip <KEY>
delilah setup cancel <KEY>
```

`set`/`unset` read the current settings, change one field and `PUT` them, so validation
is the daemon's (FR-005). Errors print each `field: message` and exit 1.
`delilah project ls` keeps its output; `show` is the detailed view.

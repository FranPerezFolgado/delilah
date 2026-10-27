## WP-05 — done
Tasks:    T015 ✓
Files:    app/src/api.ts — TaskKind, Task.kind?/parent_task?/subtask_progress?, taskKind(), SubtaskProgress, SubtaskSummary, ProjectSettings.parent_task_merge (+ default "merge"), NewBacklogTask kind/parent_task, PullRequest base_branch/expected_base/method, BaseStatus.subtask_commits, api.subtasks/makeParentTask/attach/pushBranch/markReady, mergePr confirm_live_subtasks, openPr empty_commit
          app/src/api.test.ts — new tests
          app/src/components/ProjectSettings.test.tsx — fixture gained parent_task_merge (tsc)
Tests:    "parent tasks and subtasks" (3 tests) in api.test.ts; tsc --noEmit clean, vitest api.test.ts + ProjectSettings.test.tsx pass (38)
Notes:    - The contract's `Progress` clashes with an existing `Progress` (agent turn progress) in api.ts; named `SubtaskProgress`.
          - Task.kind is optional in the type, read via taskKind() (absent => "normal"), to tolerate older daemons.
          - BaseStatus.base already existed (kept as string | null).
          - app/node_modules is absent in this worktree; I symlinked the main checkout's temporarily to run checks, then removed it.

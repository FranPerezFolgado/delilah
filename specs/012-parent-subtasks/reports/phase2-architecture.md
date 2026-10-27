# Phase 2 architecture review (0ecfef3..HEAD)

Verdict: no CRITICAL or HIGH. Layering matches plan.md: pure rules in core, base override in git, store-only integration.rs.

[MEDIUM] crates/delilah-daemon/src/integration.rs:74-101
Problem: subtask_facts runs raw SQL on `pull_requests` and `workspaces` through `store.pool`, bypassing Store/store_prs; schema knowledge of those tables is now duplicated in a second module.
Fix: add `Store::subtask_prs(parent)` and `Store::subtask_workspace_keys(parent)` next to `subtasks_of` and call them; integration.rs then holds only the rules.

[MEDIUM] crates/delilah-daemon/src/integration.rs:145-149
Problem: preferred_method converts MergePreference to MergeMethod through a serde_json round trip because core does not export MergePreference; fragile, silently yields None if the names diverge.
Fix: export MergePreference from delilah-core and add an explicit `From<MergePreference> for MergeMethod` (or match) in the daemon.

[MEDIUM] crates/delilah-git/src/workspace.rs:672 vs crates/delilah-daemon/src/base_update.rs:179
Problem: merge_refs (abort + reset --hard) is a second merge/rollback implementation next to base_update::merge_all (abort only, own `run` helper, different error reasons); the plan allowed either, but the two can drift.
Fix: when phase 5 wires base_update to base_for, have merge_all call delilah_git::merge_refs per repo, or delete merge_refs if unused.

[LOW] crates/delilah-daemon/src/integration.rs:57-61
Problem: base_for uses `expect("checked above")` after a core check that only returns a bool-derived Refusal.
Fix: `let ws = store.workspace(parent).await?.ok_or(Refusal::ParentBranchMissing-equivalent)` and keep check_start_subtask for the rule, or match on the Option.

[LOW] crates/delilah-git/src/base.rs
Problem: prepare_base_on adds a Result-returning twin of prepare_base; fine, but prepare_base's `.expect("the default base never fails")` relies on an invariant the types do not express.
Fix: optional: split the default path into an infallible inner fn.

[LOW] integration.rs is only used by its own tests so far (private mod, no callers); expected until phases 4-5, watch for dead-code warnings.

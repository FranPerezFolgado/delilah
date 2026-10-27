## WP-03 — done (one OUT OF SCOPE item)
Tasks:    T008 ✓, T009 ✓, T010 ✓
Files:    crates/delilah-git/src/base.rs — resolve_base takes Option<&str>; new prepare_base_on(dir,label,base,opts,deadline)->Result; prepare_base kept as a wrapper (base None); 3 tests
          crates/delilah-git/src/command.rs — GitError::BaseMissing { branch } (not in allowed list; required by T009)
          crates/delilah-git/src/workspace.rs — WorkspaceSpec.base; create_workspace/init_submodules/use_checkout use prepare_base_on; commit_empty, merge_refs; 3 tests
          WorkspaceSpec literals + `base: None`: git/tests/common/mod.rs, git/tests/workspace.rs, daemon/src/{actions.rs,run.rs,pr_loop/merge.rs}
Tests:    base: an_override_resolves_that_origin_branch_not_the_default, a_missing_override_is_base_missing_never_the_default, a_failed_fetch_keeps_an_existing_override_with_a_warning; workspace: commit_empty_adds_a_commit_without_changes, merge_refs_merges_in_order, merge_refs_undoes_everything_on_conflict. `cargo test -p delilah-git` passes (all suites). `cargo check --workspace` fails only on delilah-daemon: Task literal missing `kind`/`parent_task` (other implementer's core change).
Notes:    OUT OF SCOPE: crates/delilah-git/src/lib.rs must re-export `prepare_base_on`, `commit_empty`, `merge_refs` for the daemon. Base override applies to the parent and to every submodule. Ran cargo fmt on git+daemon crates.

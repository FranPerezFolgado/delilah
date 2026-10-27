# Phase 2 security review (crates/delilah-git, 0ecfef3..HEAD)

Verdict: no CRITICAL/HIGH. Two MEDIUM, one LOW. merge_refs, commit_empty and the base override have no callers outside delilah-git yet, so the exposure is latent until the daemon wires them in.

[MEDIUM] crates/delilah-git/src/workspace.rs (merge_refs)
Problem: `git merge --no-edit <r>` takes r with no `--` and no check, so a ref starting with `-` (for example a parent branch named `--strategy-option=...` or `-X theirs`) is read as an option. Git does allow branch names that start with `-` on the remote side.
Fix: reject refs that start with `-` (or check them with `git check-ref-format --branch`), and/or resolve each ref to a SHA via `rev-parse --verify --end-of-options` and merge the SHA.

[MEDIUM] crates/delilah-git/src/base.rs (resolve_base/verify) and workspace.rs (start use)
Problem: `base` is not validated. It is interpolated into `refs/remotes/origin/{base}^{commit}`, so revision syntax (`main~3`, `main^`, `x@{...}`, `:/text`) resolves and the start point becomes something other than a branch tip. The resolved refname is then passed to `git checkout -b` and `worktree add`. The "origin/" prefix prevents a leading `-`, so there is no option injection there.
Fix: validate `base` at the entry point with `git check-ref-format --branch`, or reject `~ ^ : @{ .. ` and whitespace, and a leading `-`.

[LOW] crates/delilah-git/src/workspace.rs (merge_refs cleanup)
Problem: after a failed merge, `reset --hard <start>` discards any uncommitted work in dir, although the doc comment says the branch is "left as it was". Also, the `merge --abort` and `reset` errors are swallowed.
Fix: require a clean tree before merging, or use `merge --abort` only.

commit_empty: message is passed as the value of `-m` (a separate arg, no shell), so there is no injection. The `.expect("the default base never fails")` in prepare_base is safe because the default path never returns Err.

# Final security review, spec 015 (multiple PRs)

Verdict: no CRITICAL or HIGH findings. Two LOW notes.

Scope: pr_loop/capture.rs, repo_of / link-by-URL, pr_api/open.rs, migration 0027.

Checked and fine:
- capture.rs: only PostToolUse + Bash + a simple command that starts with `gh pr create` (after VAR= prefixes). Only whole `https://host/owner/name/pull/N` words are taken (tests cover http, userinfo, trailing path, issues). Max 4 URLs per hook. Host, owner and name are charset-validated by parse_pr_url, so no shell or argument injection. gh is run with an argument vector (no shell), and `pr_merge`/`pr_ready` use `--`.
- Spoofing: the agent can only link a PR that `gh` can read with the user's own credentials, and the data comes from the API, not from the stdout text. Linking is not merging; merge and ready are user-driven.
- open.rs: `repo` is matched against the label list from `pr_repos` (an allowlist), so there is no path traversal; unknown values give 400 (tested). The push/create directory comes from the workspace, not from the request.
- Migration 0027: explicit-id copy, transactional table rebuild, no dynamic SQL. Notices whose PR is gone are resolved, not deleted.

[LOW] crates/delilah-daemon/src/pr_loop/capture.rs:90
Problem: Agent-printed URLs can name any host, so the daemon makes `gh api graphql --hostname <host>` calls to an agent-chosen host (a blind request). gh sends a token only for hosts it is authenticated to, so the impact is limited to what the agent can already do with curl.
Fix: Optional. Restrict to hosts known to `gh auth status`, or to the hosts of the project's repos, before calling pr_view_url.

[LOW] crates/delilah-daemon/src/pr_loop/capture.rs:19
Problem: The command check is a heuristic. For example `bash -c "gh pr create"` is not matched, and `gh pr create` in a later chained part would match even if an earlier part failed. Both only cause a missed link or a link that is read through the API anyway, so no escalation.
Fix: None needed.

## WP-03 — done
Tasks:    T005 ✓, T010 ✓
Files:    crates/delilah-daemon/src/pr_types.rs — Snapshot.head_branch (serde default) + old-JSON assertion
          crates/delilah-daemon/src/gh/graphql.rs — headRefName in query, RawPr, Snapshot; assertion in test; GRAPHQL fixture
          crates/delilah-daemon/src/gh/fake.rs — headRefName "feat/x" in the 3 fake answers
          (trivial `head_branch: None` literals, outside list) crates/delilah-daemon/src/autofix/mod.rs, crates/delilah-daemon/src/prs.rs
Tests:    extended an_old_stored_snapshot_still_parses and graphql_answer_becomes_a_snapshot: pass.
          cargo test -p delilah-daemon: 1125 pass, 5 fail (varying set between runs: run::tests, autopilot::finish, store_prs migration_0027 — latter is WP-02's area; others look environmental/flaky, none touch Snapshot).
Notes:    gh/mod.rs needed no change (parsing lives in graphql.rs). head_ref_name is serde(default) so answers lacking it parse to None.

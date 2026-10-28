## WP-09 — done
Tasks:    T027 ✓
Files:    crates/delilah-cli/src/main.rs — `delilah jira import <KEY> [--project P] [--defaults | --add K,..]` (JiraCommand::Import(JiraImportArgs), preview/outcome types, jira_import, preview_lines, outcome_lines, import_body, query_value)
Tests:    parses_jira_import, import_body_takes_the_preselected_or_the_given_keys, prints_the_preview_and_the_result; `cargo test -p delilah-cli` 48 pass, clippy clean
Notes:    Always GETs the preview first (with ?project=); no write flag prints it only. --defaults uses rows with selected=true. Not-linked reasons become an error (exit 1); 409/400 refusals already print the daemon `message` and exit 1 via existing Api error handling. JiraImportArgs is a separate Args struct so WP-14 can add --skip/--unskip (the POST body is built in import_body; `writes()` must also count them).

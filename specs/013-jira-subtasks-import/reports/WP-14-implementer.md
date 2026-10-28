## WP-14 — done
Tasks:    T039 ✓
Files:    crates/delilah-cli/src/main.rs — --skip/--unskip args (write flags), sent in POST body when non-empty, outcome prints skipped/unskipped
Tests:    skip_and_unskip_are_write_flags_sent_in_the_body; cargo test -p delilah-cli
Notes:    Preview filtering to new+skipped rows on parent tasks is daemon-side; CLI prints what it gets.

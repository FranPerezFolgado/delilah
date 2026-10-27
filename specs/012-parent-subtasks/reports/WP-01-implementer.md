## WP-01 — done
Tasks:    T001 ✓
Files:    crates/delilah-core/src/hierarchy.rs — new, doc comment only
          crates/delilah-core/src/lib.rs — `pub mod hierarchy;`
          crates/delilah-daemon/src/integration.rs, api_hierarchy.rs — new, doc comment only
          crates/delilah-daemon/src/lib.rs — `mod api_hierarchy;`, `mod integration;`
Tests:    none (skeletons); `cargo check -p delilah-core -p delilah-daemon` passes
Notes:    daemon modules are private (`mod`), matching most siblings; make pub later if needed.

# Work Package Map — WP-01: Module skeletons

## Files

- **crates/delilah-core/src/lib.rs:8** — `pub mod hierarchy;` declaration (after `pub mod branch;`)
- **crates/delilah-core/src/hierarchy.rs** — new, empty module for core parent/subtask types
- **crates/delilah-daemon/src/lib.rs:3** — `mod integration;` declaration (private, integration rules; after `mod actions;`)
- **crates/delilah-daemon/src/integration.rs** — new, empty module for integration logic
- **crates/delilah-daemon/src/lib.rs:6** — `pub mod api_hierarchy;` declaration (public API; after `pub mod api;`)
- **crates/delilah-daemon/src/api_hierarchy.rs** — new, empty module for hierarchy endpoints

## Pattern

**Module declarations** live in lib.rs (not main.rs) in both crates. Add declarations as:
- `pub mod X;` for public modules (exported API or cross-crate types)
- `mod X;` for private internal modules

Order: private modules first, then public. Alphabetically within each group.

**Module files** are siblings in src/ (no subdirectories). Declare once, define once.

**Re-exports** (pub use) go below declarations in lib.rs only if the module re-exports its internals; WP-01 does not require any.

## Acceptance

- All six module declarations added to lib.rs
- All three .rs files exist (empty is fine: `// …` or just empty)
- `cargo test -p delilah-core` and `cargo test -p delilah-daemon` pass
- Workspace compiles with no warnings or errors

## Watch out

- delilah-daemon/src/main.rs is not a module root; it imports from lib.rs
- lang.rs and inbox.rs are already declared in lib.rs (lines 23, 25) — WP-01 does not touch them
- integration.rs is NOT yet declared; WP-01 adds it (line 3, private)

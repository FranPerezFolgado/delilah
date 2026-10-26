# Contributing to Delilah

Thanks for helping. This page is the short version; the practical detail (running,
testing, conventions, releasing, gotchas) is in [docs/development.md](docs/development.md),
and how the pieces fit together is in [docs/architecture.md](docs/architecture.md).

## Prerequisites

- macOS on Apple Silicon, with the Xcode Command Line Tools (`xcode-select --install`).
- Rust (stable) and Node 22 or newer.
- [Claude Code](https://docs.claude.com/en/docs/claude-code) installed and logged in
  (the only agent adapter for now).

## Fixtures

Tests and previews use throwaway repos instead of real ones. Generate them with
`scripts/make-fixture-parent.sh`. Please do not add code or data copied from private
repositories.

## Trying a change: use a preview

Run `scripts/preview.sh` from your checkout (or `--fresh` for an empty state). It builds
the daemon and UI, picks a free slot and prints the URLs (UI `http://localhost:150N`,
API `http://127.0.0.1:750N`), with its own state under `fixtures/preview-home-N`.

Never use the installed app's daemon on port 7420 to try a change, and never restart or
`pkill` a `delilahd`: that daemon runs your live agent sessions. Previews exist so you
never have to. Details in [docs/development.md](docs/development.md).

## Checks

| Area | Where | Check |
|---|---|---|
| Rust workspace | `crates/*` | `cargo test -p <crate>`; everything: `cargo test --workspace` |
| Desktop shell | `app/src-tauri` (outside the workspace) | `cargo test --manifest-path app/src-tauri/Cargo.toml` |
| Web UI | `app/` (React + TS, Vite) | `cd app && npx tsc --noEmit && npx vitest related --run <files>`; everything: `npx vitest run` |

The daemon's HTTP API (`crates/delilah-daemon/src/api.rs`) and its client types
(`app/src/api.ts`) are a contract: change both together. Never edit a shipped SQL
migration; add a new one.

## Commits

Use [Conventional Commits](https://www.conventionalcommits.org/): `feat:`, `fix:`,
`docs:`, `chore:`, `refactor:`, `test:`, with an optional scope, for example
`fix(daemon): ...`. The changelog is generated from them.

## Versioning

Don't bump the version in your PR: several changes ship in one release, and the maintainer
bumps when releasing, as its own commit `chore: bump version to X.Y.Z`. The level (semver)
comes from what the release contains:

- **major**: breaks something in use (API or CLI changes old clients can't handle, data
  that needs manual migration, removed features).
- **minor**: a new user-facing feature, backwards compatible.
- **patch**: fixes, docs, internal refactors with no new behaviour.

`scripts/bump-version.sh <level> --commit` updates every place that carries the version plus
the lockfiles, and regenerates [CHANGELOG.md](CHANGELOG.md) with
[git-cliff](https://git-cliff.org/) (`brew install git-cliff`). Bumping, tagging and
publishing are the maintainer's job (see the releasing section of
[docs/development.md](docs/development.md#releasing)).

## Pull requests

1. Branch from `main`.
2. Make the change with its tests; update the docs and README if behaviour changed.
3. Open a PR and fill in the [template](.github/pull_request_template.md). Say whether
   it breaks anything already in use. Write the title as a
   [Conventional Commit](https://www.conventionalcommits.org/) (`feat: …`, `fix: …`):
   PRs are squash-merged, so the title becomes the commit on `main` and the changelog line.
4. CI must be green. A maintainer reviews and squash-merges.

## Spec Kit (optional)

Larger features are planned with Spec Kit under `specs/` (see
[specs/README.md](specs/README.md)). You do not need it for a small fix.

## Security

Report vulnerabilities privately, as described in [SECURITY.md](SECURITY.md).

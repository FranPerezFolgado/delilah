# Contracts: App auto-update

The daemon's HTTP API does not change (`api.rs` ↔ `api.ts` untouched). The new interfaces are listed below.

## 1. Manifest: `latest.json` (release → installed apps)

The URL is fixed and baked into `tauri.conf.json`:
`https://github.com/FranPerezFolgado/delilah/releases/latest/download/latest.json`

```json
{
  "version": "0.5.0",
  "notes": "## Novedades\n- …\n\n## Correcciones\n- …",
  "pub_date": "2026-10-24T10:00:00Z",
  "platforms": {
    "darwin-aarch64": {
      "signature": "<contents of Delilah.app.tar.gz.sig>",
      "url": "https://github.com/FranPerezFolgado/delilah/releases/download/v0.5.0/Delilah.app.tar.gz"
    }
  }
}
```

The format is owned by `tauri-plugin-updater` 2.x. Adding fields is safe; renaming fields breaks every installed app, so it never happens.

## 2. Shell ↔ UI

| Direction | Name | Payload | Meaning |
|---|---|---|---|
| shell → UI | event `check-updates` | none | menu "Buscar actualizaciones…" / "Check for updates…" was chosen |
| UI → shell | plugin `updater:default` (`check`, `downloadAndInstall`) | — | via `@tauri-apps/plugin-updater` |
| UI → shell | plugin `process:allow-restart` (`relaunch`) | — | via `@tauri-apps/plugin-process` |

The capability file `app/src-tauri/capabilities/default.json` adds `updater:default` and `process:allow-restart`.

`platform.ts` additions, which keep the other-shell boundary:

```ts
export const canUpdate: boolean;            // isDesktop && !import.meta.env.DEV
export function appVersion(): Promise<string | null>;
export interface FoundUpdate { version: string; notes: string;
  install(onProgress: (received: number, total: number | null) => void): Promise<void>; }
export function checkForUpdate(): Promise<FoundUpdate | null>;  // throws on network/manifest error
export function relaunch(): Promise<void>;
export function onCheckUpdatesMenu(cb: () => void): Promise<() => void>; // unlisten
```

## 3. Release command (maintainer CLI)

```
scripts/release.sh X.Y.Z [--yes] [--notes-file PATH]
```

- **Preconditions** (any failure exits non-zero before building, with one line per problem): a clean tree on `main`; tag `vX.Y.Z` at `HEAD`; the version `X.Y.Z` in `Cargo.toml`, `app/src-tauri/Cargo.toml`, `app/package.json` and `app/src-tauri/tauri.conf.json`; `~/.tauri/delilah.key` exists (override with `DELILAH_UPDATER_KEY`); the Keychain item `delilah-updater` is readable; the active `gh` account is `FranPerezFolgado`; the repo is `PUBLIC`; no *published* release `vX.Y.Z` exists.
- **Steps**:
  1. Generate the notes, then open `$EDITOR` unless `--yes` or `--notes-file` is given.
  2. Build with `npm run dist` and the signing env.
  3. Write `latest.json`.
  4. Create or reuse a draft release.
  5. Upload the dmg, tar.gz and sig, then `latest.json`.
  6. Publish the release.
- **Exit**: 0 when published. Non-zero leaves at most a draft, so the command can be rerun.
- **Helpers** (pure, `node --test`):
  - `scripts/release/versions.mjs`: `readVersions(root) → {file: version}` and `mismatches(versions, expected) → string[]`.
  - `scripts/release/notes.mjs`: `releaseNotes(subjects: string[]) → string`.
  - `scripts/release/manifest.mjs`: `manifest({version, notes, pubDate, signature, repo}) → object`.

## 4. Daemon start-up files

`$DELILAH_HOME/version` and `$DELILAH_HOME/backups/delilah-<prev>.db`; see [data-model.md](../data-model.md). Nothing else reads them. They are documented in `docs/architecture.md` (data section).

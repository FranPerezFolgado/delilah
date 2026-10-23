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

## 3. Release pipeline and dry run

**Trigger**: run `git tag [-a -m "<notes>"] vX.Y.Z && git push origin vX.Y.Z` (X.Y.Z must be semver). This starts `.github/workflows/release.yml`.

**Repository secrets** (set once with `gh secret set`): `TAURI_SIGNING_PRIVATE_KEY` (key file contents) and `TAURI_SIGNING_PRIVATE_KEY_PASSWORD`. `GITHUB_TOKEN` is the workflow's own token, with `contents: write`.

**Result**: release `vX.Y.Z`, published and marked latest, with the assets `Delilah_X.Y.Z_aarch64.dmg`, `Delilah.app.tar.gz`, `Delilah.app.tar.gz.sig` and `latest.json`. The body is the notes. Any failure leaves at most a draft, and re-running the job for the tag is safe.

**Local dry run**: `scripts/release.sh X.Y.Z --dry-run` performs the same checks plus a clean tree and a `PUBLIC` repo, then the signed build and a local `latest.json`. It never uploads. Without `--dry-run` it exits 2 and prints the tag commands.

**Helpers** (pure, tested with `node --test`):
- `scripts/release/versions.mjs`: `readVersions(root) → {file: version}` and `mismatches(versions, expected) → string[]`.
- `scripts/release/notes.mjs`: `releaseNotes(subjects: string[]) → string` and `pickNotes(tagMessage, subjects) → string`.
- `scripts/release/manifest.mjs`: `manifest({version, notes, pubDate, signature, repo}) → object` (used by the dry run).
- `scripts/release/check.mjs`: a CLI entry `node scripts/release/check.mjs X.Y.Z` that exits non-zero with one line per problem.

## 4. Daemon start-up files

`$DELILAH_HOME/version` and `$DELILAH_HOME/backups/delilah-<prev>.db`; see [data-model.md](../data-model.md). Nothing else reads them. They are documented in `docs/architecture.md` (data section).

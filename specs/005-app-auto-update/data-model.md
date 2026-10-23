# Data model: App auto-update

No database schema change and no migration. The state lives in three places: published files, the UI's memory, and two files under `$DELILAH_HOME`.

## Update manifest (`latest.json`, published)

| Field | Type | Rule |
|---|---|---|
| `version` | semver string, no `v` | equals the tag without `v` and the four version files |
| `notes` | Markdown | from `scripts/release/notes.mjs`, edited by the maintainer |
| `pub_date` | RFC 3339 | time of publishing |
| `platforms["darwin-aarch64"].signature` | string | contents of `Delilah.app.tar.gz.sig` |
| `platforms["darwin-aarch64"].url` | HTTPS URL | `…/releases/download/vX.Y.Z/Delilah.app.tar.gz` |

## Release (GitHub Release `vX.Y.Z`)

Assets: `Delilah_X.Y.Z_aarch64.dmg`, `Delilah.app.tar.gz`, `Delilah.app.tar.gz.sig`, `latest.json`. The body is the same notes. Its state goes draft → published, and only a published release is seen by `releases/latest`.

## Update state (UI, in memory, `app/src/updates.ts`)

```ts
type UpdateState =
  | { kind: "idle" }                                   // nothing to show
  | { kind: "checking"; manual: boolean }
  | { kind: "upToDate"; manual: true }                 // shown only after a manual check
  | { kind: "available"; version: string; notes: string }
  | { kind: "confirmSessions"; version: string; notes: string; running: number }
  | { kind: "waiting"; version: string; notes: string; running: number }
  | { kind: "downloading"; version: string; received: number; total: number | null }
  | { kind: "installing"; version: string }            // then relaunch
  | { kind: "error"; reason: string; manual: boolean; version?: string };

interface UpdateMeta {
  installed: string;          // app version (getVersion())
  lastCheck: Date | null;
  lastResult: "upToDate" | "available" | "error" | null;
  dismissed: string | null;   // version hidden with "Más tarde" until next launch/manual check
}
```

Transitions (events → next state):

| From | Event | To |
|---|---|---|
| any idle-like | `check(manual)` | `checking` |
| `checking` | found v (v ≠ dismissed, or manual) | `available` |
| `checking` | found v == dismissed, automatic | `idle` |
| `checking` | none | manual ? `upToDate` : `idle` |
| `checking` | failure | manual ? `error` : `idle` (meta.lastResult = error) |
| `available` | `later` | `idle`, dismissed = v |
| `available` | `update`, running = 0 | `downloading` |
| `available` | `update`, running > 0 | `confirmSessions` |
| `confirmSessions` | `now` | `downloading` |
| `confirmSessions` | `whenIdle` | `waiting` |
| `confirmSessions` / `waiting` | `cancel` | `available` |
| `waiting` | `sessions(n>0)` | `waiting` (running = n) |
| `waiting` | `sessions(0)` | `downloading` |
| `waiting` | `now` | `downloading` |
| `downloading` | `progress` | `downloading` |
| `downloading` | done | `installing` → relaunch |
| `downloading` / `installing` | failure | `error` (manual: true, version kept, "Reintentar" → `downloading`) |
| `upToDate` / `error` | `close` | `idle` |

While the state is `downloading`, `waiting` or `installing`, an automatic check is skipped (spec edge case "newer release appears while one is in progress").

## Upgrade marker and database copies (`$DELILAH_HOME`, daemon)

| Path | Content | Written |
|---|---|---|
| `$DELILAH_HOME/version` | the version that last opened the DB successfully, e.g. `0.5.0\n` | after `Store::open` succeeds |
| `$DELILAH_HOME/backups/delilah-<prev>.db` | `VACUUM INTO` copy before the first open on a new version; `<prev>` = marker or `unknown` | before `Store::open`, only if the marker differs and the DB exists, never overwritten |

Retention: the 3 newest `delilah-*.db` copies by mtime are kept, and older ones are deleted.

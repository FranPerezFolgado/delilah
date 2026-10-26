# Contract: release assets

Every published release `vX.Y.Z` (from `.github/workflows/release.yml`) has these assets.

| Asset | Producer | Consumers |
|---|---|---|
| `Delilah_X.Y.Z_aarch64.dmg` | tauri-action | Users downloading by hand |
| `Delilah_aarch64.app.tar.gz` | tauri-action (stable name) | Tauri updater (`latest.json`), `scripts/install.sh` |
| `Delilah_aarch64.app.tar.gz.sig` | tauri-action (updater key) | Tauri updater |
| `latest.json` | `scripts/release/manifest.mjs` | Tauri updater |
| `SHA256SUMS` (**new**) | release.yml "Checksums" step | `scripts/install.sh`, users (`shasum -a 256 -c`) |

Asset names other than `SHA256SUMS` are not changed by this feature.

## `SHA256SUMS`

- Format: `shasum -a 256` output, one line per asset (`<64 hex>  <name>`), sorted by name.
  It covers every other asset of the release, as downloaded from the draft release (a
  `SHA256SUMS` left by an earlier run of the same tag is dropped before hashing).
- Uploaded before the release is published. A release is never published without it.

## Build provenance

- `actions/attest-build-provenance@v3` runs over the same downloaded files
  (`$RUNNER_TEMP/dist/*`) and over `SHA256SUMS` before the release is published. The
  checksums only prove a download is intact; the attestation proves the files (and the
  checksums) were built by this repository's workflow.
- Verify: `gh attestation verify Delilah_aarch64.app.tar.gz --repo FranPerezFolgado/delilah`
  (or `SHA256SUMS`).
- Workflow permissions: `contents: write`, `id-token: write`, `attestations: write`.

## Unchanged

The ad-hoc code signature of the bundle, the updater signature, the `latest.json` format and
the endpoint in `app/src-tauri/tauri.conf.json`.

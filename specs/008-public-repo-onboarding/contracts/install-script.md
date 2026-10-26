# Contract: `scripts/install.sh`

## Invocation

```bash
curl -fsSL https://raw.githubusercontent.com/FranPerezFolgado/delilah/main/scripts/install.sh | bash
# or, from a clone / downloaded file
bash scripts/install.sh [--yes]
```

| Option | Meaning |
|---|---|
| `--yes` | Don't ask before replacing an existing `/Applications/Delilah.app`. Piped runs (`curl … \| bash`, no TTY) behave as `--yes`. |
| `-h`, `--help` | Usage and exit 0. |

Always installs the latest release (`https://github.com/FranPerezFolgado/delilah/releases/latest/download/`).

Environment (`DELILAH_INSTALL_DIR` is the one the README and the exit-8 message suggest; the
others are for `scripts/test-install.sh`):

| Variable | Default | Use |
|---|---|---|
| `DELILAH_BASE_URL` | the latest-release URL | Full base URL of the assets (tests use `file://…`). Only `https://`, `file://` or `http` on localhost (else exit 1), with a warning; redirects stay https. |
| `DELILAH_INSTALL_DIR` | `/Applications` | Install target (tests use a temp dir). |
| `DELILAH_ARCH` | `$(uname -m)` | Tests only: simulate another architecture. |
| `DELILAH_SKIP_CHECKSUM` | unset | `1` is the deliberate override for a release with no `SHA256SUMS` (only releases before 1.1.0). It prints a loud warning that the download is unverified. It never skips a checksum *mismatch*. |

## Behaviour

1. Requires macOS (`uname -s` = `Darwin`) on `arm64`. A shell under Rosetta (`uname -m` = `x86_64`) counts as `arm64` when `sysctl -n hw.optional.arm64` is `1`. Otherwise exit 2: "Delilah needs a Mac with Apple Silicon".
2. Requires `curl`, `shasum`, `tar` and `ditto` (all ship with macOS). If one is missing, exit 2 and name it.
3. Downloads `Delilah_aarch64.app.tar.gz` and `SHA256SUMS` into a `mktemp -d` dir that is removed on exit (`trap`), with `curl -fL --retry 2` (https only, also on redirects, TLS 1.2+, unless `DELILAH_BASE_URL` is set).
4. `SHA256SUMS` missing (HTTP 404, or a missing `file://` in tests) → exit 3 with a hint about `DELILAH_SKIP_CHECKSUM=1`, unless that variable is set. Any other failure to fetch it is a download error (exit 9), even with that variable set.
5. Checks the archive's line in `SHA256SUMS`. On a mismatch, exit 4. Nothing is installed or removed.
6. Extracts into the temp dir. The archive must contain exactly `Delilah.app`, a directory and not a symlink, or exit 5.
7. If Delilah is running (`pgrep -x delilah-app`, the app's process name), exit 6: "Quit Delilah first". It never kills it.
8. If `$INSTALL_DIR/Delilah.app` exists: asks for confirmation (unless `--yes`/no TTY), `ditto`s the new app into a `mktemp -d` staging dir `Delilah.app.new.XXXXXX` next to the target, moves the old one into `Delilah.app.old.XXXXXX`, moves the new one into place, and removes the old copy only after that succeeds. If the copy fails, it restores the old one and exits 7.
9. Never runs `sudo` on its own. If `$INSTALL_DIR` isn't writable, exit 8 and suggest re-running with `sudo` or `DELILAH_INSTALL_DIR=~/Applications`.
10. Clears `com.apple.quarantine` defensively (`xattr -dr`, ignoring errors). `curl` doesn't set it, but a copied file might inherit it.
11. Prints the installed version (from `Contents/Info.plist` `CFBundleShortVersionString`) and "Open it with: open -a Delilah". It does not launch the app.

Exit codes: 0 ok; 1 usage; 2 platform or missing tool; 3 no checksums; 4 checksum mismatch;
5 bad archive; 6 app running; 7 install failed (nothing changed, or the old app restored; also a failing `mktemp`); 8 not writable;
9 download failed.

## Non-goals

No uninstall (documented in `docs/usage.md`), no daemon management (the app installs its
LaunchAgent on first launch), no Intel support, no sudo escalation.

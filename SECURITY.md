# Security policy

## Reporting a vulnerability

Please do not open a public issue for a vulnerability. Use GitHub private vulnerability
reporting instead: on the repository page, open the **Security** tab and choose
**Report a vulnerability**. Include what you found, how to reproduce it and the version.

We aim to acknowledge a report within about 7 days and will keep you informed of the fix.

## Supported versions

Only the latest release gets security fixes.

## Threat model

Delilah runs a local daemon (`delilahd`) that drives coding agents on your machine. This
is what it does and does not defend against, checked against the code.

- **Loopback only.** The daemon listens on `127.0.0.1:7420` by default
  (`crates/delilah-daemon/src/config.rs`), so it is not reachable from the network.
- **CORS allow-list.** Only the desktop app's origins (`tauri://localhost`,
  `http://tauri.localhost`, `http://localhost:1420`) and, for a preview daemon, the one
  origin in `DELILAH_UI_ORIGIN` are allowed (`crates/delilah-daemon/src/api.rs`).
- **Origin check.** Any request carrying a browser `Origin` outside that list is refused
  with 403, including WebSocket upgrades, which CORS does not cover. The CLI, curl and
  the agents' hooks send no `Origin` and pass.
- **DNS rebinding.** The `Host` header must be a loopback name (`127.0.0.1`,
  `localhost`, `[::1]`); otherwise the request is refused.
- **No authentication between local processes.** Anything running as your user can call
  the API. This is out of scope: a process with your rights can already do the same.
- **Agents run with your rights.** The daemon starts Claude Code agents as your user,
  each task in its own git worktree. Their hooks are passed with `claude --settings`
  pointing at a file outside the repository, and agents are started with a clean
  environment.
- **Jira token.** It is stored in the macOS Keychain, not in the database or on disk
  (`crates/delilah-daemon/src/secrets.rs`). Only non-secret settings, such as the Jira
  site, are in the SQLite settings table.
- **Updates.** The app's updater verifies each update against the public key in
  `app/src-tauri/tauri.conf.json`; releases are built and signed by the release workflow
  with the private key held as a repository secret. Releases carry a
  `SHA256SUMS` file and a build provenance attestation (verify with
  `shasum -a 256 -c SHA256SUMS --ignore-missing` and `gh attestation verify`). The
  checksums are an integrity check only, since they come from the same release; the
  attestation (which also covers `SHA256SUMS`) is what ties the files to this
  repository's workflow. `scripts/install.sh` checks the checksum, not the attestation.

## Known limits

- The app is not signed or notarized by Apple, so macOS will warn on first launch.
- Agents execute the content of the repositories you point them at (scripts, build
  files, instructions) with your user's rights. Only register repositories you trust.

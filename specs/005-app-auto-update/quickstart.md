# Quickstart: trying auto-update

What you can try in a preview and what needs the real app.

## In a preview (no updater)

```bash
scripts/preview.sh --fresh
```

- **Settings → Actualizaciones** shows the installed version and says updates are only available in the desktop app. There is no check button and no notice, because previews run in a browser (FR-005).
- **DB copy (FR-014)**: stop the preview, then write a fake old version with `echo 0.0.1 > fixtures/preview-home-N/version` and start it again. `fixtures/preview-home-N/backups/delilah-0.0.1.db` appears and `version` now holds the current version. Start it a fourth and fifth time with other fake versions: only 3 copies remain.

## Unit tests

```bash
cargo test -p delilah-daemon upgrade
cd app && npx vitest run src/updates.test.ts src/components/UpdateNotice.test.tsx
node --test scripts/release/
```

## End to end (needs the Tauri app; the maintainer does this once)

1. **Key**: run `cd app && npx tauri signer generate -w ~/.tauri/delilah.key`, store the password with `security add-generic-password -s delilah-updater -a "$USER" -w`, paste the public key into `tauri.conf.json`, and back the key file up.
2. **Bootstrap**: release the first version with the updater (e.g. 0.5.0) with `scripts/release.sh 0.5.0`, then install it by hand as today, once per Mac.
3. **Next release**: with 0.5.0 installed, publish 0.5.1, then open the app:
   - within about 10 s the notice shows 0.5.1 with the notes;
   - with a task running, "Actualizar" shows the session warning, and "Cuando terminen" waits;
   - after the relaunch, the menu shows 0.5.1, `curl -s 127.0.0.1:7420/api/v1/health` reports `"version":"0.5.1"`, `~/.delilah/plugins/delilah` shows 0.5.1, `~/.delilah/backups/delilah-0.5.0.db` exists, and `xattr /Applications/Delilah.app` has no quarantine.
4. **Tamper check (SC-004)**: point a local build's endpoint at a hand-edited `latest.json` with a wrong signature. The app reports that it could not verify the update and installs nothing.

Never restart or kill the installed daemon to try any of this. Step 3 is the real update, done through the app.

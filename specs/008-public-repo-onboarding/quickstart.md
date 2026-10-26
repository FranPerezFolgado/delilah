# Quickstart: verifying the feature

Run on the feature branch. Never against the installed app (7420).

## Story 1: README and docs

```bash
wc -l README.md                                   # < 200
git grep -nE 'claude\.ai/(code/)?artifact|plataforma|MVP-0 in progress|deploy key|Codex|\bPi\b' \
  -- . ':!specs' ':!docs/archive' ':!docs/redesign' ':!.specify'   # no matches
git grep -nE 'Te espera|Bandeja|Ajustes|Actualizaciones' -- README.md docs/*.md CONTRIBUTING.md SECURITY.md  # no matches
lychee --offline --no-progress README.md 'docs/**/*.md' \
  --exclude-path docs/archive --exclude-path docs/redesign  # 0 errors
gh repo view FranPerezFolgado/delilah --json description,repositoryTopics
```

## Story 2: install and trust

```bash
shellcheck scripts/install.sh scripts/test-install.sh
scripts/test-install.sh                           # all cases pass, temp dirs only
# after the first release with the new workflow:
curl -fsSL https://raw.githubusercontent.com/FranPerezFolgado/delilah/main/scripts/install.sh | bash
xattr -p com.apple.quarantine /Applications/Delilah.app   # "No such xattr"
gh release download vX.Y.Z -p 'Delilah_aarch64.app.tar.gz' -p SHA256SUMS && shasum -a 256 -c SHA256SUMS --ignore-missing
gh attestation verify Delilah_aarch64.app.tar.gz --repo FranPerezFolgado/delilah
```

## Story 3: community files

```bash
ls LICENSE CONTRIBUTING.md SECURITY.md CODE_OF_CONDUCT.md CHANGELOG.md \
   .github/pull_request_template.md .github/dependabot.yml .github/ISSUE_TEMPLATE/
gh api repos/FranPerezFolgado/delilah/community/profile -q .health_percentage   # 100 after merge
git grep -nE 'work Mac|FranPerezFolgado \(|personal account' -- CLAUDE.md   # no matches
cargo test -p delilah-daemon                       # renamed fixtures still pass
```

## Story 4: English fallback

```bash
cd app && npx tsc --noEmit && npx vitest related --run src/i18n.ts src/i18n.test.ts
cargo test --manifest-path app/src-tauri/Cargo.toml english_for
scripts/preview.sh    # Settings › Language = Auto; browser language fr → English UI; es → Spanish
```
The native menu needs the Tauri app: check it there by hand.

## Story 5: secret scan

```bash
gitleaks git --log-opts=--all --redact -v --report-format json --report-path "$TMPDIR/gitleaks.json" .
# triaged summary → ~/.delilah/tasks/LIBRE-15/secret-scan.md (outside the repo)
```

## Everything

```bash
cargo test --workspace
cargo test --manifest-path app/src-tauri/Cargo.toml
cd app && npx tsc --noEmit && npx vitest run
```

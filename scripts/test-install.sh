#!/usr/bin/env bash
# Tests for scripts/install.sh against a fake release dir (file:// base URL).
set -euo pipefail

HERE="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
INSTALL="$HERE/install.sh"
ASSET="Delilah_aarch64.app.tar.gz"

WORK="$(mktemp -d)"
[ -n "$WORK" ] || exit 1
cleanup() {
  chmod -R u+w "$WORK" 2>/dev/null || true
  rm -rf "${WORK:?}"
}
trap cleanup EXIT

# A fake pgrep so a real running Delilah never affects the tests.
mkdir -p "$WORK/bin"
printf '#!/bin/sh\nexit 1\n' >"$WORK/bin/pgrep"
chmod +x "$WORK/bin/pgrep"
export PATH="$WORK/bin:$PATH"

PASS=0
FAIL=0
ok() { PASS=$((PASS + 1)); echo "ok   $1"; }
ko() { FAIL=$((FAIL + 1)); echo "FAIL $1"; }
check() { # check <name> <condition exit status>
  if [ "$2" -eq 0 ]; then ok "$1"; else ko "$1"; fi
}

# make_app <dir> <version> : a dummy Delilah.app in <dir>
make_app() {
  mkdir -p "$1/Delilah.app/Contents/MacOS"
  cat >"$1/Delilah.app/Contents/Info.plist" <<PLIST
<?xml version="1.0" encoding="UTF-8"?>
<plist version="1.0"><dict>
<key>CFBundleShortVersionString</key><string>$2</string>
</dict></plist>
PLIST
  printf '#!/bin/sh\n' >"$1/Delilah.app/Contents/MacOS/delilah-app"
}

# make_release <dir> : fake release with a valid archive and SHA256SUMS
make_release() {
  local rel="$1" src="$1.src"
  mkdir -p "$rel" "$src"
  make_app "$src" 9.9.9
  tar -czf "$rel/$ASSET" -C "$src" Delilah.app
  (cd "$rel" && shasum -a 256 "$ASSET" >SHA256SUMS)
}

# run_install <release dir> <install dir> [extra env assignments...]; sets RC and OUT
run_install() {
  local rel="$1" dest="$2"
  shift 2
  RC=0
  OUT="$(env DELILAH_BASE_URL="file://$rel" DELILAH_INSTALL_DIR="$dest" DELILAH_ARCH="${ARCH:-arm64}" \
    "$@" bash "$INSTALL" --yes 2>&1)" || RC=$?
}

REL="$WORK/release"
make_release "$REL"

# (a) fresh install
DEST="$WORK/a"; mkdir -p "$DEST"
run_install "$REL" "$DEST"
s=0
[ "$RC" -eq 0 ] && [ -f "$DEST/Delilah.app/Contents/Info.plist" ] || s=1
if xattr -pr com.apple.quarantine "$DEST/Delilah.app" >/dev/null 2>&1; then s=1; fi
case "$OUT" in *9.9.9*) ;; *) s=1 ;; esac
check "(a) fresh install, exit 0, no quarantine (rc=$RC)" "$s"

# (b) reinstall replaces the existing app
DEST="$WORK/b"; mkdir -p "$DEST"
make_app "$DEST" 0.0.1
run_install "$REL" "$DEST"
s=0
[ "$RC" -eq 0 ] || s=1
grep -q 9.9.9 "$DEST/Delilah.app/Contents/Info.plist" || s=1
for f in "$DEST"/Delilah.app.*; do [ -e "$f" ] && s=1; done
check "(b) reinstall replaces the app (rc=$RC)" "$s"

# (c) tampered archive: exit 4, previous app untouched
BAD="$WORK/tampered"
make_release "$BAD"
printf 'tamper' >>"$BAD/$ASSET"
DEST="$WORK/c"; mkdir -p "$DEST"
make_app "$DEST" 0.0.1
run_install "$BAD" "$DEST"
s=0
[ "$RC" -eq 4 ] || s=1
grep -q 0.0.1 "$DEST/Delilah.app/Contents/Info.plist" || s=1
check "(c) tampered archive -> exit 4, old app untouched (rc=$RC)" "$s"

# (d) missing SHA256SUMS
NOSUMS="$WORK/nosums"
make_release "$NOSUMS"
rm "$NOSUMS/SHA256SUMS"
DEST="$WORK/d"; mkdir -p "$DEST"
run_install "$NOSUMS" "$DEST"
s=0
[ "$RC" -eq 3 ] || s=1
[ ! -e "$DEST/Delilah.app" ] || s=1
check "(d) missing SHA256SUMS -> exit 3 (rc=$RC)" "$s"

# (e) DELILAH_SKIP_CHECKSUM=1 without SHA256SUMS
DEST="$WORK/e"; mkdir -p "$DEST"
run_install "$NOSUMS" "$DEST" DELILAH_SKIP_CHECKSUM=1
s=0
[ "$RC" -eq 0 ] && [ -d "$DEST/Delilah.app" ] || s=1
case "$OUT" in *[Uu][Nn][Vv][Ee][Rr][Ii][Ff][Ii][Ee][Dd]*) ;; *) s=1 ;; esac
check "(e) skip checksum installs and warns (rc=$RC)" "$s"

# (f) wrong architecture
DEST="$WORK/f"; mkdir -p "$DEST"
ARCH=x86_64 run_install "$REL" "$DEST"
s=0
[ "$RC" -eq 2 ] || s=1
[ ! -e "$DEST/Delilah.app" ] || s=1
check "(f) x86_64 -> exit 2 (rc=$RC)" "$s"

# (g) archive without Delilah.app
EMPTY="$WORK/noapp"
mkdir -p "$EMPTY" "$WORK/noapp.src/Other.app"
tar -czf "$EMPTY/$ASSET" -C "$WORK/noapp.src" Other.app
(cd "$EMPTY" && shasum -a 256 "$ASSET" >SHA256SUMS)
DEST="$WORK/g"; mkdir -p "$DEST"
run_install "$EMPTY" "$DEST"
s=0
[ "$RC" -eq 5 ] || s=1
check "(g) archive without Delilah.app -> exit 5 (rc=$RC)" "$s"

# (h) unwritable install dir
DEST="$WORK/h"; mkdir -p "$DEST"
chmod 555 "$DEST"
run_install "$REL" "$DEST"
chmod 755 "$DEST"
s=0
[ "$RC" -eq 8 ] || s=1
check "(h) unwritable install dir -> exit 8 (rc=$RC)" "$s"

# (k) the archive can't be downloaded: exit 9, nothing installed
GONE="$WORK/gone"; mkdir -p "$GONE"
DEST="$WORK/k"; mkdir -p "$DEST"
run_install "$GONE" "$DEST"
s=0
[ "$RC" -eq 9 ] || s=1
[ ! -e "$DEST/Delilah.app" ] || s=1
check "(k) download fails -> exit 9 (rc=$RC)" "$s"

# (l) copying the app fails: exit 7, previous app untouched, no leftovers
mkdir -p "$WORK/bin-noditto"
printf '#!/bin/sh\nexit 1\n' >"$WORK/bin-noditto/ditto"
chmod +x "$WORK/bin-noditto/ditto"
DEST="$WORK/l"; mkdir -p "$DEST"
make_app "$DEST" 0.0.1
run_install "$REL" "$DEST" PATH="$WORK/bin-noditto:$PATH"
s=0
[ "$RC" -eq 7 ] || s=1
grep -q 0.0.1 "$DEST/Delilah.app/Contents/Info.plist" || s=1
for f in "$DEST"/Delilah.app.*; do [ -e "$f" ] && s=1; done
check "(l) copy fails -> exit 7, old app untouched (rc=$RC)" "$s"

# (m) Delilah is running: exit 6, never replaced
mkdir -p "$WORK/bin-running"
printf '#!/bin/sh\nexit 0\n' >"$WORK/bin-running/pgrep"
chmod +x "$WORK/bin-running/pgrep"
DEST="$WORK/m"; mkdir -p "$DEST"
make_app "$DEST" 0.0.1
run_install "$REL" "$DEST" PATH="$WORK/bin-running:$PATH"
s=0
[ "$RC" -eq 6 ] || s=1
grep -q 0.0.1 "$DEST/Delilah.app/Contents/Info.plist" || s=1
check "(m) app running -> exit 6, old app untouched (rc=$RC)" "$s"

# (n) a base URL that is neither https, file:// nor localhost http: exit 1, nothing fetched
DEST="$WORK/n"; mkdir -p "$DEST"
RC=0
env DELILAH_BASE_URL="http://example.invalid" DELILAH_INSTALL_DIR="$DEST" DELILAH_ARCH=arm64 \
  bash "$INSTALL" --yes >/dev/null 2>&1 || RC=$?
s=0
[ "$RC" -eq 1 ] || s=1
[ ! -e "$DEST/Delilah.app" ] || s=1
check "(n) plain http base URL -> exit 1 (rc=$RC)" "$s"

# (i)/(j) over HTTP: a 404 for SHA256SUMS is "no checksums" (exit 3); any other error is a
# download failure (exit 9), even with DELILAH_SKIP_CHECKSUM=1.
if command -v python3 >/dev/null 2>&1; then
  cat >"$WORK/server.py" <<'PY'
import http.server, os, sys
os.chdir(sys.argv[1])
class H(http.server.SimpleHTTPRequestHandler):
    def log_message(self, *a): pass
    def do_GET(self):
        if self.path == "/broken/SHA256SUMS":
            self.send_error(500)
        else:
            self.path = self.path.replace("/broken/", "/", 1)
            super().do_GET()
s = http.server.HTTPServer(("127.0.0.1", 0), H)
open(sys.argv[2], "w").write(str(s.server_address[1]))
s.serve_forever()
PY
  python3 "$WORK/server.py" "$NOSUMS" "$WORK/port" &
  SERVER=$!
  for _ in 1 2 3 4 5 6 7 8 9 10; do [ -s "$WORK/port" ] && break; sleep 0.2; done
  PORT="$(cat "$WORK/port" 2>/dev/null || true)"
  run_http() { # run_http <base url> <install dir> [env...]; sets RC
    local base="$1" dest="$2"
    shift 2
    RC=0
    env DELILAH_BASE_URL="$base" DELILAH_INSTALL_DIR="$dest" DELILAH_ARCH=arm64 "$@" \
      bash "$INSTALL" --yes >/dev/null 2>&1 || RC=$?
  }
  DEST="$WORK/i"; mkdir -p "$DEST"
  run_http "http://127.0.0.1:$PORT" "$DEST"
  check "(i) SHA256SUMS 404 over http -> exit 3 (rc=$RC)" "$([ "$RC" -eq 3 ] && echo 0 || echo 1)"
  DEST="$WORK/j"; mkdir -p "$DEST"
  run_http "http://127.0.0.1:$PORT/broken" "$DEST" DELILAH_SKIP_CHECKSUM=1
  s=0
  [ "$RC" -eq 9 ] || s=1
  [ ! -e "$DEST/Delilah.app" ] || s=1
  check "(j) SHA256SUMS 500 -> exit 9 even with skip (rc=$RC)" "$s"
  kill "$SERVER" 2>/dev/null || true
  wait "$SERVER" 2>/dev/null || true
else
  echo "skip (i)/(j): python3 not found"
fi

echo "$PASS passed, $FAIL failed"
[ "$FAIL" -eq 0 ]

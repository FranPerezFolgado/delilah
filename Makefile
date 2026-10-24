.PHONY: release release-dry-run

# make release [BUMP=major|minor|patch | VERSION=X.Y.Z] [MESSAGE="notes"] [YES=1]
export VERSION BUMP MESSAGE YES

release:
	@node scripts/release/cut.mjs

# make release-dry-run VERSION=X.Y.Z
release-dry-run:
	@test -n "$(VERSION)" || { echo "usage: make release-dry-run VERSION=X.Y.Z" >&2; exit 1; }
	scripts/release.sh "$(VERSION)" --dry-run

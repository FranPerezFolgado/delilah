import test from "node:test";
import assert from "node:assert/strict";
import { manifest, validateManifest } from "./manifest.mjs";

const args = {
  version: "0.5.0",
  notes: "## Novedades\n- a",
  pubDate: "2026-10-24T10:00:00Z",
  signature: "SIG",
  repo: "FranPerezFolgado/delilah",
};

test("manifest matches the contract", () => {
  assert.deepEqual(manifest(args), {
    version: "0.5.0",
    notes: "## Novedades\n- a",
    pub_date: "2026-10-24T10:00:00Z",
    platforms: {
      "darwin-aarch64": {
        signature: "SIG",
        url: "https://github.com/FranPerezFolgado/delilah/releases/download/v0.5.0/Delilah.app.tar.gz",
      },
    },
  });
});

test("a leading v is rejected", () => {
  assert.throws(() => manifest({ ...args, version: "v0.5.0" }));
});

const good = () => manifest({ version: "1.2.3", notes: "n", pubDate: "2026-01-01T00:00:00Z", signature: "sig", repo: "o/r" });

test("validateManifest accepts a good manifest", () => {
  assert.deepEqual(validateManifest(good(), { tag: "v1.2.3", repo: "o/r" }), []);
});

test("validateManifest flags version, draft url and empty signature", () => {
  const m = good();
  m.version = "1.2.4";
  m.platforms["darwin-aarch64"].url = "https://github.com/o/r/releases/download/untagged-abc/Delilah.app.tar.gz";
  m.platforms["darwin-aarch64"].signature = " ";
  assert.equal(validateManifest(m, { tag: "v1.2.3", repo: "o/r" }).length, 3);
});

test("validateManifest flags a missing platform", () => {
  assert.equal(validateManifest({ version: "1.2.3", platforms: {} }, { tag: "v1.2.3" }).length, 1);
});

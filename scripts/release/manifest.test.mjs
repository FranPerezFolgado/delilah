import test from "node:test";
import assert from "node:assert/strict";
import { spawnSync } from "node:child_process";
import { existsSync, mkdtempSync, readFileSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { fileURLToPath } from "node:url";
import { buildChecked, manifest, validateManifest } from "./manifest.mjs";

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

test("buildChecked builds from a tag and trims the signature", () => {
  const m = buildChecked({ tag: "v1.2.3", signature: " sig\n", notes: "n", repo: "o/r", pubDate: "2026-01-01T00:00:00Z" });
  assert.equal(m.version, "1.2.3");
  assert.equal(m.platforms["darwin-aarch64"].signature, "sig");
  assert.equal(m.platforms["darwin-aarch64"].url, "https://github.com/o/r/releases/download/v1.2.3/Delilah.app.tar.gz");
});

test("buildChecked rejects an empty signature", () => {
  assert.throws(() => buildChecked({ tag: "v1.2.3", signature: "\n", notes: "n", repo: "o/r" }), /empty signature/);
});

const cli = fileURLToPath(new URL("./manifest.mjs", import.meta.url));

test("--write writes a valid manifest from files", () => {
  const dir = mkdtempSync(join(tmpdir(), "manifest-"));
  writeFileSync(join(dir, "s.sig"), "SIGDATA\n");
  writeFileSync(join(dir, "n.md"), "## Notes\n- a\n");
  const out = join(dir, "latest.json");
  const r = spawnSync("node", [cli, "--write", out, "v1.2.3", join(dir, "s.sig"), join(dir, "n.md"), "o/r"]);
  assert.equal(r.status, 0, r.stderr.toString());
  const m = JSON.parse(readFileSync(out, "utf8"));
  assert.deepEqual(validateManifest(m, { tag: "v1.2.3", repo: "o/r" }), []);
  assert.equal(m.notes, "## Notes\n- a");
  assert.equal(m.platforms["darwin-aarch64"].signature, "SIGDATA");
});

test("--write exits 1 on an empty signature and writes nothing", () => {
  const dir = mkdtempSync(join(tmpdir(), "manifest-"));
  writeFileSync(join(dir, "s.sig"), "\n");
  writeFileSync(join(dir, "n.md"), "n");
  const out = join(dir, "latest.json");
  const r = spawnSync("node", [cli, "--write", out, "v1.2.3", join(dir, "s.sig"), join(dir, "n.md"), "o/r"]);
  assert.equal(r.status, 1);
  assert.equal(existsSync(out), false);
});

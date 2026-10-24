import test from "node:test";
import assert from "node:assert/strict";
import { mkdtempSync, mkdirSync, writeFileSync, readFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { readVersions, mismatches, writeVersions } from "./versions.mjs";

const FILES = {
  "Cargo.toml": `[workspace]\nmembers = ["crates/*"]\n\n[workspace.package]\nversion = "0.4.0"\nedition = "2024"\n\n[workspace.dependencies]\nfoo = { version = "1.2.3" }\nbar = { version = "9.9.9", path = "x" }\n`,
  "app/src-tauri/Cargo.toml": `[package]\nname = "delilah-app"\nversion = "0.4.0"\nedition = "2024"\n\n[dependencies]\nserde = { version = "1.0.0" }\n\n[dependencies.tokio]\nversion = "1.0.0"\n`,
  "app/package.json": `{\n  "name": "delilah-app",\n  "version": "0.4.0",\n  "dependencies": {\n    "react": "^19.0.0"\n  }\n}\n`,
  "app/src-tauri/tauri.conf.json": `{\n  "productName": "Delilah",\n  "version": "0.4.0",\n  "plugins": { "updater": { "pubkey": "X" } }\n}\n`,
};

function fixture() {
  const root = mkdtempSync(join(tmpdir(), "versions-"));
  mkdirSync(join(root, "app/src-tauri"), { recursive: true });
  for (const [f, c] of Object.entries(FILES)) writeFileSync(join(root, f), c);
  return root;
}

test("readVersions reads only the package version", () => {
  assert.deepEqual(readVersions(fixture()), {
    "Cargo.toml": "0.4.0",
    "app/src-tauri/Cargo.toml": "0.4.0",
    "app/package.json": "0.4.0",
    "app/src-tauri/tauri.conf.json": "0.4.0",
  });
});

test("mismatches lists each differing file", () => {
  const v = readVersions(fixture());
  assert.deepEqual(mismatches(v, "0.4.0"), []);
  assert.deepEqual(mismatches({ ...v, "app/package.json": "0.3.0" }, "0.4.0"), ["app/package.json"]);
  assert.equal(mismatches(v, "0.5.0").length, 4);
});

test("writeVersions changes exactly the four version lines", () => {
  const root = fixture();
  writeVersions(root, "0.5.0");
  assert.deepEqual(Object.values(readVersions(root)), ["0.5.0", "0.5.0", "0.5.0", "0.5.0"]);
  let changed = 0;
  for (const [f, before] of Object.entries(FILES)) {
    const a = before.split("\n");
    const b = readFileSync(join(root, f), "utf8").split("\n");
    assert.equal(a.length, b.length);
    a.forEach((l, i) => {
      if (l !== b[i]) {
        changed++;
        assert.equal(b[i], l.replace("0.4.0", "0.5.0"));
      }
    });
  }
  assert.equal(changed, 4);
});

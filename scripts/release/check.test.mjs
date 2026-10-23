import test from "node:test";
import assert from "node:assert/strict";
import { problems } from "./check.mjs";

const ok = {
  versions: { "Cargo.toml": "0.5.0", "app/package.json": "0.5.0" },
  expected: "0.5.0",
  env: { TAURI_SIGNING_PRIVATE_KEY: "k", TAURI_SIGNING_PRIVATE_KEY_PASSWORD: "p" },
  publishedTags: ["v0.4.0"],
  pubkey: "real-key",
  requireSecrets: true,
};

test("no problems when all is fine", () => assert.deepEqual(problems(ok), []));

test("version mismatch", () => {
  const p = problems({ ...ok, versions: { ...ok.versions, "app/package.json": "0.4.0" } });
  assert.equal(p.length, 1);
  assert.match(p[0], /app\/package\.json/);
});

test("non-semver version", () => {
  assert.ok(problems({ ...ok, expected: "v0.5" }).some((s) => /semver|X\.Y\.Z/i.test(s)));
});

test("missing secrets only when required, never printing values", () => {
  const env = { TAURI_SIGNING_PRIVATE_KEY: "" };
  const p = problems({ ...ok, env });
  assert.equal(p.length, 2);
  assert.match(p.join("\n"), /TAURI_SIGNING_PRIVATE_KEY_PASSWORD/);
  assert.deepEqual(problems({ ...ok, env, requireSecrets: false }), []);
});

test("already published tag", () => {
  const p = problems({ ...ok, publishedTags: ["v0.5.0"] });
  assert.equal(p.length, 1);
  assert.match(p[0], /v0\.5\.0/);
});

test("placeholder pubkey", () => {
  assert.equal(problems({ ...ok, pubkey: "REPLACE_WITH_UPDATER_PUBKEY" }).length, 1);
});

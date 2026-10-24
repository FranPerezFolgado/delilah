import test from "node:test";
import assert from "node:assert/strict";
import { bump, pickVersion } from "./cut.mjs";

test("bump raises the requested level and resets the lower ones", () => {
  assert.equal(bump("0.4.0", "minor"), "0.5.0");
  assert.equal(bump("0.4.7", "minor"), "0.5.0");
  assert.equal(bump("0.4.7", "patch"), "0.4.8");
  assert.equal(bump("0.4.7", "major"), "1.0.0");
});

test("bump rejects invalid levels and versions", () => {
  assert.throws(() => bump("0.4.0", "huge"));
  assert.throws(() => bump("0.4.0", undefined));
  assert.throws(() => bump("0.4", "patch"));
});

test("untagged current version: tag only", () => {
  assert.deepEqual(pickVersion({ current: "0.4.0", latestTag: "v0.3.0" }), { version: "0.4.0", needsBump: false });
  assert.deepEqual(pickVersion({ current: "0.1.0", latestTag: null }), { version: "0.1.0", needsBump: false });
});

test("current version already tagged without BUMP or VERSION is an error", () => {
  const r = pickVersion({ current: "0.4.0", latestTag: "v0.4.0" });
  assert.match(r.error, /v0\.4\.0 ya existe; usa BUMP= o VERSION=/);
});

test("BUMP computes the next version and needs a bump", () => {
  assert.deepEqual(pickVersion({ current: "0.4.0", latestTag: "v0.4.0", BUMP: "minor" }), {
    version: "0.5.0",
    needsBump: true,
  });
  assert.ok(pickVersion({ current: "0.4.0", latestTag: "v0.4.0", BUMP: "nope" }).error);
});

test("VERSION must be greater than the latest tag", () => {
  assert.deepEqual(pickVersion({ current: "0.4.0", latestTag: "v0.4.0", VERSION: "0.4.1" }), {
    version: "0.4.1",
    needsBump: true,
  });
  assert.ok(pickVersion({ current: "0.4.0", latestTag: "v0.4.0", VERSION: "0.4.0" }).error);
  assert.ok(pickVersion({ current: "0.4.0", latestTag: "v0.4.0", VERSION: "0.3.9" }).error);
  assert.ok(pickVersion({ current: "0.4.0", latestTag: "v0.4.0", VERSION: "x" }).error);
});

test("VERSION equal to the untagged current version needs no bump", () => {
  assert.deepEqual(pickVersion({ current: "0.5.0", latestTag: "v0.4.0", VERSION: "0.5.0" }), {
    version: "0.5.0",
    needsBump: false,
  });
});

test("VERSION and BUMP together is an error", () => {
  assert.ok(pickVersion({ current: "0.4.0", latestTag: "v0.3.0", VERSION: "0.5.0", BUMP: "minor" }).error);
});

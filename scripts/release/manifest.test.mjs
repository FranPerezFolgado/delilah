import test from "node:test";
import assert from "node:assert/strict";
import { manifest } from "./manifest.mjs";

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

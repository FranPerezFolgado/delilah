import test from "node:test";
import assert from "node:assert/strict";
import { releaseNotes, pickNotes } from "./notes.mjs";

test("groups features and fixes, strips scopes, drops the rest", () => {
  const out = releaseNotes([
    "feat(ui): first",
    "docs: skip",
    "fix: second",
    "chore(deps): skip",
    "feat: third",
    "test: skip",
    "refactor(x): skip",
    "Merge pull request #1 from x/y",
    "fix(api): fourth",
  ]);
  assert.equal(out, "## Novedades\n- first\n- third\n\n## Correcciones\n- second\n- fourth");
});

test("only one group when the other is empty", () => {
  assert.equal(releaseNotes(["fix: b"]), "## Correcciones\n- b");
});

test("falls back when nothing is left", () => {
  assert.equal(releaseNotes(["docs: a", "chore: b"]), "Mantenimiento interno.");
  assert.equal(releaseNotes([]), "Mantenimiento interno.");
});

test("pickNotes prefers a non-empty tag message", () => {
  assert.equal(pickNotes("  Hola\n\n", ["feat: a"], "v0.5.0"), "Hola");
  assert.equal(pickNotes("", ["feat: a"]), "## Novedades\n- a");
  assert.equal(pickNotes("v0.5.0\n", ["fix: b"], "v0.5.0"), "## Correcciones\n- b");
  assert.equal(pickNotes("v0.5.0", ["fix: b"]), "## Correcciones\n- b");
});

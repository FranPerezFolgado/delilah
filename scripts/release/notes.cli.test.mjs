import test from "node:test";
import assert from "node:assert/strict";
import { execFileSync } from "node:child_process";
import { mkdtempSync } from "node:fs";
import { tmpdir } from "node:os";
import { join, dirname } from "node:path";
import { fileURLToPath } from "node:url";

const SCRIPT = join(dirname(fileURLToPath(import.meta.url)), "notes.mjs");

function repo() {
  const dir = mkdtempSync(join(tmpdir(), "notes-"));
  const git = (...a) => execFileSync("git", ["-c", "user.name=t", "-c", "user.email=t@t", "-c", "commit.gpgsign=false", "-c", "tag.gpgsign=false", ...a], { cwd: dir, encoding: "utf8" });
  git("init", "-q");
  const commit = (m) => git("commit", "-q", "--allow-empty", "-m", m);
  const notes = (tag) => execFileSync("node", [SCRIPT, tag], { cwd: dir, encoding: "utf8" }).trim();
  return { git, commit, notes };
}

test("annotated tag message wins", () => {
  const r = repo();
  r.commit("feat: a");
  r.git("tag", "-a", "v0.1.0", "-m", "Hola mundo");
  assert.equal(r.notes("v0.1.0"), "Hola mundo");
});

test("lightweight tag generates notes, not the commit message", () => {
  const r = repo();
  r.commit("fix: b\n\nbody");
  r.git("tag", "v0.1.0");
  assert.equal(r.notes("v0.1.0"), "## Correcciones\n- b");
});

test("missing tag uses commits since the previous tag", () => {
  const r = repo();
  r.commit("feat: old");
  r.git("tag", "v0.1.0");
  r.commit("feat: new");
  assert.equal(r.notes("v0.2.0"), "## Novedades\n- new");
});

test("no previous tag uses all history", () => {
  const r = repo();
  r.commit("feat: a");
  r.commit("fix: b");
  assert.equal(r.notes("v0.1.0"), "## Novedades\n- a\n\n## Correcciones\n- b");
});

test("second tag only covers commits since the first", () => {
  const r = repo();
  r.commit("feat: old");
  r.git("tag", "v0.1.0");
  r.commit("fix: new");
  r.git("tag", "v0.2.0");
  assert.equal(r.notes("v0.2.0"), "## Correcciones\n- new");
});

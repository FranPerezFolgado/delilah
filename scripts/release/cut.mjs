// `make release`: preflight, pick the version, bump, check, tag, confirm, push (R16).
import { execFileSync } from "node:child_process";
import { mkdtempSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { dirname, join } from "node:path";
import { createInterface } from "node:readline/promises";
import { fileURLToPath } from "node:url";
import { releaseNotes } from "./notes.mjs";
import { readVersions, writeVersions } from "./versions.mjs";

const SEMVER = /^(\d+)\.(\d+)\.(\d+)$/;
const parse = (v) => {
  const m = SEMVER.exec(v ?? "");
  if (!m) throw new Error(`"${v}" is not semver X.Y.Z`);
  return m.slice(1).map(Number);
};
const cmp = (a, b) => {
  const [x, y] = [parse(a), parse(b)];
  for (let i = 0; i < 3; i++) if (x[i] !== y[i]) return x[i] - y[i];
  return 0;
};

export function bump(version, level) {
  const [major, minor, patch] = parse(version);
  if (level === "major") return `${major + 1}.0.0`;
  if (level === "minor") return `${major}.${minor + 1}.0`;
  if (level === "patch") return `${major}.${minor}.${patch + 1}`;
  throw new Error(`invalid BUMP "${level}" (major, minor or patch)`);
}

// latestTag is the newest v* tag ("v0.4.0") or null. Returns {version, needsBump} or {error}.
export function pickVersion({ current, latestTag, VERSION, BUMP }) {
  if (VERSION && BUMP) return { error: "usa VERSION o BUMP, no los dos" };
  const latest = latestTag ? latestTag.replace(/^v/, "") : null;
  let version;
  try {
    version = VERSION ? (parse(VERSION), VERSION) : BUMP ? bump(current, BUMP) : current;
    parse(current);
    if (latest) parse(latest);
  } catch (e) {
    return { error: e.message };
  }
  if (latest && !VERSION && !BUMP && cmp(version, latest) === 0) {
    return { error: `v${version} ya existe; usa BUMP= o VERSION=` };
  }
  if (latest && cmp(version, latest) <= 0) {
    return { error: `la versión ${version} debe ser mayor que la última etiqueta ${latestTag}` };
  }
  return { version, needsBump: version !== current };
}

const LOCKFILES = ["Cargo.lock", "app/src-tauri/Cargo.lock", "app/package-lock.json"];

function run(root) {
  const sh = (cmd, args, opts = {}) =>
    (execFileSync(cmd, args, { cwd: root, encoding: "utf8", stdio: ["ignore", "pipe", "inherit"], ...opts }) ?? "").trim();
  const git = (...args) => sh("git", args);
  const fail = (msg) => {
    console.error(`error: ${msg}`);
    process.exit(1);
  };
  const env = process.env;

  // 1. Preflight: nothing is changed until it all passes.
  try {
    if (git("rev-parse", "--abbrev-ref", "HEAD") !== "main") fail("no estás en main");
    if (git("status", "--porcelain")) fail("el árbol de trabajo no está limpio");
    git("fetch", "origin");
    if (git("rev-parse", "HEAD") !== git("rev-parse", "origin/main")) fail("main no coincide con origin/main");
    if (env.DELILAH_RELEASE_SKIP_GH !== "1") {
      const login = sh("gh", ["api", "user", "-q", ".login"]);
      if (login !== "FranPerezFolgado") fail(`gh está como "${login}", se esperaba FranPerezFolgado`);
    }
  } catch (e) {
    fail(e.message);
  }

  // 2. Pick the version.
  const tags = git("tag", "-l", "v*", "--sort=-v:refname").split("\n").filter(Boolean);
  const versions = readVersions(root);
  const picked = pickVersion({
    current: versions["Cargo.toml"],
    latestTag: tags[0] ?? null,
    VERSION: env.VERSION || undefined,
    BUMP: env.BUMP || undefined,
  });
  if (picked.error) fail(picked.error);
  const { version, needsBump } = picked;
  const tag = `v${version}`;
  if (tags.includes(tag)) fail(`${tag} ya existe`);
  // check.mjs wants a CHANGELOG.md section for the version, so a bump here regenerates it.
  if (needsBump) {
    try {
      sh("git", ["cliff", "--version"], { stdio: "ignore" });
    } catch {
      fail("BUMP/VERSION regenera CHANGELOG.md y necesita git-cliff: brew install git-cliff");
    }
  }

  const start = git("rev-parse", "HEAD");
  let tagged = false;
  const rollback = (e) => {
    console.error(`error: ${e.message ?? e}`);
    try {
      if (tagged) git("tag", "-d", tag);
      git("reset", "--hard", start);
      console.error(`se deshicieron el commit y la etiqueta locales (vuelta a ${start.slice(0, 7)})`);
    } catch (e2) {
      console.error(`no se pudo deshacer: ${e2.message}; ejecuta: git reset --hard ${start}`);
    }
    process.exit(1);
  };

  try {
    // 3. Bump and commit the four files plus the lockfiles.
    if (needsBump) {
      writeVersions(root, version);
      if (env.DELILAH_RELEASE_SKIP_LOCK !== "1") {
        // Test-only escape hatch: skips the slow lockfile refreshes.
        // `cargo metadata` rewrites the lockfile's workspace versions without building, so the
        // gitignored sidecar binary the shell's build script wants is not needed.
        for (const manifest of ["Cargo.toml", "app/src-tauri/Cargo.toml"]) {
          sh("cargo", ["metadata", "--format-version", "1", "--manifest-path", manifest], { stdio: ["ignore", "ignore", "inherit"] });
        }
        sh("npm", ["install", "--package-lock-only"], { cwd: join(root, "app"), stdio: "inherit" });
      }
      sh("git", ["cliff", "--tag", tag, "-o", "CHANGELOG.md"], { stdio: ["ignore", "ignore", "inherit"] });
      git("add", "--", ...Object.keys(versions), ...LOCKFILES, "CHANGELOG.md");
      git("commit", "-m", `chore: release ${version}`);
    }

    // 4. The same checks as CI, minus the secrets.
    execFileSync("node", [join(root, "scripts/release/check.mjs"), version], { cwd: root, stdio: "inherit" });

    // 5. Annotated tag; the message is what CI publishes as notes.
    const subjects = git("log", "--format=%s", tags[0] ? `${tags[0]}..HEAD` : "HEAD")
      .split("\n")
      .filter(Boolean);
    const notes = env.MESSAGE?.trim() || releaseNotes(subjects);
    const file = join(mkdtempSync(join(tmpdir(), "delilah-release-")), "notes.txt");
    writeFileSync(file, `${notes}\n`);
    git("tag", "-a", tag, "-F", file);
    tagged = true;

    // 6. Confirm.
    console.log(git("log", "--oneline", "origin/main..HEAD") || "(sin commits nuevos)");
    console.log(`\n${tag}\n${notes}\n`);
    return { tag, rollback, git, sh };
  } catch (e) {
    rollback(e);
  }
}

async function main() {
  const root = join(dirname(fileURLToPath(import.meta.url)), "../..");
  const { tag, rollback, git } = run(root);
  try {
    if (process.env.YES !== "1") {
      const rl = createInterface({ input: process.stdin, output: process.stdout });
      const answer = (await rl.question(`¿Publicar ${tag}? [s/N] `)).trim().toLowerCase();
      rl.close();
      if (answer !== "s" && answer !== "y") throw new Error("publicación cancelada");
    }
    // 7. Both refs move or neither does.
    git("push", "--atomic", "origin", "main", tag);
    console.log(`publicado ${tag}`);
  } catch (e) {
    rollback(e);
  }
}

if (process.argv[1] === fileURLToPath(import.meta.url)) await main();

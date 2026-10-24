import { readFileSync, writeFileSync } from "node:fs";
import { fileURLToPath } from "node:url";

// Name of the updater archive on the release: tauri-action uploads the local
// bundle/macos/Delilah.app.tar.gz under this name (it adds the arch).
export const ASSET = "Delilah_aarch64.app.tar.gz";

// Builds the update manifest (latest.json) the installed apps read.
export function manifest({ version, notes, pubDate, signature, repo }) {
  if (!/^\d+\.\d+\.\d+$/.test(version)) throw new Error(`version must be X.Y.Z without a leading "v": ${version}`);
  return {
    version,
    notes,
    pub_date: pubDate,
    platforms: {
      "darwin-aarch64": {
        signature,
        url: `https://github.com/${repo}/releases/download/v${version}/${ASSET}`,
      },
    },
  };
}

// Checks a published latest.json against the tag; returns a list of problems.
export function validateManifest(json, { tag, repo }) {
  const problems = [];
  if (!json || typeof json !== "object") return ["manifest is not an object"];
  if (json.version !== tag.replace(/^v/, "")) problems.push(`version ${json.version} does not match tag ${tag}`);
  const p = json.platforms?.["darwin-aarch64"];
  if (!p) return [...problems, "missing platform darwin-aarch64"];
  const suffix = `/releases/download/${tag}/${ASSET}`;
  if (typeof p.url !== "string" || !p.url.endsWith(suffix)) problems.push(`url ${p.url} does not end with ${suffix}`);
  else if (repo && !p.url.startsWith(`https://github.com/${repo}/`)) problems.push(`url ${p.url} is not from ${repo}`);
  if (typeof p.signature !== "string" || !p.signature.trim()) problems.push("empty signature");
  return problems;
}

// Builds and validates the manifest for a tag from the raw inputs; throws listing
// the problems. Shared by the release workflow and scripts/release.sh.
export function buildChecked({ tag, signature, notes, repo, pubDate = new Date().toISOString() }) {
  const m = manifest({ version: tag.replace(/^v/, ""), notes, pubDate, signature: signature.trim(), repo });
  const problems = validateManifest(m, { tag, repo });
  if (problems.length) throw new Error(problems.map((p) => `latest.json: ${p}`).join("\n"));
  return m;
}

const isMain = process.argv[1] === fileURLToPath(import.meta.url);

// node scripts/release/manifest.mjs --write <out> <tag> <sigfile> <notesfile> [owner/repo]
if (isMain && process.argv[2] === "--write") {
  const [, , , out, tag, sigFile, notesFile, repo = process.env.GITHUB_REPOSITORY] = process.argv;
  if (!out || !tag || !sigFile || !notesFile || !repo) {
    console.error("usage: node scripts/release/manifest.mjs --write <out> <tag> <sigfile> <notesfile> [owner/repo]");
    process.exit(2);
  }
  try {
    const m = buildChecked({
      tag,
      signature: readFileSync(sigFile, "utf8"),
      notes: readFileSync(notesFile, "utf8").trimEnd(),
      repo,
    });
    writeFileSync(out, JSON.stringify(m, null, 2) + "\n");
  } catch (e) {
    console.error(e.message);
    process.exit(1);
  }
  process.exit(0);
}

// node scripts/release/manifest.mjs --validate <file> <tag> [repo]
if (isMain && process.argv[2] === "--validate") {
  const [, , , file, tag, repo] = process.argv;
  if (!file || !tag) {
    console.error("usage: node scripts/release/manifest.mjs --validate <file> <tag> [owner/repo]");
    process.exit(2);
  }
  const problems = validateManifest(JSON.parse(readFileSync(file, "utf8")), { tag, repo });
  for (const p of problems) console.error(`latest.json: ${p}`);
  process.exit(problems.length ? 1 : 0);
}

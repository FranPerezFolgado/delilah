import { readFileSync } from "node:fs";
import { fileURLToPath } from "node:url";

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
        url: `https://github.com/${repo}/releases/download/v${version}/Delilah.app.tar.gz`,
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
  const suffix = `/releases/download/${tag}/Delilah.app.tar.gz`;
  if (typeof p.url !== "string" || !p.url.endsWith(suffix)) problems.push(`url ${p.url} does not end with ${suffix}`);
  else if (repo && !p.url.startsWith(`https://github.com/${repo}/`)) problems.push(`url ${p.url} is not from ${repo}`);
  if (typeof p.signature !== "string" || !p.signature.trim()) problems.push("empty signature");
  return problems;
}

// node scripts/release/manifest.mjs --validate <file> <tag> [repo]
if (process.argv[1] === fileURLToPath(import.meta.url) && process.argv[2] === "--validate") {
  const [, , , file, tag, repo] = process.argv;
  if (!file || !tag) {
    console.error("usage: node scripts/release/manifest.mjs --validate <file> <tag> [owner/repo]");
    process.exit(2);
  }
  const problems = validateManifest(JSON.parse(readFileSync(file, "utf8")), { tag, repo });
  for (const p of problems) console.error(`latest.json: ${p}`);
  process.exit(problems.length ? 1 : 0);
}

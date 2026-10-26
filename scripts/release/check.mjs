// Pre-release checks. Reports problems; never prints secret values.
import { execFileSync } from "node:child_process";
import { existsSync, readFileSync } from "node:fs";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";
import { mismatches, readVersions } from "./versions.mjs";

export const PLACEHOLDER_PUBKEY = "REPLACE_WITH_UPDATER_PUBKEY";
const SECRETS = ["TAURI_SIGNING_PRIVATE_KEY", "TAURI_SIGNING_PRIVATE_KEY_PASSWORD"];

export function problems({ versions, expected, env = {}, publishedTags = [], pubkey, requireSecrets = false, changelog }) {
  const out = [];
  if (!/^\d+\.\d+\.\d+$/.test(expected)) out.push(`version "${expected}" is not semver X.Y.Z`);
  for (const f of mismatches(versions, expected)) out.push(`${f} has version ${versions[f]}, expected ${expected}`);
  if (requireSecrets) {
    for (const name of SECRETS) if (!env[name]) out.push(`${name} is not set`);
  }
  if (publishedTags.includes(`v${expected}`)) out.push(`release v${expected} is already published`);
  if (!pubkey || pubkey === PLACEHOLDER_PUBKEY) out.push(`updater pubkey in tauri.conf.json is still ${PLACEHOLDER_PUBKEY}`);
  // `changelog` is the text of CHANGELOG.md; undefined skips the check.
  if (changelog !== undefined && /^\d+\.\d+\.\d+$/.test(expected)) {
    const has = changelog.split("\n").some((l) => l.startsWith(`## [${expected}]`));
    if (!has) out.push(`CHANGELOG.md has no ${expected} section: run scripts/bump-version.sh (or git cliff --tag v${expected} -o CHANGELOG.md) on a branch`);
  }
  return out;
}

function publishedTags() {
  try {
    const json = execFileSync("gh", ["release", "list", "--limit", "1000", "--json", "tagName,isDraft"], {
      encoding: "utf8",
      stdio: ["ignore", "pipe", "ignore"],
    });
    return JSON.parse(json).filter((r) => !r.isDraft).map((r) => r.tagName);
  } catch {
    // In CI (the release workflow sets GH_TOKEN) a gh failure must not turn the check off.
    if (process.env.CI) {
      console.error("error: gh release list failed; can't check whether this version is already published");
      process.exit(1);
    }
    console.error("note: gh not available (or not logged in); skipping the published-release check");
    return [];
  }
}

function main(args) {
  const expected = args.find((a) => !a.startsWith("--"));
  if (!expected) {
    console.error("usage: node scripts/release/check.mjs X.Y.Z [--require-secrets]");
    process.exit(2);
  }
  const root = join(dirname(fileURLToPath(import.meta.url)), "../..");
  const conf = JSON.parse(readFileSync(join(root, "app/src-tauri/tauri.conf.json"), "utf8"));
  const found = problems({
    versions: readVersions(root),
    expected,
    env: process.env,
    publishedTags: publishedTags(),
    pubkey: conf?.plugins?.updater?.pubkey,
    requireSecrets: args.includes("--require-secrets"),
    changelog: existsSync(join(root, "CHANGELOG.md")) ? readFileSync(join(root, "CHANGELOG.md"), "utf8") : "",
  });
  for (const p of found) console.error(`error: ${p}`);
  process.exit(found.length ? 1 : 0);
}

if (process.argv[1] === fileURLToPath(import.meta.url)) main(process.argv.slice(2));

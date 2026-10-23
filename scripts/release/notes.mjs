// Release notes: the tag message, or the feat/fix commits since the previous tag.
import { execFileSync } from "node:child_process";
import { fileURLToPath } from "node:url";

export const FALLBACK = "Mantenimiento interno.";

export function releaseNotes(subjects) {
  const groups = { feat: [], fix: [] };
  for (const s of subjects) {
    const m = s.match(/^(\w+)(?:\([^)]*\))?!?:\s*(.+)$/);
    if (m && m[1] in groups) groups[m[1]].push(m[2].trim());
  }
  const section = (title, items) => (items.length ? `## ${title}\n${items.map((i) => `- ${i}`).join("\n")}` : null);
  const parts = [section("Novedades", groups.feat), section("Correcciones", groups.fix)].filter(Boolean);
  return parts.length ? parts.join("\n\n") : FALLBACK;
}

export function pickNotes(tagMessage, subjects) {
  const msg = (tagMessage ?? "").trim();
  // The git default message is the bare tag name.
  if (msg && !/^v\d+\.\d+\.\d+\S*$/.test(msg)) return msg;
  return releaseNotes(subjects);
}

const git = (...args) => execFileSync("git", args, { encoding: "utf8" });

function main(tag) {
  if (!tag) {
    console.error("usage: node scripts/release/notes.mjs vX.Y.Z");
    process.exit(2);
  }
  const message = git("tag", "-l", "--format=%(contents)", tag);
  const tags = git("tag", "-l", "v*", "--sort=-v:refname").split("\n").filter(Boolean);
  const previous = tags[tags.indexOf(tag) + 1];
  const range = previous ? `${previous}..${tag}` : tag;
  const subjects = git("log", "--format=%s", range).split("\n").filter(Boolean);
  console.log(pickNotes(message, subjects));
}

if (process.argv[1] === fileURLToPath(import.meta.url)) main(process.argv[2]);

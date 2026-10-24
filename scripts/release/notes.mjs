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

const gitIn = (cwd) => (...args) => execFileSync("git", args, { encoding: "utf8", cwd });

// Notes for `tag`. The tag may not exist yet (local dry run): then the commits
// since the newest v* tag up to HEAD are used and there is no tag message.
export function notesFor(tag, cwd = process.cwd()) {
  const git = gitIn(cwd);
  const tags = git("tag", "-l", "v*", "--sort=-v:refname").split("\n").filter(Boolean);
  const exists = tags.includes(tag);
  let message = "";
  let range;
  if (exists) {
    // Only an annotated tag carries its own message; for a lightweight one
    // %(contents) is the commit message.
    const [type, ...rest] = git("tag", "-l", "--format=%(objecttype)%0a%(contents)", tag).split("\n");
    if (type === "tag") message = rest.join("\n");
    const previous = tags[tags.indexOf(tag) + 1];
    range = previous ? `${previous}..${tag}` : tag;
  } else {
    range = tags[0] ? `${tags[0]}..HEAD` : "HEAD";
  }
  const subjects = git("log", "--format=%s", range).split("\n").filter(Boolean);
  return pickNotes(message, subjects);
}

function main(tag) {
  if (!tag) {
    console.error("usage: node scripts/release/notes.mjs vX.Y.Z");
    process.exit(2);
  }
  console.log(notesFor(tag));
}

if (process.argv[1] === fileURLToPath(import.meta.url)) main(process.argv[2]);

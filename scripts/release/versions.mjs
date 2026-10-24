// Reads and writes the four places the app version lives.
import { readFileSync, writeFileSync } from "node:fs";
import { join } from "node:path";

export const FILES = [
  "Cargo.toml",
  "app/src-tauri/Cargo.toml",
  "app/package.json",
  "app/src-tauri/tauri.conf.json",
];

const TOML_SECTIONS = { "Cargo.toml": "workspace.package", "app/src-tauri/Cargo.toml": "package" };

// Index of the `version = "..."` line inside the given TOML section, or -1.
function tomlVersionLine(lines, section) {
  let inSection = false;
  for (let i = 0; i < lines.length; i++) {
    const header = lines[i].match(/^\s*\[([^\]]+)\]\s*$/);
    if (header) inSection = header[1].trim() === section;
    else if (inSection && /^\s*version\s*=\s*"[^"]*"/.test(lines[i])) return i;
  }
  return -1;
}

// Index of the top-level (two-space indented) `"version": "..."` line of a JSON file, or -1.
function jsonVersionLine(lines) {
  return lines.findIndex((l) => /^ {2}"version"\s*:\s*"[^"]*"/.test(l));
}

function locate(file, text) {
  const lines = text.split("\n");
  const i = file in TOML_SECTIONS ? tomlVersionLine(lines, TOML_SECTIONS[file]) : jsonVersionLine(lines);
  if (i < 0) throw new Error(`${file}: version field not found`);
  return { lines, i };
}

export function readVersions(root) {
  const out = {};
  for (const file of FILES) {
    const { lines, i } = locate(file, readFileSync(join(root, file), "utf8"));
    out[file] = lines[i].match(/[=:]\s*"([^"]*)"/)[1];
  }
  return out;
}

export function mismatches(versions, expected) {
  return Object.entries(versions)
    .filter(([, v]) => v !== expected)
    .map(([f]) => f);
}

export function writeVersions(root, version) {
  for (const file of FILES) {
    const path = join(root, file);
    const { lines, i } = locate(file, readFileSync(path, "utf8"));
    // Replace the value only: the first quoted string after the key's `=` or `:`.
    lines[i] = lines[i].replace(/([=:]\s*)"[^"]*"/, `$1"${version}"`);
    writeFileSync(path, lines.join("\n"));
  }
}

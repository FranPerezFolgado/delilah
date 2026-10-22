// Mirror of `slug()` / `branch_name()` in crates/delilah-core/src/project.rs; the
// shared table crates/delilah-core/tests/branch_names.json keeps both in step.

const SLUG_MAX = 40;

/** Lower-case, hyphen-separated form of a title: ASCII `a-z0-9-`, at most 40 chars. */
export function slug(title: string): string {
  let out = "";
  for (const ch of title.normalize("NFKD").replace(/\p{M}/gu, "")) {
    const c = ch >= "A" && ch <= "Z" ? ch.toLowerCase() : ch;
    if ((c >= "a" && c <= "z") || (c >= "0" && c <= "9")) out += c;
    else if (!out.endsWith("-")) out += "-";
  }
  out = out.replace(/^-+|-+$/g, "");
  if (out.length > SLUG_MAX) {
    const cut = out[SLUG_MAX] === "-" ? SLUG_MAX : out.slice(0, SLUG_MAX).lastIndexOf("-");
    out = out.slice(0, cut === -1 ? SLUG_MAX : cut).replace(/^-+|-+$/g, "");
  }
  return out;
}

/** Expands a branch template: `{ticket}` is the key verbatim, `{slug}` the slug of the title. */
export function branchName(template: string, key: string, title: string): string {
  const s = slug(title);
  let t = template;
  if (s === "") {
    for (let i = t.indexOf("{slug}"); i !== -1; i = t.indexOf("{slug}")) {
      const before = t.slice(0, i).replace(/[-_./]+$/, "").length;
      let after = i + "{slug}".length;
      if (before === 0) after = t.length - t.slice(after).replace(/^[-_./]+/, "").length;
      t = t.slice(0, before) + t.slice(after);
    }
  } else {
    t = t.split("{slug}").join(s);
  }
  return t.split("{ticket}").join(key);
}

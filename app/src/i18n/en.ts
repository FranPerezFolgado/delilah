// English for every text of the app, keyed by its Spanish original (see i18n.ts).
// The texts live in one file per component under ./en/; this merges them all.
const modules = import.meta.glob<{ EN: Record<string, string> }>("./en/*.ts", { eager: true });

export const EN: Record<string, string> = Object.assign(
  {},
  ...Object.values(modules).map((m) => m.EN),
);

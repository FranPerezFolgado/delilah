// CodeMirror set up for Delilah: the app's colours, the language picked from the file
// name (loaded on demand), Tab indents, ⌘S saves.
import { basicSetup } from "codemirror";
import { indentWithTab } from "@codemirror/commands";
import { LanguageDescription, syntaxHighlighting, type LanguageSupport } from "@codemirror/language";
import { languages } from "@codemirror/language-data";
import { Compartment, EditorState, type Extension } from "@codemirror/state";
import { EditorView, keymap } from "@codemirror/view";
import { oneDarkHighlightStyle } from "@codemirror/theme-one-dark";

/** Swapped in once the file's language has loaded. */
export const languageSlot = new Compartment();

const dark = typeof matchMedia !== "undefined" && matchMedia("(prefers-color-scheme: dark)").matches;

const theme = EditorView.theme(
  {
    "&": { height: "100%", backgroundColor: "var(--term)", color: "var(--tx)", fontSize: "12.5px" },
    ".cm-scroller": { fontFamily: 'ui-monospace, "SF Mono", Menlo, monospace', lineHeight: "1.55" },
    ".cm-gutters": { backgroundColor: "var(--term)", color: "var(--tx3)", border: "none" },
    ".cm-activeLine": { backgroundColor: "rgba(127, 127, 127, 0.07)" },
    ".cm-activeLineGutter": { backgroundColor: "transparent", color: "var(--tx2)" },
    "&.cm-focused": { outline: "none" },
    "&.cm-focused .cm-cursor": { borderLeftColor: "var(--acc)" },
    "&.cm-focused > .cm-scroller > .cm-selectionLayer .cm-selectionBackground, .cm-selectionBackground": {
      backgroundColor: "rgba(240, 160, 60, 0.25)",
    },
    ".cm-panels": { backgroundColor: "var(--panel)", color: "var(--tx)", borderColor: "var(--bd)" },
    ".cm-searchMatch": { backgroundColor: "rgba(240, 160, 60, 0.2)" },
    ".cm-tooltip": { backgroundColor: "var(--panel)", border: "1px solid var(--bd2)" },
  },
  { dark },
);

export interface Hooks {
  onSave: () => void;
  onChange: (state: EditorState) => void;
}

export function createState(doc: string, hooks: Hooks): EditorState {
  const extensions: Extension[] = [
    basicSetup,
    keymap.of([indentWithTab, { key: "Mod-s", preventDefault: true, run: () => (hooks.onSave(), true) }]),
    theme,
    // basicSetup's highlighting is only a fallback: in dark mode this one wins.
    dark ? syntaxHighlighting(oneDarkHighlightStyle) : [],
    languageSlot.of([]),
    EditorView.updateListener.of((u) => u.docChanged && hooks.onChange(u.state)),
  ];
  return EditorState.create({ doc, extensions });
}

export async function languageFor(path: string): Promise<LanguageSupport | null> {
  const name = path.slice(path.lastIndexOf("/") + 1);
  const desc = LanguageDescription.matchFilename(languages, name);
  return desc ? desc.load() : null;
}

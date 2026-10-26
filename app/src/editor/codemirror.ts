// CodeMirror set up for Delilah: the app's colours, the language picked from the file
// name (loaded on demand), Tab indents, ⌘S saves.
import { basicSetup } from "codemirror";
import { indentWithTab } from "@codemirror/commands";
import { LanguageDescription, defaultHighlightStyle, syntaxHighlighting, type LanguageSupport } from "@codemirror/language";
import { languages } from "@codemirror/language-data";
import { Compartment, EditorState, type Extension } from "@codemirror/state";
import { EditorView, ViewPlugin, keymap } from "@codemirror/view";
import { oneDarkHighlightStyle } from "@codemirror/theme-one-dark";
import { isDark, onSchemeChange } from "../theme";

/** Swapped in once the file's language has loaded. */
export const languageSlot = new Compartment();

const baseTheme = (dark: boolean) =>
  EditorView.theme(
    {
      "&": { height: "100%", backgroundColor: "var(--term-bg)", color: "var(--term-tx)", fontSize: "12.5px" },
      ".cm-scroller": { fontFamily: 'ui-monospace, "SF Mono", Menlo, monospace', lineHeight: "1.55" },
      ".cm-gutters": { backgroundColor: "var(--term-bg)", color: "var(--tx3)", border: "none" },
      ".cm-activeLine": { backgroundColor: "var(--hov)" },
      ".cm-activeLineGutter": { backgroundColor: "transparent", color: "var(--tx2)" },
      "&.cm-focused": { outline: "none" },
      "&.cm-focused .cm-cursor": { borderLeftColor: "var(--tx-strong)" },
      "&.cm-focused > .cm-scroller > .cm-selectionLayer .cm-selectionBackground, .cm-selectionBackground": {
        backgroundColor: "var(--sel)",
      },
      ".cm-panels": { backgroundColor: "var(--panel)", color: "var(--tx)", borderColor: "var(--bd)" },
      ".cm-searchMatch": { backgroundColor: "var(--bd2)" },
      ".cm-tooltip": { backgroundColor: "var(--panel)", border: "1px solid var(--bd2)" },
    },
    { dark },
  );

const themes = new Map<boolean, Extension>();

/** The theme and syntax colours for a scheme (the CSS variables do the rest live). */
export function themeFor(dark: boolean): Extension {
  let ext = themes.get(dark);
  if (!ext) {
    // basicSetup's default highlighting is the light style; dark swaps in One Dark's.
    ext = [baseTheme(dark), syntaxHighlighting(dark ? oneDarkHighlightStyle : defaultHighlightStyle)];
    themes.set(dark, ext);
  }
  return ext;
}

/** Holds the theme; reconfigured when the OS scheme changes. */
export const themeSlot = new Compartment();

/** Follows the OS scheme for as long as the view lives. */
const followScheme = ViewPlugin.fromClass(
  class {
    off: () => void;
    constructor(view: EditorView) {
      this.off = onSchemeChange((dark) => view.dispatch({ effects: themeSlot.reconfigure(themeFor(dark)) }));
    }
    destroy() {
      this.off();
    }
  },
);

export interface Hooks {
  onSave: () => void;
  onChange: (state: EditorState) => void;
}

export function createState(doc: string, hooks: Hooks): EditorState {
  const extensions: Extension[] = [
    basicSetup,
    keymap.of([indentWithTab, { key: "Mod-s", preventDefault: true, run: () => (hooks.onSave(), true) }]),
    themeSlot.of(themeFor(isDark())),
    followScheme,
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

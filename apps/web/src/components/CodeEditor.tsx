import { useEffect, useRef } from "react";
import { EditorState } from "@codemirror/state";
import { EditorView, keymap, lineNumbers, highlightActiveLine } from "@codemirror/view";
import { defaultKeymap, history, historyKeymap } from "@codemirror/commands";
import { markdown } from "@codemirror/lang-markdown";
import { syntaxHighlighting, defaultHighlightStyle } from "@codemirror/language";

export interface CodeEditorProps {
  value: string;
  onChange: (value: string) => void;
  label: string;
  readOnly?: boolean;
  className?: string;
}

export function CodeEditor({
  value,
  onChange,
  label,
  readOnly = false,
  className,
}: CodeEditorProps) {
  const container = useRef<HTMLDivElement | null>(null);
  const view = useRef<EditorView | null>(null);
  const changeHandler = useRef(onChange);
  changeHandler.current = onChange;
  const initialDocument = useRef(value);

  useEffect(() => {
    if (container.current === null) {
      return;
    }
    const state = EditorState.create({
      doc: initialDocument.current,
      extensions: [
        lineNumbers(),
        history(),
        highlightActiveLine(),
        markdown(),
        syntaxHighlighting(defaultHighlightStyle, { fallback: true }),
        keymap.of([...defaultKeymap, ...historyKeymap]),
        EditorView.lineWrapping,
        EditorState.readOnly.of(readOnly),
        EditorView.updateListener.of((update) => {
          if (update.docChanged) {
            changeHandler.current(update.state.doc.toString());
          }
        }),
        EditorView.theme({
          "&": {
            backgroundColor: "var(--surface-input)",
            color: "var(--text-primary)",
            fontSize: "13px",
            height: "100%",
          },
          ".cm-gutters": {
            backgroundColor: "var(--surface-sunken)",
            color: "var(--text-muted)",
            border: "none",
          },
          ".cm-activeLine": {
            backgroundColor: "color-mix(in srgb, var(--accent-primary) 8%, transparent)",
          },
          ".cm-activeLineGutter": { backgroundColor: "transparent" },
          ".cm-content": { caretColor: "var(--accent-primary)" },
          "&.cm-focused": { outline: "2px solid var(--border-focus)", outlineOffset: "-2px" },
          ".cm-scroller": { fontFamily: "var(--font-mono)", lineHeight: "1.6" },
        }),
      ],
    });
    const instance = new EditorView({ state, parent: container.current });
    view.current = instance;
    return () => {
      instance.destroy();
      view.current = null;
    };
  }, [readOnly]);

  useEffect(() => {
    const instance = view.current;
    if (instance === null) {
      return;
    }
    const current = instance.state.doc.toString();
    if (current !== value) {
      instance.dispatch({ changes: { from: 0, to: current.length, insert: value } });
    }
  }, [value]);

  return (
    <div
      ref={container}
      role="textbox"
      aria-label={label}
      aria-multiline="true"
      aria-readonly={readOnly}
      tabIndex={-1}
      className={className}
    />
  );
}

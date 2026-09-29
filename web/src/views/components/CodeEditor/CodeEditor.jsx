import { forwardRef, useEffect, useImperativeHandle, useMemo, useRef } from 'react';
import { autocompletion, closeBrackets, closeBracketsKeymap, completionKeymap } from '@codemirror/autocomplete';
import { defaultKeymap, history, historyKeymap, indentWithTab } from '@codemirror/commands';
import { Annotation, EditorState } from '@codemirror/state';
import { json } from '@codemirror/lang-json';
import { sql } from '@codemirror/lang-sql';
import { StreamLanguage } from '@codemirror/language';
import { toml } from '@codemirror/legacy-modes/mode/toml';
import {
  EditorView,
  highlightActiveLine,
  highlightActiveLineGutter,
  keymap,
  lineNumbers,
} from '@codemirror/view';
import { oneDark } from '@codemirror/theme-one-dark';
import { vscodeDark } from '@uiw/codemirror-theme-vscode';
import { useTranslation } from 'react-i18next';
import styles from './CodeEditor.module.css';
import { editorTheme } from './editorTheme';
import { createBundleCompletionSource } from './treeSitter/completionSource';
import { createTreeSitterHighlightExtension } from './treeSitter/highlightExtension';

const EXTERNAL_UPDATE = Annotation.define();
const TREE_SITTER_LANGUAGES = new Set(['wpl', 'oml', 'wfs', 'wfl', 'wfg']);

function CodeEditor(props, ref) {
  const editorRef = useRef(null);
  const viewRef = useRef(null);
  const language = props.language || 'plain';
  const textColor = props.textColor;
  const theme = props.theme; // 可选的主题属性
  const { i18n } = useTranslation();
  const uiLanguage = i18n.language;
  const wplCompletionSource = useMemo(
    () => createBundleCompletionSource('wpl', uiLanguage),
    [uiLanguage],
  );
  const omlCompletionSource = useMemo(
    () => createBundleCompletionSource('oml', uiLanguage),
    [uiLanguage],
  );
  const wfsCompletionSource = useMemo(
    () => createBundleCompletionSource('wfs', uiLanguage),
    [uiLanguage],
  );
  const wflCompletionSource = useMemo(
    () => createBundleCompletionSource('wfl', uiLanguage),
    [uiLanguage],
  );
  const wfgCompletionSource = useMemo(
    () => createBundleCompletionSource('wfg', uiLanguage),
    [uiLanguage],
  );
  const colorTheme = useMemo(() => {
    if (!textColor) return null;
    return EditorView.theme({
      '&': {
        color: textColor,
      },
      '.cm-content': {
        color: textColor,
      },
    });
  }, [textColor]);

  useImperativeHandle(ref, () => ({
    getValue: () => viewRef.current?.state.doc.toString() || '',
    setValue: (value) => {
      const view = viewRef.current;
      if (!view) return;
      const nextValue = value || '';
      const currentValue = view.state.doc.toString();
      if (currentValue !== nextValue) {
        view.dispatch({
          changes: { from: 0, to: currentValue.length, insert: nextValue },
          annotations: EXTERNAL_UPDATE.of(true),
        });
      }
    },
  }));

  useEffect(() => {
    if (!editorRef.current) return;

    const updateListener = EditorView.updateListener.of((update) => {
      if (update.docChanged && !update.transactions.some((tr) => tr.annotation(EXTERNAL_UPDATE))) {
        props.onChange?.(update.state.doc.toString());
      }
    });

    const extensions = [
      lineNumbers(),
      highlightActiveLineGutter(),
      highlightActiveLine(),
      EditorView.lineWrapping,
      EditorState.tabSize.of(2),
      history(),
      closeBrackets(),
      keymap.of([
        ...completionKeymap,
        ...closeBracketsKeymap,
        indentWithTab,
        ...historyKeymap,
        ...defaultKeymap,
      ]),
      editorTheme,
      ...(colorTheme ? [colorTheme] : []),
      updateListener,
    ];

    // 添加主题：默认使用 vscodeDark
    if (theme === 'vscodeDark' || !theme) {
      extensions.push(vscodeDark);
    } else {
      extensions.push(oneDark);
    }

    if (language === 'wpl') {
      extensions.splice(
        6,
        0,
        createTreeSitterHighlightExtension('wpl'),
        autocompletion({ override: [wplCompletionSource] }),
      );
    }
    if (language === 'oml') {
      extensions.splice(
        6,
        0,
        createTreeSitterHighlightExtension('oml'),
        autocompletion({ override: [omlCompletionSource] }),
      );
    }
    if (language === 'wfs') {
      extensions.splice(
        6,
        0,
        createTreeSitterHighlightExtension('wfs'),
        autocompletion({ override: [wfsCompletionSource] }),
      );
    }
    if (language === 'wfl') {
      extensions.splice(
        6,
        0,
        createTreeSitterHighlightExtension('wfl'),
        autocompletion({ override: [wflCompletionSource] }),
      );
    }
    if (language === 'wfg') {
      extensions.splice(
        6,
        0,
        createTreeSitterHighlightExtension('wfg'),
        autocompletion({ override: [wfgCompletionSource] }),
      );
    }
    if (TREE_SITTER_LANGUAGES.has(language) && language !== 'wpl' && language !== 'oml') {
      // 上面已分别插入 wf 系列高亮与补全，这里不重复注册。
    }
    if (language === 'json') {
      extensions.splice(6, 0, json());
    }
    if (language === 'toml') {
      extensions.splice(6, 0, StreamLanguage.define(toml));
    }
    if (language === 'sql') {
      extensions.splice(6, 0, sql());
    }

    const state = EditorState.create({
      doc: props.value || '',
      extensions,
    });

    const view = new EditorView({
      state,
      parent: editorRef.current,
    });

    viewRef.current = view;

    return () => {
      view.destroy();
      viewRef.current = null;
    };
  }, [
    language,
    uiLanguage,
    wplCompletionSource,
    omlCompletionSource,
    wfsCompletionSource,
    wflCompletionSource,
    wfgCompletionSource,
    colorTheme,
    theme,
  ]);

  // 同步外部 value 到编辑器
  useEffect(() => {
    const view = viewRef.current;
    if (!view) return;
    
    // 只在 value 确实变化时更新
    const nextValue = props.value ?? '';
    const currentValue = view.state.doc.toString();
    
    if (currentValue !== nextValue) {
      view.dispatch({
        changes: { from: 0, to: currentValue.length, insert: nextValue },
        annotations: EXTERNAL_UPDATE.of(true),
      });
    }
  }, [props.value]);

  return (
    <div className={`${styles.editor} ${props.className || ''}`}>
      <div ref={editorRef} className={styles.code} />
    </div>
  );
}

export default forwardRef(CodeEditor);

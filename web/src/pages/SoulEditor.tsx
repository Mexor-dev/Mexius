import { useState, useEffect, useCallback, useRef } from 'react';
import { Feather, Save, RotateCcw, AlertCircle, CheckCircle } from 'lucide-react';
import { getSoul, saveSoul } from '@/lib/api';

type SaveState = 'idle' | 'saving' | 'saved' | 'error';

export default function SoulEditor() {
  const [content, setContent] = useState('');
  const [original, setOriginal] = useState('');
  const [loading, setLoading] = useState(true);
  const [saveState, setSaveState] = useState<SaveState>('idle');
  const [errorMsg, setErrorMsg] = useState('');
  const textareaRef = useRef<HTMLTextAreaElement>(null);

  // Load soul.md on mount
  useEffect(() => {
    getSoul()
      .then((c) => { setContent(c); setOriginal(c); })
      .catch((err) => {
        setContent('# Soul\n\nFailed to load: ' + err.message);
        setOriginal('');
      })
      .finally(() => setLoading(false));
  }, []);

  const isDirty = content !== original;

  const handleSave = useCallback(async () => {
    setSaveState('saving');
    setErrorMsg('');
    try {
      await saveSoul(content);
      setOriginal(content);
      setSaveState('saved');
      setTimeout(() => setSaveState('idle'), 2500);
    } catch (err: unknown) {
      setErrorMsg(err instanceof Error ? err.message : 'Save failed');
      setSaveState('error');
    }
  }, [content]);

  const handleRevert = () => {
    setContent(original);
    setSaveState('idle');
  };

  // Ctrl/Cmd + S shortcut
  useEffect(() => {
    const handler = (e: KeyboardEvent) => {
      if ((e.ctrlKey || e.metaKey) && e.key === 's') {
        e.preventDefault();
        if (isDirty && saveState !== 'saving') handleSave();
      }
    };
    window.addEventListener('keydown', handler);
    return () => window.removeEventListener('keydown', handler);
  }, [handleSave, isDirty, saveState]);

  // Tab key inserts two spaces instead of moving focus
  const handleKeyDown = (e: React.KeyboardEvent<HTMLTextAreaElement>) => {
    if (e.key === 'Tab') {
      e.preventDefault();
      const ta = textareaRef.current;
      if (!ta) return;
      const start = ta.selectionStart;
      const end = ta.selectionEnd;
      const newContent = content.substring(0, start) + '  ' + content.substring(end);
      setContent(newContent);
      requestAnimationFrame(() => {
        ta.selectionStart = ta.selectionEnd = start + 2;
      });
    }
  };

  if (loading) {
    return (
      <div className="flex items-center justify-center h-64">
        <div className="h-8 w-8 border-2 rounded-full animate-spin"
          style={{ borderColor: 'var(--pc-border)', borderTopColor: '#a78bfa' }} />
      </div>
    );
  }

  return (
    <div className="flex flex-col h-[calc(100vh-4rem)] p-6 gap-4 animate-fade-in">
      {/* Header */}
      <div className="flex items-center justify-between flex-shrink-0">
        <div className="flex items-center gap-3">
          <div className="p-2 rounded-2xl" style={{ background: 'rgba(167,139,250,0.1)', color: '#a78bfa' }}>
            <Feather className="h-5 w-5" />
          </div>
          <div>
            <h1 className="text-lg font-semibold" style={{ color: 'var(--pc-text-primary)' }}>Soul Editor</h1>
            <p className="text-xs" style={{ color: 'var(--pc-text-muted)' }}>
              soul.md · Identity & character definition
            </p>
          </div>
        </div>

        <div className="flex items-center gap-2">
          {isDirty && (
            <span className="text-xs px-2 py-1 rounded-md"
              style={{ background: 'rgba(251,191,36,0.1)', color: '#fbbf24', border: '1px solid rgba(251,191,36,0.3)' }}>
              Unsaved changes
            </span>
          )}

          {saveState === 'saved' && (
            <span className="flex items-center gap-1.5 text-xs px-2 py-1 rounded-md animate-fade-in"
              style={{ background: 'rgba(52,211,153,0.1)', color: '#34d399', border: '1px solid rgba(52,211,153,0.3)' }}>
              <CheckCircle className="h-3 w-3" /> Saved
            </span>
          )}

          {saveState === 'error' && (
            <span className="flex items-center gap-1.5 text-xs px-2 py-1 rounded-md"
              style={{ background: 'rgba(239,68,68,0.1)', color: '#f87171', border: '1px solid rgba(239,68,68,0.3)' }}>
              <AlertCircle className="h-3 w-3" /> {errorMsg}
            </span>
          )}

          <button
            onClick={handleRevert}
            disabled={!isDirty || saveState === 'saving'}
            className="flex items-center gap-1.5 px-3 py-1.5 rounded-lg text-xs font-medium transition-colors disabled:opacity-40"
            style={{ background: 'var(--pc-hover)', color: 'var(--pc-text-muted)', border: '1px solid var(--pc-border)' }}
          >
            <RotateCcw className="h-3 w-3" />
            Revert
          </button>

          <button
            onClick={handleSave}
            disabled={!isDirty || saveState === 'saving'}
            className="flex items-center gap-1.5 px-4 py-1.5 rounded-lg text-xs font-semibold transition-all disabled:opacity-40 btn-electric"
          >
            {saveState === 'saving' ? (
              <>
                <span className="h-3 w-3 border border-white/30 border-t-white rounded-full animate-spin" />
                Saving…
              </>
            ) : (
              <>
                <Save className="h-3 w-3" />
                Save  <kbd className="ml-1 opacity-60 text-[10px]">⌘S</kbd>
              </>
            )}
          </button>
        </div>
      </div>

      {/* Description */}
      <div className="rounded-2xl px-4 py-3 text-xs flex-shrink-0"
        style={{ background: 'rgba(167,139,250,0.06)', border: '1px solid rgba(167,139,250,0.15)', color: 'var(--pc-text-muted)' }}>
        <strong style={{ color: '#a78bfa' }}>soul.md</strong> defines the agent's identity, values, and behavioral boundaries.
        Changes are synced immediately to the Mexius data directory on save.
        Supports Markdown — use headings, bullet points, and code blocks freely.
      </div>

      {/* Editor */}
      <div className="flex-1 rounded-2xl overflow-hidden" style={{ border: '1px solid var(--pc-border)' }}>
        <div className="flex items-center justify-between px-4 py-2 border-b"
          style={{ background: 'var(--pc-bg-elevated)', borderColor: 'var(--pc-border)' }}>
          <span className="text-xs font-mono" style={{ color: 'var(--pc-text-faint)' }}>soul.md</span>
          <span className="text-xs" style={{ color: 'var(--pc-text-faint)' }}>
            {content.split('\n').length} lines · {content.length} chars
          </span>
        </div>
        <textarea
          ref={textareaRef}
          value={content}
          onChange={(e) => setContent(e.target.value)}
          onKeyDown={handleKeyDown}
          spellCheck={false}
          className="w-full h-[calc(100%-2.5rem)] resize-none p-4 font-mono text-sm leading-relaxed outline-none"
          style={{
            background: 'var(--pc-bg-base)',
            color: 'var(--pc-text-primary)',
            caretColor: '#a78bfa',
          }}
          placeholder="# Soul&#10;&#10;Define your agent's identity, values, and behavioral boundaries here..."
        />
      </div>
    </div>
  );
}

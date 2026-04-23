import { useState, useEffect, useRef, useCallback } from 'react';
import { Radio, Pause, Play, Trash2, ArrowDown, Circle } from 'lucide-react';
import { apiOrigin, basePath } from '@/lib/basePath';
import { getToken } from '@/lib/auth';
import { getHardware, getLogs } from '@/lib/api';
import type { HardwareTelemetry } from '@/types/api';

interface LogLine {
  id: number;
  raw: string;
  level: 'INFO' | 'WARN' | 'ERROR' | 'DEBUG' | 'TRACE' | 'UNKNOWN';
  ts: string;
  body: string;
}

let lineCounter = 0;

function classifyLine(raw: string): Omit<LogLine, 'id' | 'raw'> {
  // Common patterns: [2024-01-01T12:00:00Z INFO  gateway] message
  const isoMatch = raw.match(/\[(\d{4}-\d\d-\d\dT[\d:.]+Z?)\s+(INFO|WARN|ERROR|DEBUG|TRACE)/i);
  if (isoMatch && isoMatch[1] && isoMatch[2]) {
    const level = isoMatch[2].toUpperCase() as LogLine['level'];
    const ts = isoMatch[1];
    const body = raw.slice(isoMatch[0].length).replace(/\]/, '').trim();
    return { level, ts, body };
  }

  // Systemd / journald style: "INFO  gateway" prefix
  if (/\bERROR\b/i.test(raw)) return { level: 'ERROR', ts: '', body: raw };
  if (/\bWARN\b/i.test(raw)) return { level: 'WARN', ts: '', body: raw };
  if (/\bDEBUG\b/i.test(raw)) return { level: 'DEBUG', ts: '', body: raw };
  if (/\bINFO\b/i.test(raw)) return { level: 'INFO', ts: '', body: raw };
  if (/\bTRACE\b/i.test(raw)) return { level: 'TRACE', ts: '', body: raw };

  return { level: 'UNKNOWN', ts: '', body: raw };
}

function levelColor(level: LogLine['level']): string {
  switch (level) {
    case 'ERROR': return '#f87171';
    case 'WARN':  return '#fbbf24';
    case 'INFO':  return '#34d399';
    case 'DEBUG': return '#38bdf8';
    case 'TRACE': return 'var(--pc-text-faint)';
    default:      return 'var(--pc-text-muted)';
  }
}

const MAX_LINES = 2000;

export default function Telemetry() {
  const [lines, setLines] = useState<LogLine[]>([]);
  const [paused, setPaused] = useState(false);
  const [autoScroll, setAutoScroll] = useState(true);
  const [connected, setConnected] = useState(false);
  const [filter, setFilter] = useState('');
  const [hardware, setHardware] = useState<HardwareTelemetry | null>(null);
  const containerRef = useRef<HTMLDivElement>(null);
  const pausedRef = useRef(false);
  const abortRef = useRef<AbortController | null>(null);

  pausedRef.current = paused;

  const appendRawLines = useCallback((rawLines: string[]) => {
    if (rawLines.length === 0) return;
    setLines((prev) => {
      const parsed = rawLines.map((lineText) => {
        const { level, ts, body } = classifyLine(lineText);
        return { id: ++lineCounter, raw: lineText, level, ts, body } as LogLine;
      });
      const next = [...prev, ...parsed];
      return next.length > MAX_LINES ? next.slice(next.length - MAX_LINES) : next;
    });
  }, []);

  const connect = useCallback(() => {
    if (abortRef.current) abortRef.current.abort();
    const ac = new AbortController();
    abortRef.current = ac;

    const token = getToken();
    const params = new URLSearchParams();
    if (token) params.set('token', token);
    const url = `${apiOrigin ?? ''}${basePath}/api/v1/logs/stream?${params.toString()}`;

    setConnected(false);

    fetch(url, { signal: ac.signal, headers: { Accept: 'text/event-stream' } })
      .then(async (res) => {
        if (!res.ok) throw new Error(`HTTP ${res.status}`);
        if (!res.body) return;
        setConnected(true);
        const reader = res.body.getReader();
        const dec = new TextDecoder();
        let partial = '';
        while (true) {
          const { done, value } = await reader.read();
          if (done) break;
          const text = dec.decode(value, { stream: true });
          partial += text;
          const parts = partial.split('\n');
          partial = parts.pop() ?? '';
          const newLines: LogLine[] = [];
          for (const part of parts) {
            if (!part.startsWith('data:')) continue;
            const raw = part.slice(5).trimStart();
            if (!raw) continue;
            if (pausedRef.current) continue;
            // Gateway wraps lines in {"line":"..."} JSON
            let lineText = raw;
            try {
              const parsed = JSON.parse(raw) as { line?: string };
              if (parsed.line) lineText = parsed.line;
            } catch { /* use raw */ }
            const { level, ts, body } = classifyLine(lineText);
            newLines.push({ id: ++lineCounter, raw: lineText, level, ts, body });
          }
          if (newLines.length > 0) {
            setLines((prev) => {
              const next = [...prev, ...newLines];
              return next.length > MAX_LINES ? next.slice(next.length - MAX_LINES) : next;
            });
          }
        }
        setConnected(false);
      })
      .catch((err) => {
        if (err.name !== 'AbortError') {
          setConnected(false);
          // retry after 3s
          setTimeout(connect, 3000);
        }
      });
  }, []);

  useEffect(() => {
    getLogs(300)
      .then((snapshot) => appendRawLines(snapshot.lines))
      .catch(() => {});

    getHardware().then(setHardware).catch(() => {});
    const timer = window.setInterval(() => {
      getHardware().then(setHardware).catch(() => {});
    }, 5000);

    connect();
    return () => { abortRef.current?.abort(); window.clearInterval(timer); };
  }, [appendRawLines, connect]);

  // Auto-scroll
  useEffect(() => {
    if (!autoScroll) return;
    const el = containerRef.current;
    if (el) el.scrollTop = el.scrollHeight;
  }, [lines, autoScroll]);

  const handleScroll = () => {
    const el = containerRef.current;
    if (!el) return;
    const atBottom = el.scrollHeight - el.scrollTop - el.clientHeight < 40;
    setAutoScroll(atBottom);
  };

  const scrollToBottom = () => {
    const el = containerRef.current;
    if (el) { el.scrollTop = el.scrollHeight; setAutoScroll(true); }
  };

  const displayLines = filter
    ? lines.filter((l) => l.raw.toLowerCase().includes(filter.toLowerCase()))
    : lines;

  return (
    <div className="flex flex-col h-[calc(100vh-4rem)] p-6 gap-4 animate-fade-in">
      {/* Header */}
      <div className="flex items-center justify-between flex-shrink-0 flex-wrap gap-3">
        <div className="flex items-center gap-3">
          <div className="p-2 rounded-2xl" style={{ background: 'rgba(56,189,248,0.1)', color: '#38bdf8' }}>
            <Radio className="h-5 w-5" />
          </div>
          <div>
            <h1 className="text-lg font-semibold" style={{ color: 'var(--pc-text-primary)' }}>Telemetry</h1>
            <p className="text-xs" style={{ color: 'var(--pc-text-muted)' }}>Live gateway.log stream · {lines.length} lines</p>
          </div>
        </div>

        <div className="flex items-center gap-2">
          <div className="px-3 py-1.5 rounded-lg text-xs font-medium"
            style={{
              background: hardware?.loaded_model ? 'rgba(167,139,250,0.10)' : 'var(--pc-bg-elevated)',
              color: hardware?.loaded_model ? '#a78bfa' : 'var(--pc-text-muted)',
              border: `1px solid ${hardware?.loaded_model ? 'rgba(167,139,250,0.25)' : 'var(--pc-border)'}`,
            }}>
            {hardware?.loaded_model ? `Loaded: ${hardware.loaded_model}` : 'No model loaded'}
          </div>

          {/* Connection dot */}
          <div className="flex items-center gap-1.5 px-3 py-1.5 rounded-lg text-xs font-medium"
            style={{
              background: connected ? 'rgba(52,211,153,0.1)' : 'rgba(239,68,68,0.1)',
              color: connected ? '#34d399' : '#f87171',
              border: `1px solid ${connected ? 'rgba(52,211,153,0.3)' : 'rgba(239,68,68,0.3)'}`,
            }}>
            <Circle className={`h-2 w-2 fill-current ${connected && !paused ? 'animate-pulse' : ''}`} />
            {connected ? (paused ? 'Paused' : 'Live') : 'Reconnecting…'}
          </div>

          {/* Filter */}
          <input
            type="text"
            value={filter}
            onChange={(e) => setFilter(e.target.value)}
            placeholder="Filter…"
            className="px-3 py-1.5 rounded-lg text-xs outline-none w-36"
            style={{
              background: 'var(--pc-bg-elevated)',
              color: 'var(--pc-text-primary)',
              border: '1px solid var(--pc-border)',
            }}
          />

          {/* Pause/Resume */}
          <button
            onClick={() => setPaused((v) => !v)}
            className="flex items-center gap-1.5 px-3 py-1.5 rounded-lg text-xs font-medium transition-colors"
            style={{
              background: paused ? 'rgba(251,191,36,0.1)' : 'var(--pc-hover)',
              color: paused ? '#fbbf24' : 'var(--pc-text-muted)',
              border: `1px solid ${paused ? 'rgba(251,191,36,0.3)' : 'var(--pc-border)'}`,
            }}
          >
            {paused ? <Play className="h-3 w-3" /> : <Pause className="h-3 w-3" />}
            {paused ? 'Resume' : 'Pause'}
          </button>

          {/* Clear */}
          <button
            onClick={() => setLines([])}
            className="px-3 py-1.5 rounded-lg text-xs font-medium transition-colors"
            style={{ background: 'var(--pc-hover)', color: 'var(--pc-text-muted)', border: '1px solid var(--pc-border)' }}
            title="Clear log"
          >
            <Trash2 className="h-3 w-3" />
          </button>
        </div>
      </div>

      {/* Log area */}
      <div
        ref={containerRef}
        onScroll={handleScroll}
        className="flex-1 overflow-y-auto rounded-2xl p-3 font-mono text-xs"
        style={{ background: 'var(--pc-bg-elevated)', border: '1px solid var(--pc-border)' }}
      >
        {displayLines.length === 0 ? (
          <div className="flex flex-col items-center justify-center h-full gap-3 py-16"
            style={{ color: 'var(--pc-text-faint)' }}>
            <Radio className="h-10 w-10 opacity-20" />
            <p>No log lines yet…</p>
            <p className="text-xs opacity-60">Waiting for gateway.log data from the SSE stream.</p>
          </div>
        ) : (
          <div className="space-y-0.5">
            {displayLines.map((line) => (
              <div key={line.id} className="flex gap-2 py-0.5 hover:bg-white/[0.02] rounded px-1 transition-colors leading-snug">
                {line.ts && (
                  <span className="shrink-0 text-[10px] opacity-50" style={{ color: 'var(--pc-text-faint)', minWidth: '7rem' }}>
                    {line.ts.replace('T', ' ').replace('Z', '')}
                  </span>
                )}
                <span className="shrink-0 text-[10px] font-bold w-12 uppercase" style={{ color: levelColor(line.level) }}>
                  {line.level === 'UNKNOWN' ? '' : line.level}
                </span>
                <span className="flex-1 whitespace-pre-wrap break-all" style={{ color: 'var(--pc-text-secondary)' }}>
                  {line.ts ? line.body : line.raw}
                </span>
              </div>
            ))}
          </div>
        )}
      </div>

      {/* Scroll-to-bottom FAB */}
      {!autoScroll && (
        <button
          onClick={scrollToBottom}
          className="absolute bottom-24 right-10 p-2 rounded-full shadow-lg transition-all animate-fade-in"
          style={{ background: 'var(--pc-accent)', color: '#fff' }}
          title="Scroll to latest"
        >
          <ArrowDown className="h-4 w-4" />
        </button>
      )}
    </div>
  );
}

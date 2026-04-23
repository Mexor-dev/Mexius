import { useState, useEffect, useRef, useCallback, useMemo } from 'react';
import { Brain, Zap, Circle, ArrowDown, Trash2 } from 'lucide-react';
import type { NexusMessage } from '@/types/api';
import { getToken } from '@/lib/auth';
import { apiOrigin, basePath } from '@/lib/basePath';

interface NexusFrame {
  id: string;
  type: NexusMessage['type'];
  content: string;
  timestamp: Date;
}

function typeLabel(type: NexusMessage['type']): string {
  switch (type) {
    case 'nexus_connected': return 'CONNECTED';
    case 'thinking': return 'THINKING';
    case 'reasoning_start': return 'REASONING ▶';
    case 'reasoning_end': return 'REASONING ■';
    case 'heartbeat': return 'PULSE';
    default: return (type as string).toUpperCase();
  }
}

function typeStyle(type: NexusMessage['type']): { color: string; bg: string; border: string } {
  switch (type) {
    case 'nexus_connected':
      return { color: '#34d399', bg: 'rgba(52,211,153,0.06)', border: 'rgba(52,211,153,0.2)' };
    case 'thinking':
      return { color: '#a78bfa', bg: 'rgba(167,139,250,0.06)', border: 'rgba(167,139,250,0.2)' };
    case 'reasoning_start':
    case 'reasoning_end':
      return { color: '#38bdf8', bg: 'rgba(56,189,248,0.06)', border: 'rgba(56,189,248,0.2)' };
    case 'heartbeat':
      return { color: 'var(--pc-text-faint)', bg: 'transparent', border: 'transparent' };
    default:
      return { color: 'var(--pc-text-muted)', bg: 'var(--pc-hover)', border: 'var(--pc-border)' };
  }
}

const MAX_FRAMES = 500;

export default function Nexus() {
  const [frames, setFrames] = useState<NexusFrame[]>([]);
  const [connected, setConnected] = useState(false);
  const [autoScroll, setAutoScroll] = useState(true);
  const [showHeartbeats, setShowHeartbeats] = useState(false);
  const containerRef = useRef<HTMLDivElement>(null);
  const wsRef = useRef<WebSocket | null>(null);
  const intentionalRef = useRef(false);
  const reconnectTimer = useRef<ReturnType<typeof setTimeout> | null>(null);
  const frameIdRef = useRef(0);

  const displayFrames = useMemo(
    () => showHeartbeats ? frames : frames.filter((f) => f.type !== 'heartbeat'),
    [frames, showHeartbeats],
  );

  const connect = useCallback(() => {
    if (wsRef.current && wsRef.current.readyState < WebSocket.CLOSING) return;

    const proto = window.location.protocol === 'https:' ? 'wss:' : 'ws:';
    const origin = apiOrigin ? apiOrigin.replace(/^http/, 'ws') : `${proto}//${window.location.host}`;
    const token = getToken();
    const params = new URLSearchParams();
    if (token) params.set('token', token);
    const url = `${origin}${basePath}/ws/nexus?${params.toString()}`;

    const ws = new WebSocket(url, ['herma.v1']);
    wsRef.current = ws;

    ws.onopen = () => {
      setConnected(true);
    };

    ws.onmessage = (ev: MessageEvent) => {
      try {
        const msg = JSON.parse(ev.data) as NexusMessage;
        const frame: NexusFrame = {
          id: `f-${++frameIdRef.current}`,
          type: msg.type,
          content: msg.content ?? msg.message ?? (msg.tick !== undefined ? `tick ${msg.tick}` : ''),
          timestamp: new Date(),
        };
        setFrames((prev) => {
          const next = [...prev, frame];
          return next.length > MAX_FRAMES ? next.slice(next.length - MAX_FRAMES) : next;
        });
      } catch {
        // ignore non-JSON
      }
    };

    ws.onclose = () => {
      setConnected(false);
      if (!intentionalRef.current) {
        reconnectTimer.current = setTimeout(connect, 3000);
      }
    };

    ws.onerror = () => {
      ws.close();
    };
  }, []);

  useEffect(() => {
    intentionalRef.current = false;
    connect();
    return () => {
      intentionalRef.current = true;
      if (reconnectTimer.current) clearTimeout(reconnectTimer.current);
      wsRef.current?.close();
    };
  }, [connect]);

  // Auto-scroll to bottom
  useEffect(() => {
    if (!autoScroll) return;
    const el = containerRef.current;
    if (el) el.scrollTop = el.scrollHeight;
  }, [displayFrames, autoScroll]);

  const handleScroll = useCallback(() => {
    const el = containerRef.current;
    if (!el) return;
    const atBottom = el.scrollHeight - el.scrollTop - el.clientHeight < 40;
    setAutoScroll(atBottom);
  }, []);

  const scrollToBottom = () => {
    const el = containerRef.current;
    if (el) { el.scrollTop = el.scrollHeight; setAutoScroll(true); }
  };

  const clearFrames = () => setFrames([]);

  return (
    <div className="flex flex-col h-[calc(100vh-4rem)] p-6 gap-4 animate-fade-in">
      {/* Header */}
      <div className="flex items-center justify-between flex-shrink-0">
        <div className="flex items-center gap-3">
          <div className="p-2 rounded-2xl" style={{ background: 'rgba(167,139,250,0.1)', color: '#a78bfa' }}>
            <Brain className="h-5 w-5" />
          </div>
          <div>
            <h1 className="text-lg font-semibold" style={{ color: 'var(--pc-text-primary)' }}>Nexus</h1>
            <p className="text-xs" style={{ color: 'var(--pc-text-muted)' }}>Internal Chain of Thought</p>
          </div>
        </div>

        <div className="flex items-center gap-3">
          {/* Connection indicator */}
          <div className="flex items-center gap-2 px-3 py-1.5 rounded-lg text-xs font-medium"
            style={{
              background: connected ? 'rgba(52,211,153,0.1)' : 'rgba(239,68,68,0.1)',
              color: connected ? '#34d399' : '#f87171',
              border: `1px solid ${connected ? 'rgba(52,211,153,0.3)' : 'rgba(239,68,68,0.3)'}`,
            }}
          >
            <Circle className={`h-2 w-2 fill-current ${connected ? 'animate-pulse' : ''}`} />
            {connected ? 'Live' : 'Reconnecting…'}
          </div>

          {/* Show heartbeats toggle */}
          <button
            onClick={() => setShowHeartbeats((v) => !v)}
            className="px-3 py-1.5 rounded-lg text-xs font-medium transition-colors"
            style={{
              background: showHeartbeats ? 'var(--pc-accent-glow)' : 'var(--pc-hover)',
              color: showHeartbeats ? 'var(--pc-accent-light)' : 'var(--pc-text-muted)',
              border: `1px solid ${showHeartbeats ? 'var(--pc-accent-dim)' : 'var(--pc-border)'}`,
            }}
          >
            <Zap className="h-3 w-3 inline mr-1" />
            Pulses
          </button>

          {/* Clear */}
          <button
            onClick={clearFrames}
            className="px-3 py-1.5 rounded-lg text-xs font-medium transition-colors"
            style={{ background: 'var(--pc-hover)', color: 'var(--pc-text-muted)', border: '1px solid var(--pc-border)' }}
            title="Clear frames"
          >
            <Trash2 className="h-3 w-3" />
          </button>
        </div>
      </div>

      {/* Info banner */}
      <div className="rounded-2xl px-4 py-3 text-xs flex-shrink-0"
        style={{ background: 'rgba(167,139,250,0.06)', border: '1px solid rgba(167,139,250,0.15)', color: 'var(--pc-text-muted)' }}>
        <strong style={{ color: '#a78bfa' }}>Nexus</strong> streams internal reasoning frames from the agent's chain of thought.
        Frames tagged <span style={{ color: '#a78bfa' }}>thinking</span> are reasoning-only and never appear in Chat.
        Chat responses appear only in the Agent Chat page.
      </div>

      {/* Frame list */}
      <div
        ref={containerRef}
        onScroll={handleScroll}
        className="flex-1 overflow-y-auto rounded-2xl font-mono text-xs space-y-1 p-4"
        style={{ background: 'var(--pc-bg-elevated)', border: '1px solid var(--pc-border)' }}
      >
        {displayFrames.length === 0 ? (
          <div className="flex flex-col items-center justify-center h-full gap-3 py-16"
            style={{ color: 'var(--pc-text-faint)' }}>
            <Brain className="h-10 w-10 opacity-20" />
            <p>Waiting for reasoning frames…</p>
            <p className="text-xs opacity-60">Frames will appear here when the agent begins processing.</p>
          </div>
        ) : (
          displayFrames.map((frame) => {
            const s = typeStyle(frame.type);
            if (frame.type === 'heartbeat') {
              return (
                <div key={frame.id} className="flex items-center gap-2 py-0.5 opacity-30">
                  <span className="text-[10px]" style={{ color: s.color }}>
                    {frame.timestamp.toLocaleTimeString()}
                  </span>
                  <span className="text-[10px]" style={{ color: s.color }}>· · ·</span>
                </div>
              );
            }
            return (
              <div key={frame.id}
                className="flex gap-3 p-2.5 rounded-xl transition-all"
                style={{ background: s.bg, border: `1px solid ${s.border}` }}
              >
                <span className="shrink-0 text-[10px] mt-0.5 w-20 text-right" style={{ color: 'var(--pc-text-faint)' }}>
                  {frame.timestamp.toLocaleTimeString()}
                </span>
                <span className="shrink-0 text-[10px] mt-0.5 w-24 font-semibold tracking-wider uppercase" style={{ color: s.color }}>
                  {typeLabel(frame.type)}
                </span>
                <span className="flex-1 whitespace-pre-wrap break-words leading-relaxed" style={{ color: 'var(--pc-text-secondary)' }}>
                  {frame.content}
                </span>
              </div>
            );
          })
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

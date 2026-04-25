/**
 * Nexus — Mission Control Dashboard
 *
 * Three-pane layout when in Nexus mode:
 *  1. Main Task Pane    — Enter a high-level goal; delegate to named sub-agents
 *  2. Agent Logs Pane  — Real-time stream of agent-to-agent delegations
 *  3. Internal Monologue — Raw Chain-of-Thought from the supervisor (ws/nexus)
 *
 * Falls back to the classic single-pane CoT viewer when NOT in Nexus mode.
 */

import { useState, useEffect, useRef, useCallback, useMemo } from 'react';
import {
  Brain, Zap, Circle, Trash2, Network,
  Send, ChevronRight, User, Bot, AlertTriangle,
} from 'lucide-react';
import type { NexusMessage, NexusAgentEvent } from '@/types/api';
import { getToken } from '@/lib/auth';
import { apiOrigin, basePath } from '@/lib/basePath';
import { useSovereignty } from '@/contexts/SovereigntyContext';
import { delegateToNexusAgent, getOllamaVram } from '@/lib/api';
import type { OllamaRunningModel } from '@/types/api';

// ─── Types ───────────────────────────────────────────────────────────────────

interface NexusFrame {
  id: string;
  type: NexusMessage['type'] | NexusAgentEvent['type'];
  content: string;
  from?: string;
  to?: string;
  timestamp: Date;
}

type AgentDelegation = {
  id: string;
  fromAgent: string;
  toAgent: string;
  task: string;
  result?: string;
  timestamp: Date;
  pending: boolean;
};

// ─── Helpers ─────────────────────────────────────────────────────────────────

function typeLabel(type: NexusFrame['type']): string {
  switch (type) {
    case 'nexus_connected':   return 'CONNECTED';
    case 'thinking':          return 'THINKING';
    case 'reasoning_start':   return 'REASONING ▶';
    case 'reasoning_end':     return 'REASONING ■';
    case 'heartbeat':         return 'PULSE';
    case 'agent_delegation':  return 'DELEGATE';
    case 'agent_result':      return 'RESULT';
    default: return (type as string).toUpperCase();
  }
}

function typeStyle(type: NexusFrame['type']): { color: string; bg: string; border: string } {
  switch (type) {
    case 'nexus_connected':
      return { color: '#34d399', bg: 'rgba(52,211,153,0.06)', border: 'rgba(52,211,153,0.2)' };
    case 'thinking':
      return { color: '#a78bfa', bg: 'rgba(167,139,250,0.06)', border: 'rgba(167,139,250,0.2)' };
    case 'reasoning_start':
    case 'reasoning_end':
      return { color: '#38bdf8', bg: 'rgba(56,189,248,0.06)', border: 'rgba(56,189,248,0.2)' };
    case 'agent_delegation':
      return { color: '#fb923c', bg: 'rgba(251,146,60,0.06)', border: 'rgba(251,146,60,0.25)' };
    case 'agent_result':
      return { color: '#4ade80', bg: 'rgba(74,222,128,0.06)', border: 'rgba(74,222,128,0.25)' };
    case 'heartbeat':
      return { color: 'var(--pc-text-faint)', bg: 'transparent', border: 'transparent' };
    default:
      return { color: 'var(--pc-text-muted)', bg: 'var(--pc-hover)', border: 'var(--pc-border)' };
  }
}

const MAX_FRAMES = 500;
const MAX_DELEGATIONS = 100;

// Built-in sub-agent profiles for the Nexus dispatch pane
const BUILT_IN_AGENTS = [
  { name: 'Coder',      prompt: 'You are an expert software engineer. Analyze code, suggest implementations, and write high-quality Rust/TypeScript.', color: '#38bdf8' },
  { name: 'Strategist', prompt: 'You are a strategic analyst. Break down high-level goals into actionable steps and identify risks.', color: '#a78bfa' },
  { name: 'Researcher', prompt: 'You are a research specialist. Gather information, summarize findings, and cite sources.', color: '#34d399' },
  { name: 'Critic',     prompt: 'You are a critical reviewer. Identify flaws, edge cases, and improvements in any plan or implementation.', color: '#fb923c' },
];

// ─── WebSocket hook ───────────────────────────────────────────────────────────

function useNexusWebSocket() {
  const [frames, setFrames] = useState<NexusFrame[]>([]);
  const [connected, setConnected] = useState(false);
  const wsRef = useRef<WebSocket | null>(null);
  const intentionalRef = useRef(false);
  const reconnectTimer = useRef<ReturnType<typeof setTimeout> | null>(null);
  const frameIdRef = useRef(0);

  const connect = useCallback(() => {
    if (wsRef.current && wsRef.current.readyState < WebSocket.CLOSING) return;
    const proto = window.location.protocol === 'https:' ? 'wss:' : 'ws:';
    const origin = apiOrigin ? apiOrigin.replace(/^http/, 'ws') : `${proto}//${window.location.host}`;
    const token = getToken();
    const params = new URLSearchParams();
    if (token) params.set('token', token);
    const url = `${origin}${basePath}/ws/nexus?${params.toString()}`;
    const ws = new WebSocket(url, ['mexius.v1']);
    wsRef.current = ws;

    ws.onopen = () => setConnected(true);
    ws.onmessage = (ev) => {
      try {
        const msg = JSON.parse(ev.data) as NexusAgentEvent;
        const frame: NexusFrame = {
          id: `f-${++frameIdRef.current}`,
          type: msg.type as NexusFrame['type'],
          content: msg.task ?? msg.result ?? msg.message ?? (msg.tick !== undefined ? `tick ${msg.tick}` : ''),
          from: msg.from_agent,
          to: msg.to_agent,
          timestamp: new Date(),
        };
        setFrames((prev) => {
          const next = [...prev, frame];
          return next.length > MAX_FRAMES ? next.slice(next.length - MAX_FRAMES) : next;
        });
      } catch { /* ignore */ }
    };
    ws.onclose = () => {
      setConnected(false);
      if (!intentionalRef.current) reconnectTimer.current = setTimeout(connect, 3000);
    };
    ws.onerror = () => ws.close();
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

  const clear = useCallback(() => setFrames([]), []);
  return { frames, connected, clear };
}

// ─── Classic Single-Pane View ─────────────────────────────────────────────────

function ClassicView({ frames, connected, onClear }: { frames: NexusFrame[]; connected: boolean; onClear: () => void }) {
  const [showHeartbeats, setShowHeartbeats] = useState(false);
  const [autoScroll, setAutoScroll] = useState(true);
  const containerRef = useRef<HTMLDivElement>(null);

  const displayFrames = useMemo(
    () => showHeartbeats ? frames : frames.filter((f) => f.type !== 'heartbeat'),
    [frames, showHeartbeats],
  );

  useEffect(() => {
    if (autoScroll && containerRef.current) containerRef.current.scrollTop = containerRef.current.scrollHeight;
  }, [displayFrames, autoScroll]);

  return (
    <div className="flex flex-col h-full gap-4">
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
        <div className="flex items-center gap-2">
          <StatusBadge connected={connected} />
          <button onClick={() => setShowHeartbeats((v) => !v)}
            className="px-3 py-1.5 rounded-lg text-xs font-medium transition-colors"
            style={{ background: showHeartbeats ? 'var(--pc-accent-glow)' : 'var(--pc-hover)', color: showHeartbeats ? 'var(--pc-accent-light)' : 'var(--pc-text-muted)', border: `1px solid ${showHeartbeats ? 'var(--pc-accent-dim)' : 'var(--pc-border)'}` }}>
            <Zap className="h-3 w-3 inline mr-1" />Pulses
          </button>
          <button onClick={onClear} className="px-3 py-1.5 rounded-lg text-xs font-medium"
            style={{ background: 'var(--pc-hover)', color: 'var(--pc-text-muted)', border: '1px solid var(--pc-border)' }}>
            <Trash2 className="h-3 w-3" />
          </button>
        </div>
      </div>

      <div className="rounded-2xl px-4 py-3 text-xs flex-shrink-0"
        style={{ background: 'rgba(167,139,250,0.06)', border: '1px solid rgba(167,139,250,0.15)', color: 'var(--pc-text-muted)' }}>
        <strong style={{ color: '#a78bfa' }}>Nexus</strong> streams internal reasoning frames.
        Switch to <strong style={{ color: '#a78bfa' }}>Nexus mode</strong> via the header toggle for multi-agent Mission Control.
      </div>

      <div ref={containerRef}
        onScroll={(e) => { const el = e.currentTarget; setAutoScroll(el.scrollHeight - el.scrollTop - el.clientHeight < 40); }}
        className="flex-1 overflow-y-auto rounded-2xl font-mono text-xs space-y-1 p-4"
        style={{ background: 'var(--pc-bg-elevated)', border: '1px solid var(--pc-border)' }}>
        {displayFrames.length === 0 ? (
          <EmptyState icon={<Brain className="h-10 w-10 opacity-20" />} message="Waiting for reasoning frames…" />
        ) : displayFrames.map((frame) => <FrameRow key={frame.id} frame={frame} />)}
      </div>
    </div>
  );
}

// ─── VRAM Status Hook ───────────────────────────────────────────────────────
function useVramStatus() {
  const [models, setModels] = useState<OllamaRunningModel[]>([]);
  useEffect(() => {
    let mounted = true;
    const poll = async () => {
      try {
        const data = await getOllamaVram();
        if (mounted) setModels(data.models ?? []);
      } catch { /* Ollama may not be running */ }
    };
    poll();
    const id = setInterval(poll, 10_000);
    return () => { mounted = false; clearInterval(id); };
  }, []);
  return models;
}

// ─── VRAM Bar ─────────────────────────────────────────────────────────────────
function VramBar({ models }: { models: OllamaRunningModel[] }) {
  if (models.length === 0) return null;
  const fmt = (b: number) => b > 1e9 ? `${(b / 1e9).toFixed(1)} GB` : `${(b / 1e6).toFixed(0)} MB`;
  return (
    <div style={{ padding: '8px 12px', borderRadius: '12px', background: 'rgba(167,139,250,0.06)', border: '1px solid rgba(167,139,250,0.2)', marginBottom: '4px', flexShrink: 0 }}>
      <p style={{ fontSize: '10px', letterSpacing: '0.12em', textTransform: 'uppercase', color: 'rgba(167,139,250,0.55)', margin: '0 0 6px 0' }}>Active VRAM Models</p>
      <div style={{ display: 'flex', flexWrap: 'wrap', gap: '8px' }}>
        {models.map((m) => {
          const pct = m.size > 0 ? Math.round((m.size_vram / m.size) * 100) : 0;
          return (
            <div key={m.name} style={{ display: 'flex', flexDirection: 'column', gap: '3px', minWidth: '140px', flex: '1 1 140px' }}>
              <div style={{ display: 'flex', justifyContent: 'space-between', alignItems: 'baseline' }}>
                <span style={{ fontSize: '11px', fontWeight: 600, color: '#fbbf24', maxWidth: '120px', overflow: 'hidden', textOverflow: 'ellipsis', whiteSpace: 'nowrap' }}>{m.name}</span>
                <span style={{ fontSize: '10px', color: 'rgba(167,139,250,0.7)' }}>{fmt(m.size_vram)}</span>
              </div>
              <div style={{ height: '4px', borderRadius: '2px', background: 'rgba(167,139,250,0.15)', overflow: 'hidden' }}>
                <div style={{ height: '100%', width: `${pct}%`, borderRadius: '2px', background: 'linear-gradient(90deg, #a78bfa, #fbbf24)', transition: 'width 0.5s ease' }} />
              </div>
            </div>
          );
        })}
      </div>
    </div>
  );
}

// ─── Nexus Mission Control View ───────────────────────────────────────────────

function NexusView({ frames, connected, onClear }: { frames: NexusFrame[]; connected: boolean; onClear: () => void }) {
  const vramModels = useVramStatus();
  const [task, setTask] = useState('');
  const [selectedAgent, setSelectedAgent] = useState(BUILT_IN_AGENTS[0]?.name ?? 'Coder');
  const [delegating, setDelegating] = useState(false);
  const [delegationError, setDelegationError] = useState<string | null>(null);
  const [delegations, setDelegations] = useState<AgentDelegation[]>([]);
  const logsEndRef = useRef<HTMLDivElement>(null);
  const cotEndRef = useRef<HTMLDivElement>(null);

  // Parse delegation events from WS frames
  useEffect(() => {
    const lastFrame = frames[frames.length - 1];
    if (!lastFrame) return;
    if (lastFrame.type === 'agent_delegation') {
      setDelegations((prev) => {
        const id = `${lastFrame.from}-${lastFrame.to}-${lastFrame.timestamp.getTime()}`;
        if (prev.find((d) => d.id === id)) return prev;
        const entry: AgentDelegation = { id, fromAgent: lastFrame.from ?? 'Supervisor', toAgent: lastFrame.to ?? 'Agent', task: lastFrame.content, timestamp: lastFrame.timestamp, pending: true };
        const next = [...prev, entry];
        return next.length > MAX_DELEGATIONS ? next.slice(next.length - MAX_DELEGATIONS) : next;
      });
    } else if (lastFrame.type === 'agent_result') {
      setDelegations((prev) => prev.map((d) =>
        d.toAgent === lastFrame.from && d.pending ? { ...d, result: lastFrame.content, pending: false } : d,
      ));
    }
  }, [frames]);

  useEffect(() => { logsEndRef.current?.scrollIntoView({ behavior: 'smooth' }); }, [delegations]);
  useEffect(() => { cotEndRef.current?.scrollIntoView({ behavior: 'smooth' }); }, [frames]);

  const handleDelegate = async () => {
    if (!task.trim()) return;
    setDelegating(true);
    setDelegationError(null);
    try {
      const agent = BUILT_IN_AGENTS.find((a) => a.name === selectedAgent);
      await delegateToNexusAgent(selectedAgent, task.trim(), agent?.prompt ?? '');
      setTask('');
    } catch (e: unknown) {
      setDelegationError(e instanceof Error ? e.message : 'Delegation failed');
    } finally {
      setDelegating(false);
    }
  };

  const cotFrames = useMemo(
    () => frames.filter((f) => !['agent_delegation', 'agent_result', 'heartbeat'].includes(f.type as string)),
    [frames],
  );

  return (
    <div className="flex flex-col h-full gap-4">
      <div className="flex items-center justify-between flex-shrink-0">
        <div className="flex items-center gap-3">
          <div className="p-2 rounded-2xl" style={{ background: 'rgba(167,139,250,0.1)', color: '#a78bfa' }}>
            <Network className="h-5 w-5" />
          </div>
          <div>
            <h1 className="text-lg font-semibold" style={{ color: 'var(--pc-text-primary)' }}>Nexus Mission Control</h1>
            <p className="text-xs" style={{ color: 'var(--pc-text-muted)' }}>Multi-agent orchestration dashboard</p>
          </div>
        </div>
        <div className="flex items-center gap-2">
          <StatusBadge connected={connected} />
          <button onClick={onClear} className="px-3 py-1.5 rounded-lg text-xs font-medium"
            style={{ background: 'var(--pc-hover)', color: 'var(--pc-text-muted)', border: '1px solid var(--pc-border)' }}>
            <Trash2 className="h-3 w-3" />
          </button>
        </div>
      </div>

      <VramBar models={vramModels} />

      {/* ── Agent Bento Cards ─────────────────────────────────────────── */}
      <div style={{ display: 'grid', gridTemplateColumns: 'repeat(auto-fill, minmax(140px, 1fr))', gap: '8px', flexShrink: 0 }}>
        {BUILT_IN_AGENTS.map((a) => {
          const isActive = delegations.some((d) => d.toAgent === a.name && d.pending);
          const lastResult = [...delegations].reverse().find((d) => d.toAgent === a.name && !d.pending);
          return (
            <button
              key={a.name}
              type="button"
              onClick={() => setSelectedAgent(a.name)}
              style={{
                display: 'flex', flexDirection: 'column', gap: '6px',
                padding: '12px',
                background: selectedAgent === a.name ? `${a.color}10` : '#0d0d0d',
                border: `1px solid ${isActive ? a.color : selectedAgent === a.name ? `${a.color}50` : 'rgba(255,255,255,0.07)'}`,
                borderRadius: '2px',
                cursor: 'pointer',
                textAlign: 'left',
                transition: 'all 0.2s ease',
                boxShadow: isActive ? `0 0 10px ${a.color}30` : 'none',
              }}
            >
              <div style={{ display: 'flex', alignItems: 'center', justifyContent: 'space-between' }}>
                <span style={{ fontSize: '11px', fontWeight: 700, letterSpacing: '0.1em', color: a.color }}>
                  {a.name.toUpperCase()}
                </span>
                {isActive && (
                  <span style={{ width: '6px', height: '6px', borderRadius: '50%', background: a.color, boxShadow: `0 0 6px ${a.color}` }} className="animate-pulse" />
                )}
              </div>
              <span style={{ fontSize: '10px', color: 'rgba(255,255,255,0.3)', letterSpacing: '0.05em' }}>
                {isActive ? 'ACTIVE' : lastResult ? 'IDLE' : 'READY'}
              </span>
            </button>
          );
        })}
      </div>

      {/* Three-pane grid */}
      <div className="flex-1 grid grid-cols-1 lg:grid-cols-3 gap-4 min-h-0 overflow-hidden">

        {/* Pane 1: Main Task Dispatch */}
        <div className="flex flex-col rounded-2xl overflow-hidden" style={{ background: 'var(--pc-bg-elevated)', border: '1px solid var(--pc-border)' }}>
          <PaneHeader label="Main Task" icon={<Send className="h-3.5 w-3.5" />} />
          <div className="flex flex-col gap-3 p-4 flex-1">
            <div>
              <label className="block text-xs font-medium mb-1.5" style={{ color: 'var(--pc-text-muted)' }}>Delegate to</label>
              <div className="flex flex-wrap gap-1.5">
                {BUILT_IN_AGENTS.map((a) => (
                  <button key={a.name} type="button" onClick={() => setSelectedAgent(a.name)}
                    className="px-2.5 py-1 rounded-lg text-xs font-medium transition-all"
                    style={{ background: selectedAgent === a.name ? `${a.color}20` : 'var(--pc-hover)', border: `1px solid ${selectedAgent === a.name ? a.color + '60' : 'var(--pc-border)'}`, color: selectedAgent === a.name ? a.color : 'var(--pc-text-muted)' }}>
                    {a.name}
                  </button>
                ))}
              </div>
            </div>
            <div className="flex-1 flex flex-col gap-2">
              <label className="block text-xs font-medium" style={{ color: 'var(--pc-text-muted)' }}>Task</label>
              <textarea value={task} onChange={(e) => setTask(e.target.value)}
                onKeyDown={(e) => { if (e.key === 'Enter' && (e.metaKey || e.ctrlKey)) handleDelegate(); }}
                placeholder={`Describe what ${selectedAgent} should do…`}
                className="flex-1 resize-none rounded-xl p-3 text-xs font-mono leading-relaxed"
                style={{ background: 'var(--pc-bg-base)', border: '1px solid var(--pc-border)', color: 'var(--pc-text-primary)', minHeight: '120px' }}
                rows={6} />
            </div>
            {delegationError && (
              <div className="flex items-center gap-2 rounded-lg px-3 py-2 text-xs" style={{ background: 'rgba(239,68,68,0.1)', border: '1px solid rgba(239,68,68,0.3)', color: '#f87171' }}>
                <AlertTriangle className="h-3.5 w-3.5 shrink-0" />{delegationError}
              </div>
            )}
            <button onClick={handleDelegate} disabled={delegating || !task.trim()}
              className="flex items-center justify-center gap-2 py-2.5 rounded-xl text-xs font-semibold transition-all"
              style={{ background: delegating || !task.trim() ? 'var(--pc-hover)' : 'rgba(167,139,250,0.15)', border: '1px solid rgba(167,139,250,0.3)', color: delegating || !task.trim() ? 'var(--pc-text-faint)' : '#a78bfa', cursor: delegating || !task.trim() ? 'not-allowed' : 'pointer' }}>
              {delegating ? <span className="h-3.5 w-3.5 border border-current border-t-transparent rounded-full animate-spin" /> : <ChevronRight className="h-3.5 w-3.5" />}
              {delegating ? 'Delegating…' : `Delegate to ${selectedAgent}`}
            </button>
          </div>
        </div>

        {/* Pane 2: Agent Logs */}
        <div className="flex flex-col rounded-2xl overflow-hidden" style={{ background: 'var(--pc-bg-elevated)', border: '1px solid var(--pc-border)' }}>
          <PaneHeader label="Agent Log" icon={<Bot className="h-3.5 w-3.5" />} count={delegations.length} />
          <div className="flex-1 overflow-y-auto p-3 space-y-2 font-mono text-xs">
            {delegations.length === 0 ? (
              <EmptyState icon={<User className="h-8 w-8 opacity-20" />} message="No delegations yet" />
            ) : delegations.map((d) => (
              <div key={d.id} className="rounded-xl p-2.5 space-y-1.5"
                style={{ background: d.pending ? 'rgba(251,146,60,0.06)' : 'rgba(74,222,128,0.06)', border: `1px solid ${d.pending ? 'rgba(251,146,60,0.2)' : 'rgba(74,222,128,0.2)'}` }}>
                <div className="flex items-center gap-1.5 text-[10px]">
                  <span className="font-semibold" style={{ color: '#a78bfa' }}>{d.fromAgent}</span>
                  <ChevronRight className="h-2.5 w-2.5" style={{ color: 'var(--pc-text-faint)' }} />
                  <span className="font-semibold" style={{ color: '#38bdf8' }}>{d.toAgent}</span>
                  <span className="ml-auto" style={{ color: 'var(--pc-text-faint)' }}>{d.timestamp.toLocaleTimeString()}</span>
                </div>
                <p className="text-[11px] leading-relaxed whitespace-pre-wrap break-words" style={{ color: 'var(--pc-text-secondary)' }}>{d.task}</p>
                {d.result && (
                  <div className="rounded-lg px-2.5 py-1.5 mt-1" style={{ background: 'rgba(74,222,128,0.08)', border: '1px solid rgba(74,222,128,0.15)' }}>
                    <p className="text-[10px] font-semibold mb-0.5" style={{ color: '#4ade80' }}>↩ Result</p>
                    <p className="text-[11px] leading-relaxed whitespace-pre-wrap break-words" style={{ color: 'var(--pc-text-secondary)' }}>{d.result}</p>
                  </div>
                )}
                {d.pending && (
                  <div className="flex items-center gap-1.5 text-[10px]" style={{ color: '#fb923c' }}>
                    <span className="h-2 w-2 rounded-full bg-current animate-pulse" />
                    Waiting for {d.toAgent}…
                  </div>
                )}
              </div>
            ))}
            <div ref={logsEndRef} />
          </div>
        </div>

        {/* Pane 3: Internal Monologue */}
        <div className="flex flex-col rounded-2xl overflow-hidden" style={{ background: 'var(--pc-bg-elevated)', border: '1px solid var(--pc-border)' }}>
          <PaneHeader label="Internal Monologue" icon={<Brain className="h-3.5 w-3.5" />} count={cotFrames.length} />
          <div className="flex-1 overflow-y-auto p-3 space-y-1 font-mono text-xs">
            {cotFrames.length === 0 ? (
              <EmptyState icon={<Brain className="h-8 w-8 opacity-20" />} message="Awaiting reasoning frames…" />
            ) : cotFrames.map((frame) => <FrameRow key={frame.id} frame={frame} compact />)}
            <div ref={cotEndRef} />
          </div>
        </div>
      </div>
    </div>
  );
}

// ─── Shared sub-components ────────────────────────────────────────────────────

function StatusBadge({ connected }: { connected: boolean }) {
  return (
    <div className="flex items-center gap-2 px-3 py-1.5 rounded-lg text-xs font-medium"
      style={{ background: connected ? 'rgba(52,211,153,0.1)' : 'rgba(239,68,68,0.1)', color: connected ? '#34d399' : '#f87171', border: `1px solid ${connected ? 'rgba(52,211,153,0.3)' : 'rgba(239,68,68,0.3)'}` }}>
      <Circle className={`h-2 w-2 fill-current ${connected ? 'animate-pulse' : ''}`} />
      {connected ? 'Live' : 'Reconnecting…'}
    </div>
  );
}

function PaneHeader({ label, icon, count }: { label: string; icon: React.ReactNode; count?: number }) {
  return (
    <div className="flex items-center justify-between px-4 py-2.5 border-b flex-shrink-0"
      style={{ borderColor: 'var(--pc-border)', background: 'var(--pc-bg-surface)' }}>
      <div className="flex items-center gap-2" style={{ color: 'var(--pc-text-muted)' }}>
        {icon}
        <span className="text-xs font-semibold uppercase tracking-wider">{label}</span>
      </div>
      {count !== undefined && (
        <span className="text-[10px] px-1.5 py-0.5 rounded-md font-mono"
          style={{ background: 'var(--pc-hover)', color: 'var(--pc-text-faint)' }}>{count}</span>
      )}
    </div>
  );
}

function EmptyState({ icon, message }: { icon: React.ReactNode; message: string }) {
  return (
    <div className="flex flex-col items-center justify-center h-full gap-3 py-12" style={{ color: 'var(--pc-text-faint)' }}>
      {icon}
      <p className="text-xs">{message}</p>
    </div>
  );
}

function FrameRow({ frame, compact = false }: { frame: NexusFrame; compact?: boolean }) {
  const s = typeStyle(frame.type);
  if (frame.type === 'heartbeat') {
    return (
      <div className="flex items-center gap-2 py-0.5 opacity-30">
        <span className="text-[10px]" style={{ color: s.color }}>{frame.timestamp.toLocaleTimeString()}</span>
        <span className="text-[10px]" style={{ color: s.color }}>· · ·</span>
      </div>
    );
  }
  return (
    <div className={`flex gap-2 ${compact ? 'p-1.5' : 'p-2.5'} rounded-xl`}
      style={{ background: s.bg, border: `1px solid ${s.border}` }}>
      {!compact && (
        <span className="shrink-0 text-[10px] mt-0.5 w-20 text-right" style={{ color: 'var(--pc-text-faint)' }}>
          {frame.timestamp.toLocaleTimeString()}
        </span>
      )}
      <span className={`shrink-0 text-[10px] mt-0.5 ${compact ? 'w-16' : 'w-24'} font-semibold tracking-wider uppercase`} style={{ color: s.color }}>
        {typeLabel(frame.type)}
      </span>
      <span className="flex-1 whitespace-pre-wrap break-words leading-relaxed" style={{ color: 'var(--pc-text-secondary)' }}>
        {frame.content}
      </span>
    </div>
  );
}

// ─── Main Export ──────────────────────────────────────────────────────────────

export default function Nexus() {
  const { state } = useSovereignty();
  const { frames, connected, clear } = useNexusWebSocket();

  return (
    <div className="flex flex-col h-[calc(100vh-4rem)] p-6 gap-4 animate-fade-in">
      {state === 'nexus' ? (
        <NexusView frames={frames} connected={connected} onClear={clear} />
      ) : (
        <ClassicView frames={frames} connected={connected} onClear={clear} />
      )}
    </div>
  );
}


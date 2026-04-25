/**
 * ModelMesh — Dynamic Model Registry
 *
 * Manage the live set of models available to the Nexus orchestrator.
 * Models are persisted server-side in ~/.mexius/model_registry.json.
 */

import { useState, useEffect, useCallback } from 'react';
import { Cpu, Plus, Trash2, RefreshCw, ToggleLeft, ToggleRight, Globe, Edit3, Check, X, Copy, ChevronDown, ChevronUp } from 'lucide-react';
import type { RegisteredModel, ModelSource, RegisterModelRequest } from '@/types/api';
import {
  getRegisteredModels,
  registerModel,
  deleteRegisteredModel,
  patchRegisteredModel,
  getSupervisorPrompt,
} from '@/lib/api';

// ─── Constants ────────────────────────────────────────────────────────────────

const SOURCE_OPTIONS: { value: ModelSource; label: string; color: string }[] = [
  { value: 'ollama',    label: 'Ollama',    color: '#34d399' },
  { value: 'openai',   label: 'OpenAI',    color: '#60a5fa' },
  { value: 'anthropic',label: 'Anthropic', color: '#f59e0b' },
  { value: 'custom',   label: 'Custom',    color: '#a78bfa' },
];

const DEFAULT_ENDPOINT: Record<ModelSource, string> = {
  ollama:    'http://127.0.0.1:11434',
  openai:    'https://api.openai.com/v1',
  anthropic: 'https://api.anthropic.com/v1',
  custom:    '',
};

function sourceColor(source: ModelSource): string {
  return SOURCE_OPTIONS.find((s) => s.value === source)?.color ?? '#a78bfa';
}

// ─── Add Model Form ───────────────────────────────────────────────────────────

function AddModelForm({ onAdded }: { onAdded: () => void }) {
  const [open, setOpen] = useState(false);
  const [source, setSource] = useState<ModelSource>('ollama');
  const [customName, setCustomName] = useState('');
  const [displayName, setDisplayName] = useState('');
  const [modelId, setModelId] = useState('');
  const [apiEndpoint, setApiEndpoint] = useState(DEFAULT_ENDPOINT['ollama']);
  const [apiKey, setApiKey] = useState('');
  const [submitting, setSubmitting] = useState(false);
  const [error, setError] = useState<string | null>(null);

  const handleSourceChange = (s: ModelSource) => {
    setSource(s);
    setApiEndpoint(DEFAULT_ENDPOINT[s]);
  };

  const handleSubmit = async (e: React.FormEvent) => {
    e.preventDefault();
    if (!customName.trim() || !modelId.trim() || !apiEndpoint.trim()) return;
    setSubmitting(true);
    setError(null);
    try {
      const req: RegisterModelRequest = {
        custom_name: customName.trim(),
        ...(displayName.trim() ? { display_name: displayName.trim() } : {}),
        model_id: modelId.trim(),
        api_endpoint: apiEndpoint.trim(),
        source,
        ...(apiKey.trim() ? { api_key: apiKey.trim() } : {}),
      };
      await registerModel(req);
      setCustomName(''); setDisplayName(''); setModelId(''); setApiKey('');
      setSource('ollama'); setApiEndpoint(DEFAULT_ENDPOINT['ollama']);
      setOpen(false);
      onAdded();
    } catch (e: unknown) {
      setError(e instanceof Error ? e.message : 'Failed to register model');
    } finally {
      setSubmitting(false);
    }
  };

  if (!open) {
    return (
      <button onClick={() => setOpen(true)}
        className="flex items-center gap-2 px-4 py-2.5 rounded-xl text-sm font-semibold transition-all"
        style={{ background: 'rgba(167,139,250,0.15)', border: '1px solid rgba(167,139,250,0.3)', color: '#a78bfa' }}>
        <Plus className="h-4 w-4" /> Add Model
      </button>
    );
  }

  return (
    <div className="rounded-2xl p-5 space-y-4" style={{ background: 'var(--pc-bg-elevated)', border: '1px solid rgba(167,139,250,0.3)' }}>
      <div className="flex items-center justify-between">
        <h3 className="text-sm font-semibold" style={{ color: 'var(--pc-text-primary)' }}>Register New Model</h3>
        <button onClick={() => setOpen(false)} style={{ color: 'var(--pc-text-faint)' }}><X className="h-4 w-4" /></button>
      </div>

      <form onSubmit={handleSubmit} className="space-y-3">
        {/* Source selector */}
        <div>
          <label className="block text-xs font-medium mb-1.5" style={{ color: 'var(--pc-text-muted)' }}>Source</label>
          <div className="flex flex-wrap gap-2">
            {SOURCE_OPTIONS.map((s) => (
              <button key={s.value} type="button" onClick={() => handleSourceChange(s.value)}
                className="px-3 py-1.5 rounded-lg text-xs font-medium transition-all"
                style={{
                  background: source === s.value ? `${s.color}20` : 'var(--pc-hover)',
                  border: `1px solid ${source === s.value ? s.color + '60' : 'var(--pc-border)'}`,
                  color: source === s.value ? s.color : 'var(--pc-text-muted)',
                }}>
                {s.label}
              </button>
            ))}
          </div>
        </div>

        {/* Two-column fields */}
        <div className="grid grid-cols-1 sm:grid-cols-2 gap-3">
          <div>
            <label className="block text-xs font-medium mb-1" style={{ color: 'var(--pc-text-muted)' }}>Display Name</label>
            <input value={customName} onChange={(e) => setCustomName(e.target.value)} required
              placeholder="e.g. Trevor-Coder"
              className="w-full px-3 py-2 rounded-xl text-sm"
              style={{ background: 'var(--pc-bg-base)', border: '1px solid var(--pc-border)', color: 'var(--pc-text-primary)' }} />
          </div>
          <div>
            <label className="block text-xs font-medium mb-1" style={{ color: 'var(--pc-text-muted)' }}>Label (optional)</label>
            <input value={displayName} onChange={(e) => setDisplayName(e.target.value)}
              placeholder="e.g. Coder · Fast"
              className="w-full px-3 py-2 rounded-xl text-sm"
              style={{ background: 'var(--pc-bg-base)', border: '1px solid var(--pc-border)', color: 'var(--pc-text-primary)' }} />
          </div>
          <div>
            <label className="block text-xs font-medium mb-1" style={{ color: 'var(--pc-text-muted)' }}>Model ID</label>
            <input value={modelId} onChange={(e) => setModelId(e.target.value)} required
              placeholder="e.g. gemma-trevor:latest"
              className="w-full px-3 py-2 rounded-xl text-sm"
              style={{ background: 'var(--pc-bg-base)', border: '1px solid var(--pc-border)', color: 'var(--pc-text-primary)' }} />
          </div>
        </div>

        <div>
          <label className="block text-xs font-medium mb-1" style={{ color: 'var(--pc-text-muted)' }}>
            <Globe className="h-3 w-3 inline mr-1" />API Endpoint
          </label>
          <input value={apiEndpoint} onChange={(e) => setApiEndpoint(e.target.value)} required
            placeholder="http://..."
            className="w-full px-3 py-2 rounded-xl text-sm font-mono"
            style={{ background: 'var(--pc-bg-base)', border: '1px solid var(--pc-border)', color: 'var(--pc-text-primary)' }} />
        </div>

        {source !== 'ollama' && (
          <div>
            <label className="block text-xs font-medium mb-1" style={{ color: 'var(--pc-text-muted)' }}>API Key (optional)</label>
            <input type="password" value={apiKey} onChange={(e) => setApiKey(e.target.value)}
              placeholder="sk-..."
              className="w-full px-3 py-2 rounded-xl text-sm font-mono"
              style={{ background: 'var(--pc-bg-base)', border: '1px solid var(--pc-border)', color: 'var(--pc-text-primary)' }} />
          </div>
        )}

        {error && (
          <p className="text-xs" style={{ color: '#f87171' }}>{error}</p>
        )}

        <div className="flex justify-end gap-2 pt-1">
          <button type="button" onClick={() => setOpen(false)}
            className="px-3 py-2 rounded-xl text-xs font-medium"
            style={{ background: 'var(--pc-hover)', color: 'var(--pc-text-muted)', border: '1px solid var(--pc-border)' }}>
            Cancel
          </button>
          <button type="submit" disabled={submitting}
            className="flex items-center gap-2 px-4 py-2 rounded-xl text-xs font-semibold"
            style={{ background: 'rgba(167,139,250,0.15)', border: '1px solid rgba(167,139,250,0.3)', color: '#a78bfa', opacity: submitting ? 0.6 : 1 }}>
            {submitting ? <span className="h-3.5 w-3.5 border border-current border-t-transparent rounded-full animate-spin" /> : <Check className="h-3.5 w-3.5" />}
            Register
          </button>
        </div>
      </form>
    </div>
  );
}

// ─── Model Row ────────────────────────────────────────────────────────────────

function ModelRow({ model, onDelete, onToggle, onRename }: {
  model: RegisteredModel;
  onDelete: (id: string) => void;
  onToggle: (id: string, active: boolean) => void;
  onRename: (id: string, name: string) => void;
}) {
  const [editing, setEditing] = useState(false);
  const [editName, setEditName] = useState(model.custom_name);
  const [deleting, setDeleting] = useState(false);

  const handleRename = async () => {
    if (editName.trim() && editName !== model.custom_name) {
      onRename(model.id, editName.trim());
    }
    setEditing(false);
  };

  const handleDelete = async () => {
    if (!window.confirm(`Remove "${model.custom_name}" from the registry?`)) return;
    setDeleting(true);
    onDelete(model.id);
  };

  const color = sourceColor(model.source);

  return (
    <div className="flex items-center gap-3 px-4 py-3 rounded-xl transition-all"
      style={{ background: model.is_active ? 'var(--pc-bg-elevated)' : 'transparent', border: `1px solid ${model.is_active ? 'var(--pc-border)' : 'transparent'}`, opacity: model.is_active ? 1 : 0.55 }}>

      {/* Active toggle */}
      <button onClick={() => onToggle(model.id, !model.is_active)} title={model.is_active ? 'Deactivate' : 'Activate'}>
        {model.is_active
          ? <ToggleRight className="h-5 w-5" style={{ color }} />
          : <ToggleLeft className="h-5 w-5" style={{ color: 'var(--pc-text-faint)' }} />}
      </button>

      {/* Source badge */}
      <span className="text-[10px] px-2 py-0.5 rounded-md font-semibold uppercase tracking-wider flex-shrink-0"
        style={{ background: `${color}18`, color, border: `1px solid ${color}40` }}>
        {model.source}
      </span>

      {/* Name (editable) */}
      {editing ? (
        <input value={editName} onChange={(e) => setEditName(e.target.value)}
          onBlur={handleRename} onKeyDown={(e) => { if (e.key === 'Enter') handleRename(); if (e.key === 'Escape') { setEditName(model.custom_name); setEditing(false); } }}
          autoFocus className="flex-1 min-w-0 px-2 py-0.5 rounded-lg text-sm"
          style={{ background: 'var(--pc-bg-base)', border: '1px solid var(--pc-accent-dim)', color: 'var(--pc-text-primary)' }} />
      ) : (
        <div className="flex items-center gap-1.5 flex-1 min-w-0">
          <span className="text-sm font-medium truncate" style={{ color: 'var(--pc-text-primary)' }}>{model.custom_name}</span>
          {model.display_name && (
            <span className="text-xs px-1.5 py-0.5 shrink-0" style={{ background: 'rgba(212,175,55,0.08)', color: 'rgba(212,175,55,0.7)', border: '1px solid rgba(212,175,55,0.2)', borderRadius: '2px', letterSpacing: '0.04em' }}>
              {model.display_name}
            </span>
          )}
          <button onClick={() => setEditing(true)} className="opacity-0 group-hover:opacity-100 shrink-0"
            style={{ color: 'var(--pc-text-faint)' }}><Edit3 className="h-3 w-3" /></button>
        </div>
      )}

      {/* Model ID */}
      <span className="text-xs font-mono truncate max-w-[180px] hidden sm:block" style={{ color: 'var(--pc-text-muted)' }}>
        {model.model_id}
      </span>

      {/* Endpoint */}
      <span className="text-xs font-mono truncate max-w-[200px] hidden md:block" style={{ color: 'var(--pc-text-faint)' }}>
        {model.api_endpoint}
      </span>

      {/* Delete */}
      <button onClick={handleDelete} disabled={deleting} title="Remove"
        className="ml-auto shrink-0 p-1.5 rounded-lg transition-colors hover:bg-red-900/20"
        style={{ color: deleting ? 'var(--pc-text-faint)' : 'var(--pc-text-faint)' }}>
        <Trash2 className="h-3.5 w-3.5" />
      </button>
    </div>
  );
}

// ─── Supervisor Prompt Panel ──────────────────────────────────────────────────

function SupervisorPromptPanel() {
  const [prompt, setPrompt] = useState<string | null>(null);
  const [open, setOpen] = useState(false);
  const [copied, setCopied] = useState(false);

  useEffect(() => {
    getSupervisorPrompt()
      .then((r) => setPrompt(r.prompt))
      .catch(() => { /* gateway may not be running */ });
  }, []);

  const handleCopy = () => {
    if (!prompt) return;
    navigator.clipboard.writeText(prompt).then(() => {
      setCopied(true);
      setTimeout(() => setCopied(false), 2000);
    });
  };

  if (!prompt) return null;

  return (
    <div className="mx-3 mt-3 mb-1 rounded-xl overflow-hidden" style={{ border: '1px solid rgba(251,191,36,0.2)', background: 'rgba(251,191,36,0.04)' }}>
      <button
        type="button"
        onClick={() => setOpen((v) => !v)}
        className="w-full flex items-center justify-between px-4 py-2.5"
        style={{ background: 'transparent', border: 'none', cursor: 'pointer', color: '#fbbf24' }}
      >
        <span style={{ fontSize: '11px', fontWeight: 700, letterSpacing: '0.12em', textTransform: 'uppercase' }}>
          Nexus Supervisor Prompt
        </span>
        <div className="flex items-center gap-2">
          <button
            type="button"
            onClick={(e) => { e.stopPropagation(); handleCopy(); }}
            title="Copy to clipboard"
            style={{ background: 'rgba(251,191,36,0.1)', border: '1px solid rgba(251,191,36,0.3)', borderRadius: '6px', padding: '2px 8px', color: '#fbbf24', fontSize: '11px', cursor: 'pointer', display: 'flex', alignItems: 'center', gap: '4px' }}
          >
            <Copy className="h-3 w-3" />
            {copied ? 'Copied!' : 'Copy'}
          </button>
          {open ? <ChevronUp className="h-3.5 w-3.5" /> : <ChevronDown className="h-3.5 w-3.5" />}
        </div>
      </button>
      {open && (
        <pre
          style={{ margin: 0, padding: '12px 16px', fontSize: '11px', lineHeight: 1.6, color: 'rgba(167,139,250,0.85)', whiteSpace: 'pre-wrap', wordBreak: 'break-word', maxHeight: '320px', overflowY: 'auto', background: 'rgba(5,3,15,0.5)' }}
        >
          {prompt}
        </pre>
      )}
    </div>
  );
}

// ─── Main Page ────────────────────────────────────────────────────────────────

export default function ModelMesh() {
  const [models, setModels] = useState<RegisteredModel[]>([]);
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState<string | null>(null);

  const load = useCallback(async () => {
    setLoading(true);
    setError(null);
    try {
      const data = await getRegisteredModels();
      setModels(data);
    } catch (e: unknown) {
      setError(e instanceof Error ? e.message : 'Failed to load models');
    } finally {
      setLoading(false);
    }
  }, []);

  useEffect(() => { load(); }, [load]);

  const handleDelete = async (id: string) => {
    try {
      await deleteRegisteredModel(id);
      setModels((prev) => prev.filter((m) => m.id !== id));
    } catch (e: unknown) {
      alert(e instanceof Error ? e.message : 'Delete failed');
    }
  };

  const handleToggle = async (id: string, active: boolean) => {
    try {
      const updated = await patchRegisteredModel(id, { is_active: active });
      setModels((prev) => prev.map((m) => m.id === id ? { ...m, ...updated } : m));
    } catch (e: unknown) {
      alert(e instanceof Error ? e.message : 'Update failed');
    }
  };

  const handleRename = async (id: string, name: string) => {
    try {
      const updated = await patchRegisteredModel(id, { custom_name: name });
      setModels((prev) => prev.map((m) => m.id === id ? { ...m, ...updated } : m));
    } catch (e: unknown) {
      alert(e instanceof Error ? e.message : 'Rename failed');
    }
  };

  const activeCount = models.filter((m) => m.is_active).length;

  return (
    <div className="flex flex-col h-[calc(100vh-4rem)] p-6 gap-6 animate-fade-in">
      {/* Header */}
      <div className="flex items-center justify-between flex-shrink-0">
        <div className="flex items-center gap-3">
          <div className="p-2 rounded-2xl" style={{ background: 'rgba(167,139,250,0.1)', color: '#a78bfa' }}>
            <Cpu className="h-5 w-5" />
          </div>
          <div>
            <h1 className="text-lg font-semibold" style={{ color: 'var(--pc-text-primary)' }}>Model Mesh</h1>
            <p className="text-xs" style={{ color: 'var(--pc-text-muted)' }}>
              {loading ? 'Loading…' : `${models.length} registered · ${activeCount} active`}
            </p>
          </div>
        </div>
        <div className="flex items-center gap-2">
          <button onClick={load} disabled={loading} title="Refresh"
            className="p-2 rounded-xl transition-colors"
            style={{ background: 'var(--pc-hover)', color: 'var(--pc-text-muted)', border: '1px solid var(--pc-border)' }}>
            <RefreshCw className={`h-4 w-4 ${loading ? 'animate-spin' : ''}`} />
          </button>
          <AddModelForm onAdded={load} />
        </div>
      </div>

      {/* Info banner */}
      <div className="rounded-2xl px-4 py-3 text-xs flex-shrink-0"
        style={{ background: 'rgba(167,139,250,0.06)', border: '1px solid rgba(167,139,250,0.15)', color: 'var(--pc-text-muted)' }}>
        <strong style={{ color: '#a78bfa' }}>Model Mesh</strong> registers external models for use by Nexus sub-agents.
        Active models receive an identity prompt when delegated tasks. API keys are stored server-side only.
      </div>

      {/* Error */}
      {error && (
        <div className="rounded-2xl px-4 py-3 text-xs flex-shrink-0"
          style={{ background: 'rgba(239,68,68,0.1)', border: '1px solid rgba(239,68,68,0.3)', color: '#f87171' }}>
          {error}
        </div>
      )}

      {/* Supervisor Prompt Panel */}
      <SupervisorPromptPanel />

      {/* Table */}
      <div className="flex-1 overflow-y-auto rounded-2xl" style={{ background: 'var(--pc-bg-elevated)', border: '1px solid var(--pc-border)' }}>

      {/* Column headers */}
        <div className="flex items-center gap-3 px-4 py-2.5 border-b text-[10px] font-semibold uppercase tracking-wider"
          style={{ borderColor: 'var(--pc-border)', color: 'var(--pc-text-faint)', background: 'var(--pc-bg-surface)' }}>
          <span className="w-5 shrink-0" />
          <span className="w-16 shrink-0">Source</span>
          <span className="flex-1">Name</span>
          <span className="w-40 hidden sm:block">Model ID</span>
          <span className="w-48 hidden md:block">Endpoint</span>
          <span className="w-8 ml-auto" />
        </div>

        {loading ? (
          <div className="flex items-center justify-center py-16">
            <span className="h-6 w-6 border-2 rounded-full animate-spin" style={{ borderColor: 'var(--pc-border)', borderTopColor: '#a78bfa' }} />
          </div>
        ) : models.length === 0 ? (
          <div className="flex flex-col items-center justify-center py-16 gap-3" style={{ color: 'var(--pc-text-faint)' }}>
            <Cpu className="h-10 w-10 opacity-20" />
            <p className="text-sm">No models registered yet</p>
            <p className="text-xs opacity-60">Use "Add Model" to register your first model.</p>
          </div>
        ) : (
          <div className="p-3 space-y-1 group">
            {models.map((model) => (
              <ModelRow
                key={model.id}
                model={model}
                onDelete={handleDelete}
                onToggle={handleToggle}
                onRename={handleRename}
              />
            ))}
          </div>
        )}
      </div>
    </div>
  );
}

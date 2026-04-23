import { useState, useEffect } from 'react';
import {
  Wrench,
  Search,
  ChevronDown,
  ChevronRight,
  Terminal,
  Package,
  GitBranch,
  FolderOpen,
  Brain,
  Globe,
  Shield,
} from 'lucide-react';
import type { ToolSpec, CliTool, ZeroClawTool } from '@/types/api';
import { getTools, getCliTools, getZeroClawTools } from '@/lib/api';
import { t } from '@/lib/i18n';

// icon lookup for ZeroClaw tools
function zeroIcon(iconName: string | undefined) {
  switch ((iconName ?? '').toLowerCase()) {
    case 'terminal':   return Terminal;
    case 'gitbranch':  return GitBranch;
    case 'folder':     return FolderOpen;
    case 'brain':      return Brain;
    case 'globe':      return Globe;
    default:           return Wrench;
  }
}

export default function Tools() {
  const [tools, setTools] = useState<ToolSpec[]>([]);
  const [cliTools, setCliTools] = useState<CliTool[]>([]);
  const [zeroTools, setZeroTools] = useState<ZeroClawTool[]>([]);
  const [search, setSearch] = useState('');
  const [expandedTool, setExpandedTool] = useState<string | null>(null);
  const [agentSectionOpen, setAgentSectionOpen] = useState(true);
  const [cliSectionOpen, setCliSectionOpen] = useState(true);
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState<string | null>(null);
  const [toolAuthModes, setToolAuthModes] = useState<Record<string, 'user' | 'autonomous'>>({});

  useEffect(() => {
    Promise.all([getTools(), getCliTools(), getZeroClawTools()])
      .then(([t, c, z]) => { setTools(t); setCliTools(c); setZeroTools(z); })
      .catch((err) => setError(err.message))
      .finally(() => setLoading(false));
  }, []);

  useEffect(() => {
    try {
      const raw = localStorage.getItem('herma.toolAuthModes');
      if (raw) {
        const parsed = JSON.parse(raw) as Record<string, 'user' | 'autonomous'>;
        setToolAuthModes(parsed);
      }
    } catch {
      // ignore parse errors and use defaults
    }
  }, []);

  useEffect(() => {
    try {
      localStorage.setItem('herma.toolAuthModes', JSON.stringify(toolAuthModes));
    } catch {
      // storage may be unavailable (private mode)
    }
  }, [toolAuthModes]);

  const filtered = tools.filter((t) =>
    t.name.toLowerCase().includes(search.toLowerCase()) ||
    t.description.toLowerCase().includes(search.toLowerCase()),
  );

  const filteredCli = cliTools.filter((t) =>
    t.name.toLowerCase().includes(search.toLowerCase()) ||
    t.category.toLowerCase().includes(search.toLowerCase()),
  );

  if (error) {
    return (
      <div className="p-6 animate-fade-in">
        <div className="rounded-2xl border p-4" style={{ background: 'rgba(239, 68, 68, 0.08)', borderColor: 'rgba(239, 68, 68, 0.2)', color: '#f87171' }}>
          {t('tools.load_error')}: {error}
        </div>
      </div>
    );
  }

  if (loading) {
    return (
      <div className="flex items-center justify-center h-64">
        <div className="h-8 w-8 border-2 rounded-full animate-spin" style={{ borderColor: 'var(--pc-border)', borderTopColor: 'var(--pc-accent)' }} />
      </div>
    );
  }

  return (
    <div className="p-6 space-y-6 animate-fade-in">
      {/* Search */}
      <div className="relative max-w-md">
        <Search className="absolute left-3 top-1/2 -translate-y-1/2 h-4 w-4" style={{ color: 'var(--pc-text-faint)' }} />
        <input
          type="text"
          value={search}
          onChange={(e) => setSearch(e.target.value)}
          placeholder={t('tools.search')}
          className="input-electric w-full pl-10 pr-4 py-2.5 text-sm"
        />
      </div>

      {/* Agent Tools Grid */}
      <div>
        <button
          onClick={() => setAgentSectionOpen((v) => !v)}
          className="flex items-center gap-2 mb-4 w-full text-left group"
          style={{ background: 'transparent', border: 'none', cursor: 'pointer', padding: 0 }}
          aria-expanded={agentSectionOpen}
          aria-controls="agent-tools-section"
        >
          <Wrench className="h-5 w-5" style={{ color: 'var(--pc-accent)' }} />
          <span className="text-sm font-semibold uppercase tracking-wider flex-1" role="heading" aria-level={2} style={{ color: 'var(--pc-text-primary)' }}>
            {t('tools.agent_tools')} ({filtered.length})
          </span>
          <ChevronDown
            className="h-4 w-4 opacity-40 group-hover:opacity-100"
            style={{ color: 'var(--pc-text-muted)', transform: agentSectionOpen ? 'rotate(0deg)' : 'rotate(-90deg)', transition: 'transform 0.2s ease, opacity 0.2s ease' }}
          />
        </button>

        <div id="agent-tools-section">
          {agentSectionOpen && (filtered.length === 0 ? (
            <p className="text-sm" style={{ color: 'var(--pc-text-muted)' }}>{t('tools.empty')}</p>
          ) : (
            <div className="grid grid-cols-1 md:grid-cols-2 xl:grid-cols-3 gap-4 stagger-children">
              {filtered.map((tool) => {
                const isExpanded = expandedTool === tool.name;
                return (
                  <div
                    key={tool.name}
                    className="card overflow-hidden animate-slide-in-up"
                  >
                    <button
                      onClick={() => setExpandedTool(isExpanded ? null : tool.name)}
                      className="w-full text-left p-4 transition-all"
                      style={{ background: 'transparent' }}
                      onMouseEnter={(e) => { e.currentTarget.style.background = 'var(--pc-hover)'; }}
                      onMouseLeave={(e) => { e.currentTarget.style.background = 'transparent'; }}
                    >
                      <div className="flex items-start justify-between gap-2">
                        <div className="flex items-center gap-2 min-w-0">
                          <Package className="h-4 w-4 flex-shrink-0" style={{ color: 'var(--pc-accent)' }} />
                          <h3 className="text-sm font-semibold truncate" style={{ color: 'var(--pc-text-primary)' }}>{tool.name}</h3>
                        </div>
                        {isExpanded
                          ? <ChevronDown className="h-4 w-4 flex-shrink-0" style={{ color: 'var(--pc-accent)' }} />
                          : <ChevronRight className="h-4 w-4 flex-shrink-0" style={{ color: 'var(--pc-text-faint)' }} />
                        }
                      </div>
                      <p className="text-sm mt-2 line-clamp-2" style={{ color: 'var(--pc-text-muted)' }}>
                        {tool.description}
                      </p>
                    </button>

                    {isExpanded && tool.parameters && (
                      <div className="border-t p-4 animate-fade-in" style={{ borderColor: 'var(--pc-border)' }}>
                        <p className="text-[10px] font-semibold uppercase tracking-wider mb-2" style={{ color: 'var(--pc-text-muted)' }}>
                          {t('tools.parameter_schema')}
                        </p>
                        <pre className="text-xs rounded-xl p-3 overflow-x-auto max-h-64 overflow-y-auto font-mono" style={{ background: 'var(--pc-bg-base)', color: 'var(--pc-text-secondary)' }}>
                          {JSON.stringify(tool.parameters, null, 2)}
                        </pre>
                      </div>
                    )}
                  </div>
                );
              })}
            </div>
          ))}
        </div>
      </div>

      {/* CLI Tools Section */}
      {filteredCli.length > 0 && (
        <div className="animate-slide-in-up" style={{ animationDelay: '200ms' }}>
          <button
            onClick={() => setCliSectionOpen((v) => !v)}
            className="flex items-center gap-2 mb-4 w-full text-left group"
            style={{ background: 'transparent', border: 'none', cursor: 'pointer', padding: 0 }}
            aria-expanded={cliSectionOpen}
            aria-controls="cli-tools-section"
          >
            <Terminal className="h-5 w-5" style={{ color: 'var(--color-status-success)' }} />
            <span className="text-sm font-semibold uppercase tracking-wider flex-1" role="heading" aria-level={2} style={{ color: 'var(--pc-text-primary)' }}>
              {t('tools.cli_tools')} ({filteredCli.length})
            </span>
            <ChevronDown
              className="h-4 w-4 opacity-40 group-hover:opacity-100"
              style={{ color: 'var(--pc-text-muted)', transform: cliSectionOpen ? 'rotate(0deg)' : 'rotate(-90deg)', transition: 'transform 0.2s ease, opacity 0.2s ease' }}
            />
          </button>

          <div id="cli-tools-section">
            {cliSectionOpen && <div className="card overflow-hidden rounded-2xl">
              <table className="table-electric">
                <thead>
                  <tr>
                    <th>{t('tools.name')}</th>
                    <th>{t('tools.path')}</th>
                    <th>{t('tools.version')}</th>
                    <th>{t('tools.category')}</th>
                  </tr>
                </thead>
                <tbody>
                  {filteredCli.map((tool) => (
                    <tr key={tool.name}>
                      <td className="font-medium text-sm" style={{ color: 'var(--pc-text-primary)' }}>
                        {tool.name}
                      </td>
                      <td className="font-mono text-xs truncate max-w-[200px]" style={{ color: 'var(--pc-text-muted)' }}>
                        {tool.path}
                      </td>
                      <td style={{ color: 'var(--pc-text-muted)' }}>
                        {tool.version ?? '-'}
                      </td>
                      <td>
                        <span className="inline-flex items-center px-2.5 py-0.5 rounded-full text-[10px] font-semibold capitalize border" style={{ borderColor: 'var(--pc-border)', color: 'var(--pc-text-secondary)', background: 'var(--pc-accent-glow)' }}>
                          {tool.category}
                        </span>
                      </td>
                    </tr>
                  ))}
                </tbody>
              </table>
            </div>}
          </div>
        </div>
      )}

      {/* ZeroClaw Embedded Tools */}
      {zeroTools.length > 0 && (
        <div>
          <div className="flex items-center gap-2 mb-4">
            <Shield className="h-4 w-4" style={{ color: '#a78bfa' }} />
            <h2 className="text-sm font-semibold uppercase tracking-wider" style={{ color: 'var(--pc-text-primary)' }}>
              ZeroClaw Embedded Tools
            </h2>
          </div>
          <div className="grid grid-cols-1 sm:grid-cols-2 lg:grid-cols-3 gap-4 stagger-children">
            {zeroTools.map((tool) => {
              const Icon = zeroIcon(tool.icon);
              const statusColor = tool.locked
                ? '#fbbf24'
                : tool.available ? '#34d399' : '#f87171';
              const statusLabel = tool.locked
                ? 'Locked'
                : tool.available ? 'OK' : 'Error';
              const statusBg = tool.locked
                ? 'rgba(251,191,36,0.1)'
                : tool.available ? 'rgba(52,211,153,0.1)' : 'rgba(239,68,68,0.1)';
              const statusBorder = tool.locked
                ? 'rgba(251,191,36,0.3)'
                : tool.available ? 'rgba(52,211,153,0.3)' : 'rgba(239,68,68,0.3)';
              const mode = toolAuthModes[tool.tool_id] ?? (tool.locked ? 'user' : 'autonomous');

              return (
                <div key={tool.tool_id} className="card p-5 animate-slide-in-up">
                  <div className="flex items-start justify-between mb-3">
                    <div className="flex items-center gap-3">
                      <div className="p-2 rounded-2xl" style={{ background: 'rgba(167,139,250,0.1)', color: '#a78bfa' }}>
                        <Icon className="h-5 w-5" />
                      </div>
                      <div>
                        <p className="text-sm font-semibold" style={{ color: 'var(--pc-text-primary)' }}>{tool.name}</p>
                        <p className="text-xs font-mono" style={{ color: 'var(--pc-text-faint)' }}>{tool.tool_id}</p>
                      </div>
                    </div>
                    <span className="inline-flex items-center gap-1 text-[10px] font-semibold px-2 py-1 rounded-full"
                      style={{ background: statusBg, color: statusColor, border: `1px solid ${statusBorder}` }}>
                      {statusLabel}
                    </span>
                  </div>
                  <p className="text-xs leading-relaxed" style={{ color: 'var(--pc-text-muted)' }}>
                    {tool.description}
                  </p>

                  <div className="mt-4 pt-3" style={{ borderTop: '1px solid var(--pc-border)' }}>
                    <p className="text-[10px] uppercase tracking-wider mb-2" style={{ color: 'var(--pc-text-faint)' }}>
                      Authorization Mode
                    </p>
                    <div className="inline-flex rounded-lg p-1" style={{ background: 'var(--pc-bg-elevated)', border: '1px solid var(--pc-border)' }}>
                      <button
                        onClick={() => setToolAuthModes((prev) => ({ ...prev, [tool.tool_id]: 'user' }))}
                        className="px-2.5 py-1 text-[11px] rounded-md transition-all"
                        style={mode === 'user'
                          ? { background: 'rgba(251,191,36,0.12)', color: '#fbbf24' }
                          : { background: 'transparent', color: 'var(--pc-text-muted)' }}
                      >
                        User Required
                      </button>
                      <button
                        onClick={() => setToolAuthModes((prev) => ({ ...prev, [tool.tool_id]: 'autonomous' }))}
                        className="px-2.5 py-1 text-[11px] rounded-md transition-all"
                        style={mode === 'autonomous'
                          ? { background: 'rgba(52,211,153,0.12)', color: '#34d399' }
                          : { background: 'transparent', color: 'var(--pc-text-muted)' }}
                      >
                        Autonomous
                      </button>
                    </div>
                  </div>
                </div>
              );
            })}
          </div>
        </div>
      )}
    </div>
  );
}

import { useMemo } from 'react';
import { Activity } from 'lucide-react';
import { useSSE } from '@/hooks/useSSE';
import type { SSEEvent } from '@/types/api';

function formatRelative(iso?: string): string {
  if (!iso) return '';
  try {
    const diff = Date.now() - new Date(iso).getTime();
    const seconds = Math.floor(diff / 1000);
    if (seconds < 60) return `${seconds}s ago`;
    const minutes = Math.floor(seconds / 60);
    if (minutes < 60) return `${minutes}m ago`;
    const hours = Math.floor(minutes / 60);
    if (hours < 24) return `${hours}h ago`;
    const days = Math.floor(hours / 24);
    return `${days}d ago`;
  } catch {
    return iso;
  }
}

export default function SubconsciousWidget({ maxItems = 12, multiSessionEnabled = true, sessionId = '' }: { maxItems?: number; multiSessionEnabled?: boolean; sessionId?: string }) {
  const { events } = useSSE({ filterTypes: ['audit', 'doctor', 'subconscious'], maxEvents: 500, autoConnect: true });

  const recent = useMemo(() => {
    const ev: SSEEvent[] = events.slice(-maxItems).slice().reverse();
    const filtered = ev.filter((e) => {
      if (!multiSessionEnabled && sessionId) {
        const sid = (e.session_id ?? e.sessionId ?? '') as string;
        if (!sid) return false;
        return sid === sessionId;
      }
      return true;
    });
    return filtered;
  }, [events, maxItems, multiSessionEnabled, sessionId]);

  return (
    <div className="card p-5 animate-slide-in-up">
      <div className="flex items-center gap-2 mb-4">
        <Activity className="h-5 w-5" style={{ color: 'var(--pc-accent)' }} />
        <h3 className="text-sm font-semibold uppercase tracking-wider" style={{ color: 'var(--pc-text-primary)' }}>Subconscious Activity</h3>
      </div>

      {recent.length === 0 ? (
        <p className="text-sm" style={{ color: 'var(--pc-text-faint)' }}>No recent activity</p>
      ) : (
        <div className="space-y-2 max-h-40 overflow-y-auto pr-1">
          {recent.map((e, idx) => {
            const detail = e.message ?? e.data ?? e.operation ?? JSON.stringify(e);
            return (
              <div key={`${e.type}-${idx}`} className="px-2 py-2 rounded-md" style={{ background: 'var(--pc-bg-elevated)' }}>
                <div className="flex items-center justify-between">
                  <div className="text-sm font-medium" style={{ color: 'var(--pc-text-primary)' }}>{e.type}</div>
                  <div className="text-xs font-mono" style={{ color: 'var(--pc-text-muted)' }}>{formatRelative(e.timestamp)}</div>
                </div>
                <div className="text-xs mt-1 truncate" style={{ color: 'var(--pc-text-muted)' }}>{String(detail).slice(0, 240)}</div>
              </div>
            );
          })}
        </div>
      )}
    </div>
  );
}

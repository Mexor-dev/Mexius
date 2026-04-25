/**
 * ModeToggle — Sovereignty mode switcher displayed in the header.
 *
 * Shows three pill buttons: [Entity] [Nexus] [Dream]
 * Entity  = "active" state (single persona, interactive)
 * Nexus   = multi-agent orchestration mode
 * Dream   = Maintenance/Dream State (agent self-reflects; UI locked)
 */

import { useSovereignty } from '@/contexts/SovereigntyContext';
import type { SovereigntyStateValue } from '@/types/api';
import { Moon, Network, User } from 'lucide-react';

interface ModeOption {
  value: SovereigntyStateValue;
  label: string;
  Icon: React.ComponentType<{ className?: string }>;
  color: string;
  bg: string;
  border: string;
  activeBg: string;
  activeBorder: string;
  activeColor: string;
}

const MODES: ModeOption[] = [
  {
    value: 'active',
    label: 'Entity',
    Icon: User,
    color: 'var(--pc-text-muted)',
    bg: 'transparent',
    border: 'var(--pc-border)',
    activeBg: 'rgba(99,102,241,0.12)',
    activeBorder: 'rgba(99,102,241,0.4)',
    activeColor: '#818cf8',
  },
  {
    value: 'nexus',
    label: 'Nexus',
    Icon: Network,
    color: 'var(--pc-text-muted)',
    bg: 'transparent',
    border: 'var(--pc-border)',
    activeBg: 'rgba(167,139,250,0.12)',
    activeBorder: 'rgba(167,139,250,0.4)',
    activeColor: '#a78bfa',
  },
  {
    value: 'dreaming',
    label: 'Dream',
    Icon: Moon,
    color: 'var(--pc-text-muted)',
    bg: 'transparent',
    border: 'var(--pc-border)',
    activeBg: 'rgba(251,191,36,0.08)',
    activeBorder: 'rgba(251,191,36,0.35)',
    activeColor: '#fbbf24',
  },
];

export default function ModeToggle() {
  const { state, loading, toggle } = useSovereignty();

  return (
    <div
      className="flex items-center gap-0.5 rounded-xl p-0.5"
      style={{ background: 'var(--pc-bg-elevated)', border: '1px solid var(--pc-border)' }}
      title="Sovereignty Mode"
    >
      {MODES.map(({ value, label, Icon, activeBg, activeBorder, activeColor, color }) => {
        const isActive = state === value;
        return (
          <button
            key={value}
            type="button"
            disabled={loading}
            onClick={() => !isActive && toggle(value)}
            className="flex items-center gap-1.5 px-2.5 py-1 rounded-lg text-xs font-medium transition-all duration-200"
            style={{
              background: isActive ? activeBg : 'transparent',
              border: `1px solid ${isActive ? activeBorder : 'transparent'}`,
              color: isActive ? activeColor : color,
              cursor: isActive ? 'default' : loading ? 'wait' : 'pointer',
              opacity: loading && !isActive ? 0.5 : 1,
            }}
            aria-pressed={isActive}
            aria-label={`Switch to ${label} mode`}
          >
            <Icon className="h-3 w-3" />
            <span className="hidden sm:inline">{label}</span>
          </button>
        );
      })}
    </div>
  );
}

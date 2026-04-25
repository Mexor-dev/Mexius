/**
 * PinGate — Sovereign Access Control
 *
 * Wraps the entire app. On every new browser session (tab open):
 *  1. Checks sessionStorage for 'mexius_pin_ok' — if present, passes through.
 *  2. Fetches GET /api/pin/status — if no PIN configured, shows setup screen.
 *  3. If PIN is set, shows full-screen lock until correct PIN is entered.
 *
 * PIN is hashed (SHA-256 + salt) server-side. Never stored in plain text.
 */

import { useEffect, useRef, useState, useCallback } from 'react';
import { apiOrigin, basePath } from '@/lib/basePath';

const SESSION_KEY = 'mexius_pin_ok';

// ─── Styles ──────────────────────────────────────────────────────────────────
const PIN_STYLE = `
@keyframes pin-in {
  from { opacity: 0; transform: scale(1.04); }
  to   { opacity: 1; transform: scale(1); }
}
@keyframes pin-shake {
  0%,100% { transform: translateX(0); }
  20%     { transform: translateX(-8px); }
  40%     { transform: translateX(8px); }
  60%     { transform: translateX(-6px); }
  80%     { transform: translateX(6px); }
}
@keyframes pin-dot-fill {
  from { transform: scale(0.5); opacity: 0.3; }
  to   { transform: scale(1); opacity: 1; }
}
@keyframes pin-glow-pulse {
  0%,100% { box-shadow: 0 0 0 0 rgba(212,175,55,0.4); }
  50%     { box-shadow: 0 0 0 12px rgba(212,175,55,0); }
}
@keyframes pin-scan {
  0%   { background-position: 0% 0%; }
  100% { background-position: 0% 100%; }
}
.pin-overlay {
  animation: pin-in 0.4s ease-out forwards;
}
.pin-shake {
  animation: pin-shake 0.5s ease-out;
}
.pin-dot-filled {
  animation: pin-dot-fill 0.15s ease-out forwards;
}
.pin-submit-active {
  animation: pin-glow-pulse 1.5s ease-in-out infinite;
}
`;

// ─── Scanline overlay for depth ───────────────────────────────────────────────
function ScanLines() {
  return (
    <div style={{
      position: 'absolute', inset: 0, pointerEvents: 'none', zIndex: 1,
      background: 'repeating-linear-gradient(0deg, rgba(0,0,0,0.04) 0px, rgba(0,0,0,0.04) 1px, transparent 1px, transparent 3px)',
    }} />
  );
}

// ─── Corner accent (sharp 45° chamfer) ────────────────────────────────────────
function CornerAccent({ pos }: { pos: 'tl' | 'tr' | 'bl' | 'br' }) {
  const isTop = pos[0] === 't';
  const isLeft = pos[1] === 'l';
  return (
    <div style={{
      position: 'absolute',
      [isTop ? 'top' : 'bottom']: 0,
      [isLeft ? 'left' : 'right']: 0,
      width: '24px', height: '24px',
      borderTop: isTop ? '2px solid rgba(212,175,55,0.5)' : 'none',
      borderBottom: !isTop ? '2px solid rgba(212,175,55,0.5)' : 'none',
      borderLeft: isLeft ? '2px solid rgba(212,175,55,0.5)' : 'none',
      borderRight: !isLeft ? '2px solid rgba(212,175,55,0.5)' : 'none',
    }} />
  );
}

// ─── 4-digit PIN input ────────────────────────────────────────────────────────
function PinDots({ digits, shake, error }: { digits: string[]; shake: boolean; error: string | null }) {
  return (
    <div style={{ display: 'flex', flexDirection: 'column', alignItems: 'center', gap: '20px' }}>
      <div className={shake ? 'pin-shake' : ''} style={{ display: 'flex', gap: '16px' }}>
        {digits.map((d, i) => (
          <div
            key={i}
            className={d ? 'pin-dot-filled' : ''}
            style={{
              width: '18px', height: '18px',
              borderRadius: '50%',
              border: `2px solid ${d ? '#d4af37' : 'rgba(212,175,55,0.3)'}`,
              background: d ? '#d4af37' : 'transparent',
              transition: 'border-color 0.15s',
              boxShadow: d ? '0 0 10px rgba(212,175,55,0.6)' : 'none',
            }}
          />
        ))}
      </div>
      {error && (
        <p style={{ fontSize: '12px', color: '#ff4466', letterSpacing: '0.08em', margin: 0 }}>
          {error}
        </p>
      )}
    </div>
  );
}

// ─── Main PinGate component ───────────────────────────────────────────────────
interface PinGateProps { children: React.ReactNode; }

export default function PinGate({ children }: PinGateProps) {
  const [status, setStatus] = useState<'loading' | 'none' | 'locked' | 'setup' | 'unlocked'>('loading');
  const [mode, setMode] = useState<'unlock' | 'setup' | 'confirm'>('unlock');
  const [digits, setDigits] = useState(['', '', '', '']);
  const [setupDigits, setSetupDigits] = useState(['', '', '', '']);
  const [error, setError] = useState<string | null>(null);
  const [shake, setShake] = useState(false);
  const [busy, setBusy] = useState(false);
  const inputRef = useRef<HTMLInputElement>(null);

  const base = `${apiOrigin ?? ''}${basePath}`;

  useEffect(() => {
    if (sessionStorage.getItem(SESSION_KEY) === '1') {
      setStatus('unlocked');
      return;
    }
    fetch(`${base}/api/pin/status`)
      .then((r) => r.json())
      .then((d: { has_pin: boolean }) => {
        if (d.has_pin) {
          setStatus('locked');
          setMode('unlock');
        } else {
          setStatus('setup');
          setMode('setup');
        }
      })
      .catch(() => {
        // Gateway not reachable or no PIN endpoint — pass through
        setStatus('unlocked');
      });
  }, [base]);

  // Focus hidden input whenever locked/setup
  useEffect(() => {
    if (status !== 'locked' && status !== 'setup') return;
    const t = setTimeout(() => inputRef.current?.focus(), 100);
    return () => clearTimeout(t);
  }, [status, mode]);

  const triggerShake = useCallback(() => {
    setShake(true);
    setTimeout(() => setShake(false), 600);
  }, []);

  const handleKeyInput = useCallback(async (key: string) => {
    if (busy) return;

    if (key === 'Backspace') {
      setDigits((d) => {
        const idx = d.reduce((last, v, i) => (v ? i : last), -1);
        if (idx < 0) return d;
        const next = [...d];
        next[idx] = '';
        return next;
      });
      setError(null);
      return;
    }

    if (!/^\d$/.test(key)) return;

    setDigits((prev) => {
      const idx = prev.findIndex((v) => !v);
      if (idx < 0) return prev;
      const next = [...prev];
      next[idx] = key;

      // Auto-submit when 4th digit entered
      if (idx === 3) {
        const pin = next.join('');
        setTimeout(() => submitPin(pin), 80);
      }
      return next;
    });
  }, [busy]); // eslint-disable-line

  const submitPin = useCallback(async (pin: string) => {
    if (pin.length !== 4) return;
    setBusy(true);
    setError(null);

    if (mode === 'unlock') {
      try {
        const r = await fetch(`${base}/api/pin/verify`, {
          method: 'POST',
          headers: { 'Content-Type': 'application/json' },
          body: JSON.stringify({ pin }),
        });
        const data: { ok: boolean } = await r.json();
        if (data.ok) {
          sessionStorage.setItem(SESSION_KEY, '1');
          setStatus('unlocked');
        } else {
          triggerShake();
          setError('Incorrect PIN');
          setDigits(['', '', '', '']);
          setTimeout(() => inputRef.current?.focus(), 50);
        }
      } catch {
        setError('Connection error');
        setDigits(['', '', '', '']);
      }
    }

    if (mode === 'setup') {
      setSetupDigits([pin[0], pin[1], pin[2], pin[3]]);
      setDigits(['', '', '', '']);
      setMode('confirm');
      setTimeout(() => inputRef.current?.focus(), 50);
    }

    if (mode === 'confirm') {
      const first = setupDigits.join('');
      if (pin !== first) {
        triggerShake();
        setError('PINs do not match — start over');
        setDigits(['', '', '', '']);
        setSetupDigits(['', '', '', '']);
        setMode('setup');
        setTimeout(() => inputRef.current?.focus(), 50);
      } else {
        try {
          const r = await fetch(`${base}/api/pin/set`, {
            method: 'POST',
            headers: { 'Content-Type': 'application/json' },
            body: JSON.stringify({ pin }),
          });
          if (r.ok) {
            sessionStorage.setItem(SESSION_KEY, '1');
            setStatus('unlocked');
          } else {
            const d = await r.json().catch(() => ({}));
            setError((d as { error?: string }).error ?? 'Setup failed');
            setDigits(['', '', '', '']);
          }
        } catch {
          setError('Connection error');
          setDigits(['', '', '', '']);
        }
      }
    }

    setBusy(false);
  }, [mode, setupDigits, base, triggerShake]);

  if (status === 'unlocked') return <>{children}</>;
  if (status === 'loading') return null;

  const title = mode === 'unlock'
    ? 'MEXIUS SOVEREIGN ACCESS'
    : mode === 'setup'
    ? 'SECURE MEXIUS'
    : 'CONFIRM PIN';

  const subtitle = mode === 'unlock'
    ? 'Enter your 4-digit access key'
    : mode === 'setup'
    ? 'Set your 4-digit access key'
    : 'Re-enter to confirm';

  return (
    <>
      <style>{PIN_STYLE}</style>

      {/* Full-screen overlay */}
      <div
        className="pin-overlay"
        style={{
          position: 'fixed', inset: 0, zIndex: 10000,
          background: '#050505',
          display: 'flex', flexDirection: 'column',
          alignItems: 'center', justifyContent: 'center',
          overflow: 'hidden',
          fontFamily: "'JetBrains Mono', 'Roboto Mono', ui-monospace, monospace",
        }}
        onClick={() => inputRef.current?.focus()}
      >
        <ScanLines />

        {/* Grid background */}
        <div style={{
          position: 'absolute', inset: 0, pointerEvents: 'none',
          backgroundImage: 'linear-gradient(rgba(212,175,55,0.03) 1px, transparent 1px), linear-gradient(90deg, rgba(212,175,55,0.03) 1px, transparent 1px)',
          backgroundSize: '40px 40px',
        }} />

        {/* Center panel */}
        <div style={{
          position: 'relative', zIndex: 2,
          width: '320px',
          padding: '40px 36px',
          background: '#0d0d0d',
          border: '1px solid rgba(212,175,55,0.2)',
          display: 'flex', flexDirection: 'column', alignItems: 'center', gap: '32px',
        }}>
          <CornerAccent pos="tl" />
          <CornerAccent pos="tr" />
          <CornerAccent pos="bl" />
          <CornerAccent pos="br" />

          {/* Wordmark */}
          <div style={{ textAlign: 'center' }}>
            <p style={{ fontSize: '10px', letterSpacing: '0.4em', color: 'rgba(212,175,55,0.5)', margin: '0 0 8px 0' }}>
              MEXIUS v1.0 // SOVEREIGN CORE
            </p>
            <p style={{ fontSize: '13px', fontWeight: 700, letterSpacing: '0.2em', color: '#d4af37', margin: 0 }}>
              {title}
            </p>
            <p style={{ fontSize: '11px', letterSpacing: '0.06em', color: 'rgba(255,255,255,0.35)', margin: '6px 0 0 0' }}>
              {subtitle}
            </p>
          </div>

          {/* PIN dots */}
          <PinDots digits={digits} shake={shake} error={error} />

          {/* Hidden keyboard capture input */}
          <input
            ref={inputRef}
            type="tel"
            inputMode="numeric"
            maxLength={1}
            style={{ position: 'absolute', opacity: 0, width: '1px', height: '1px', pointerEvents: 'none' }}
            onKeyDown={(e) => {
              e.preventDefault();
              handleKeyInput(e.key);
            }}
            onChange={() => {}}
            value=""
            aria-label="PIN digit"
          />

          {/* Instruction */}
          <p style={{ fontSize: '10px', color: 'rgba(255,255,255,0.2)', letterSpacing: '0.1em', margin: 0, textAlign: 'center' }}>
            {mode === 'setup'
              ? 'TAP / TYPE TO ENTER DIGITS'
              : 'PRESS BACKSPACE TO CORRECT'}
          </p>

          {/* Numpad for touch */}
          <div style={{ display: 'grid', gridTemplateColumns: 'repeat(3, 1fr)', gap: '8px', width: '100%' }}>
            {['1','2','3','4','5','6','7','8','9','','0','⌫'].map((k, i) => (
              <button
                key={i}
                type="button"
                onClick={() => k === '⌫' ? handleKeyInput('Backspace') : k ? handleKeyInput(k) : undefined}
                style={{
                  padding: '14px',
                  background: k ? 'rgba(212,175,55,0.06)' : 'transparent',
                  border: k ? '1px solid rgba(212,175,55,0.15)' : 'none',
                  borderRadius: '2px',
                  color: k === '⌫' ? 'rgba(212,175,55,0.6)' : '#d4d4d8',
                  fontSize: '15px',
                  fontFamily: 'inherit',
                  fontWeight: 500,
                  cursor: k ? 'pointer' : 'default',
                  transition: 'background 0.1s, border-color 0.1s',
                }}
                onMouseEnter={(e) => { if (k) e.currentTarget.style.background = 'rgba(212,175,55,0.12)'; }}
                onMouseLeave={(e) => { if (k) e.currentTarget.style.background = 'rgba(212,175,55,0.06)'; }}
              >
                {k}
              </button>
            ))}
          </div>
        </div>

        {/* Footer */}
        <p style={{
          position: 'absolute', bottom: '24px',
          fontSize: '10px', color: 'rgba(255,255,255,0.15)',
          letterSpacing: '0.12em', zIndex: 2,
        }}>
          SESSION LOCKED — CLOSE TAB TO LOCK
        </p>
      </div>
    </>
  );
}

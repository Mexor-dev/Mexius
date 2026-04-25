/**
 * DreamOverlay — Full-screen immersive Dream State experience.
 *
 * When the agent enters Dream State, the entire viewport is taken over by:
 *  - A deep-space star-field (pure CSS, deterministic seed)
 *  - A pulsing Mexius Golden Spiral (SVG, gold + purple glow)
 *  - Rotating status messages cycling every 4s
 *  - A single large "WAKE" button to return to Active mode
 */

import { useEffect, useRef, useState } from 'react';
import { useSovereignty } from '@/contexts/SovereigntyContext';

// ─── Dream messages that cycle during the sleep state ────────────────────────
const DREAM_MESSAGES = [
  'Synthesizing Neural Pathways...',
  'Optimizing Sovereignty...',
  'Refining Soul Architecture...',
  'Consolidating Episodic Memory...',
  'Calibrating Golden Ratio...',
  'Distilling Operational Essence...',
  'Mapping Delegation Vectors...',
  'Aligning Agent Harmonics...',
];

// ─── CSS injected once ────────────────────────────────────────────────────────
const DREAM_STYLE = `
@keyframes mex-star-drift {
  from { transform: translateY(0px) translateX(0px); }
  to   { transform: translateY(-120px) translateX(20px); }
}
@keyframes mex-spiral-rotate {
  0%   { opacity: 0.55; filter: drop-shadow(0 0 14px rgba(251,191,36,0.55)) drop-shadow(0 0 32px rgba(167,139,250,0.3)); transform: rotate(0deg) scale(1); }
  50%  { opacity: 0.88; filter: drop-shadow(0 0 30px rgba(251,191,36,0.95)) drop-shadow(0 0 64px rgba(167,139,250,0.55)); transform: rotate(180deg) scale(1.07); }
  100% { opacity: 0.55; filter: drop-shadow(0 0 14px rgba(251,191,36,0.55)) drop-shadow(0 0 32px rgba(167,139,250,0.3)); transform: rotate(360deg) scale(1); }
}
@keyframes mex-ring-a {
  0%,100% { opacity: 0.14; transform: scale(0.94); }
  50%     { opacity: 0.46; transform: scale(1.09); }
}
@keyframes mex-ring-b {
  0%,100% { opacity: 0.07; transform: scale(0.88); }
  50%     { opacity: 0.27; transform: scale(1.16); }
}
@keyframes mex-msg-fade {
  0%   { opacity: 0; transform: translateY(9px); }
  15%  { opacity: 1; transform: translateY(0); }
  82%  { opacity: 1; transform: translateY(0); }
  100% { opacity: 0; transform: translateY(-9px); }
}
@keyframes mex-wake-pulse {
  0%   { box-shadow: 0 0 0 0 rgba(251,191,36,0.42); }
  70%  { box-shadow: 0 0 0 18px rgba(251,191,36,0); }
  100% { box-shadow: 0 0 0 0 rgba(251,191,36,0); }
}
@keyframes mex-in {
  from { opacity: 0; }
  to   { opacity: 1; }
}
.mex-dream-root  { animation: mex-in 0.55s ease forwards; }
.mex-spiral      { animation: mex-spiral-rotate 6s ease-in-out infinite; transform-origin: center center; }
.mex-ring-a      { animation: mex-ring-a 3s ease-in-out infinite; transform-origin: center center; }
.mex-ring-b      { animation: mex-ring-b 4.5s ease-in-out infinite 0.8s; transform-origin: center center; }
.mex-msg         { animation: mex-msg-fade 4s ease-in-out forwards; }
.mex-wake        { animation: mex-wake-pulse 2s ease-out infinite; }
.mex-star        { position: absolute; border-radius: 50%; background: white; animation: mex-star-drift linear infinite; }
`;

// ─── Deterministic star field ─────────────────────────────────────────────────
interface Star { id: number; left: string; top: string; size: number; opacity: number; duration: number; delay: number; }
function buildStars(n: number): Star[] {
  const stars: Star[] = [];
  let s = 0xdeadbeef;
  const r = () => { s = (s * 1664525 + 1013904223) >>> 0; return (s >>> 0) / 0xffffffff; };
  for (let i = 0; i < n; i++) stars.push({ id: i, left: `${r() * 100}%`, top: `${r() * 120}%`, size: r() * 2.2 + 0.4, opacity: r() * 0.7 + 0.15, duration: r() * 20 + 15, delay: -(r() * 20) });
  return stars;
}
const STARS = buildStars(180);

// ─── Golden Spiral SVG ────────────────────────────────────────────────────────
function GoldenSpiral() {
  return (
    <svg className="mex-spiral" viewBox="0 0 200 200" width="260" height="260" fill="none">
      <defs>
        <linearGradient id="mex-gold" x1="0%" y1="0%" x2="100%" y2="100%">
          <stop offset="0%"   stopColor="#fbbf24" stopOpacity="0.92" />
          <stop offset="50%"  stopColor="#a78bfa" stopOpacity="0.72" />
          <stop offset="100%" stopColor="#fbbf24" stopOpacity="0.42" />
        </linearGradient>
        <filter id="mex-glow"><feGaussianBlur stdDeviation="3" result="b"/><feMerge><feMergeNode in="b"/><feMergeNode in="SourceGraphic"/></feMerge></filter>
      </defs>
      <polygon points="100,10 174,52.5 174,147.5 100,190 26,147.5 26,52.5" stroke="url(#mex-gold)" strokeWidth="1.2" opacity="0.4" filter="url(#mex-glow)" />
      <polygon points="100,38 148,65 148,135 100,162 52,135 52,65"         stroke="url(#mex-gold)" strokeWidth="1"   opacity="0.5" filter="url(#mex-glow)" />
      <path d="M 100 100 Q 145 80 140 45"     stroke="url(#mex-gold)" strokeWidth="2.5" strokeLinecap="round" filter="url(#mex-glow)" />
      <path d="M 140 45  Q 170 30 165 80"     stroke="url(#mex-gold)" strokeWidth="2"   strokeLinecap="round" filter="url(#mex-glow)" opacity="0.85" />
      <path d="M 165 80  Q 172 130 125 150"   stroke="url(#mex-gold)" strokeWidth="1.8" strokeLinecap="round" filter="url(#mex-glow)" opacity="0.7" />
      <path d="M 125 150 Q 80 172 50 140"     stroke="url(#mex-gold)" strokeWidth="1.5" strokeLinecap="round" filter="url(#mex-glow)" opacity="0.55" />
      <path d="M 50 140  Q 28 110 42 75"      stroke="url(#mex-gold)" strokeWidth="1.2" strokeLinecap="round" filter="url(#mex-glow)" opacity="0.4" />
      <line x1="100" y1="38" x2="100" y2="162" stroke="#a78bfa" strokeWidth="0.5" opacity="0.25" />
      <line x1="52"  y1="65" x2="148" y2="135" stroke="#a78bfa" strokeWidth="0.5" opacity="0.25" />
      <line x1="148" y1="65" x2="52"  y2="135" stroke="#a78bfa" strokeWidth="0.5" opacity="0.25" />
      <circle cx="100" cy="100" r="6" fill="#fbbf24" opacity="0.85" filter="url(#mex-glow)" />
      <circle cx="100" cy="100" r="3" fill="white"   opacity="0.9" />
    </svg>
  );
}

// ─── Component ────────────────────────────────────────────────────────────────
export default function DreamOverlay() {
  const { state, toggle } = useSovereignty();
  const [msgIdx, setMsgIdx] = useState(0);
  const [msgKey, setMsgKey] = useState(0);
  const timerRef = useRef<ReturnType<typeof setInterval> | null>(null);

  useEffect(() => {
    if (state !== 'dreaming') return;
    timerRef.current = setInterval(() => {
      setMsgIdx((i) => (i + 1) % DREAM_MESSAGES.length);
      setMsgKey((k) => k + 1);
    }, 4000);
    return () => { if (timerRef.current) clearInterval(timerRef.current); };
  }, [state]);

  if (state !== 'dreaming') return null;

  return (
    <>
      <style>{DREAM_STYLE}</style>
      <div className="mex-dream-root" style={{ position: 'fixed', inset: 0, zIndex: 9000, background: 'radial-gradient(ellipse at 50% 40%, rgba(20,8,40,0.98) 0%, rgba(5,3,15,0.99) 60%, rgba(2,1,8,1) 100%)', display: 'flex', flexDirection: 'column', alignItems: 'center', justifyContent: 'center', overflow: 'hidden' }}>

        {/* Star field */}
        {STARS.map((s) => (
          <span key={s.id} className="mex-star" style={{ left: s.left, top: s.top, width: `${s.size}px`, height: `${s.size}px`, opacity: s.opacity, animationDuration: `${s.duration}s`, animationDelay: `${s.delay}s` }} />
        ))}

        {/* Ambient rings */}
        <div className="mex-ring-b" style={{ position: 'absolute', width: '520px', height: '520px', borderRadius: '50%', border: '1px solid rgba(167,139,250,0.3)', pointerEvents: 'none' }} />
        <div className="mex-ring-a" style={{ position: 'absolute', width: '380px', height: '380px', borderRadius: '50%', border: '1px solid rgba(251,191,36,0.25)', pointerEvents: 'none' }} />

        {/* Main content */}
        <div style={{ position: 'relative', zIndex: 1, display: 'flex', flexDirection: 'column', alignItems: 'center', gap: '28px' }}>
          <div style={{ display: 'flex', flexDirection: 'column', alignItems: 'center', gap: '6px' }}>
            <span style={{ fontSize: '11px', letterSpacing: '0.35em', color: 'rgba(251,191,36,0.5)', textTransform: 'uppercase', fontWeight: 600 }}>MEXIUS CORE</span>
            <GoldenSpiral />
          </div>

          <div style={{ height: '28px', display: 'flex', alignItems: 'center' }}>
            <p key={msgKey} className="mex-msg" style={{ fontSize: '15px', fontWeight: 500, letterSpacing: '0.04em', color: 'rgba(167,139,250,0.85)', textAlign: 'center', margin: 0 }}>
              {DREAM_MESSAGES[msgIdx]}
            </p>
          </div>

          <p style={{ fontSize: '11px', color: 'rgba(167,139,250,0.35)', letterSpacing: '0.1em', textTransform: 'uppercase', margin: 0 }}>
            Dream State Active — External Input Suspended
          </p>

          <button
            type="button"
            onClick={() => toggle('active')}
            className="mex-wake"
            aria-label="Exit Dream State"
            style={{ marginTop: '8px', padding: '14px 52px', borderRadius: '50px', border: '1px solid rgba(251,191,36,0.5)', background: 'rgba(251,191,36,0.08)', color: '#fbbf24', fontSize: '13px', fontWeight: 700, letterSpacing: '0.25em', textTransform: 'uppercase', cursor: 'pointer', transition: 'background 0.2s, border-color 0.2s' }}
            onMouseEnter={(e) => { e.currentTarget.style.background = 'rgba(251,191,36,0.18)'; e.currentTarget.style.borderColor = 'rgba(251,191,36,0.8)'; }}
            onMouseLeave={(e) => { e.currentTarget.style.background = 'rgba(251,191,36,0.08)'; e.currentTarget.style.borderColor = 'rgba(251,191,36,0.5)'; }}
          >
            WAKE
          </button>
        </div>
      </div>
    </>
  );
}

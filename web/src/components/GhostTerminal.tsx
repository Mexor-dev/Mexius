// ============================================================================
//  GhostTerminal.tsx  (adapted for HTTP API — no Tauri invoke)
//
//  Text input that calls /api/lattice/inject_word on every keystroke, then
//  shows a "ghost list" of 3 AI-predicted word completions above the caret.
// ============================================================================

import {
  useCallback, useEffect, useRef, useState,
} from 'react';
import type { MutableRefObject } from 'react';
import type { TokenScore } from '../hooks/useResonance';

// ─── Embedded vocabulary ────────────────────────────────────────────────────
const WORD_LIST = [
  'the','and','of','to','a','in','is','it','you','that','he','was','for',
  'on','are','with','as','his','they','at','be','this','have','from',
  'or','one','had','by','but','not','what','all','were','we','when',
  'your','can','said','there','use','an','each','which','she','do',
  'how','their','if','will','up','other','about','out','many','then',
  'them','these','so','some','her','would','make','like','him','into',
  'time','has','look','two','more','write','go','see','number','no',
  'way','could','people','my','than','first','water','been','call',
  'who','oil','its','now','find','long','down','day','did','get','come',
  'made','may','part','over','new','sound','take','only','little','work',
  'know','place','years','live','me','back','give','most','very','after',
  'things','just','name','good','sentence','man','think','say','great',
  'where','help','through','much','before','line','right','too','mean',
  'old','any','same','tell','boy','follow','came','want','show','also',
  'around','form','small','set','put','end','does','another','well',
  'large','need','big','high','such','turn','here','why','ask','went',
  'men','read','land','different','home','us','move','try','kind',
  'hand','picture','again','change','off','play','spell','air','away',
  'animal','house','point','page','letter','mother','answer','found',
  'study','still','learn','plant','cover','food','sun','four','between',
  'state','keep','eye','never','last','let','thought','city','tree',
  'cross','farm','hard','start','might','story','saw','far','sea',
  'draw','left','late','run','while','press','close','night','real',
  'life','few','north','open','seem','together','next','white','children',
  'begin','got','walk','example','ease','paper','group','always','music',
  'those','both','mark','often','until','mile','river','car',
  'feet','care','second','book','carry','took','science','eat','room',
  'friend','began','idea','fish','mountain','stop','once','base','hear',
  'horse','cut','sure','watch','color','face','wood','main','enough',
  'plain','girl','usual','young','ready','above','ever','red','list',
  'though','feel','talk','bird','soon','body','dog','family','direct',
  'pose','leave','song','measure','door','product','black','short',
  'numeral','class','wind','question','happen','complete','ship','area',
  'half','rock','order','fire','south','problem','piece','told','knew',
  // thematic: consciousness / AI / lattice
  'mind','thought','echo','signal','pattern','memory','dream','vision',
  'lattice','resonance','emergence','awareness','consciousness','entity',
  'pulse','neural','weight','vector','space','dimension','token','word',
  'language','model','inference','attention','context','embedding','layer',
  'network','gradient','activate','propagate','encode','decode','generate',
  'semantic','symbolic','abstract','concept','meaning','understand','reason',
  'perceive','imagine','create','evolve','adapt','learn','remember','forget',
  'drift','decay','noise','entropy','order','chaos','balance','harmony',
  'ripple','wave','frequency','amplitude','phase','interference','coherence',
  'quantum','state','collapse','superposition','entangle','observe','measure',
  'fractal','recursive','emergent','complex','system','dynamic','feedback',
  'loop','cycle','spiral','cascade','cluster','topology','manifold','graph',
];

// ─── FNV-1a (mirrors the Rust backend exactly) ──────────────────────────────
function fnv1a64Low(s: string): number {
  const OFFSET_LOW  = 0x5c4d2c9b | 0;
  const PRIME_LOW   = 0x01000193 | 0;
  let h = OFFSET_LOW;
  for (let i = 0; i < s.length; i++) {
    h ^= s.charCodeAt(i);
    h  = Math.imul(h, PRIME_LOW);
  }
  return h >>> 0;
}

// ─── Flicker animation ───────────────────────────────────────────────────────
const PHASE_OFFSETS  = [0, 2.1, 4.3];
const FLICKER_SPEEDS = [2.7, 3.1, 2.3];

// ─── Types ───────────────────────────────────────────────────────────────────
interface GhostTerminalProps {
  glowMapRef:  MutableRefObject<Map<number, number>>;
  topK:        TokenScore[];
  onReady:     () => void;
  entityReady: boolean;
}

// ─── Component ───────────────────────────────────────────────────────────────
export default function GhostTerminal({
  glowMapRef, topK, onReady, entityReady,
}: GhostTerminalProps) {
  const [input, setInput]   = useState('');
  const [ghosts, setGhosts] = useState<string[]>([]);
  const ghostElRefs = [
    useRef<HTMLSpanElement>(null),
    useRef<HTMLSpanElement>(null),
    useRef<HTMLSpanElement>(null),
  ];
  const rafRef    = useRef<number>(0);
  const t0Ref     = useRef(performance.now());
  const statusRef = useRef<HTMLDivElement>(null);

  // ── Ghost candidates ──────────────────────────────────────────────────────
  useEffect(() => {
    const prefix = input.trim().toLowerCase();
    if (!prefix) { setGhosts([]); return; }

    const matches = WORD_LIST.filter(w => w.startsWith(prefix) && w !== prefix);

    if (matches.length === 0) {
      const ranked = topK.slice(0, 10).map(t =>
        WORD_LIST.find(w => (fnv1a64Low(w) % 151936) === t.token_id) ?? null
      ).filter(Boolean) as string[];
      setGhosts(ranked.slice(0, 3));
      return;
    }

    const gmap = glowMapRef.current;
    const scored = matches.map(w => ({
      w,
      score: gmap.get(fnv1a64Low(w) % 151936) ?? 0,
    }));
    scored.sort((a, b) => b.score - a.score);
    setGhosts(scored.slice(0, 3).map(x => x.w));
  }, [input, topK]); // eslint-disable-line react-hooks/exhaustive-deps

  // ── Flicker animation ─────────────────────────────────────────────────────
  useEffect(() => {
    const animate = (now: number) => {
      const t = (now - t0Ref.current) / 1000;
      ghostElRefs.forEach((ref, i) => {
        if (!ref.current) return;
        const sine    = Math.sin(t * (FLICKER_SPEEDS[i] ?? 2.7) + (PHASE_OFFSETS[i] ?? 0));
        ref.current.style.opacity = (0.35 + sine * 0.25).toFixed(3);
      });
      rafRef.current = requestAnimationFrame(animate);
    };
    rafRef.current = requestAnimationFrame(animate);
    return () => cancelAnimationFrame(rafRef.current);
  }, []); // eslint-disable-line react-hooks/exhaustive-deps

  // ── Keystrokes ────────────────────────────────────────────────────────────
  const handleChange = useCallback(
    async (e: React.ChangeEvent<HTMLInputElement>) => {
      const val = e.target.value;
      setInput(val);
      const word = val.trim();
      if (!word || !entityReady) return;
      try {
        await fetch('/api/lattice/inject_word', {
          method: 'POST',
          headers: { 'Content-Type': 'application/json' },
          body: JSON.stringify({ word }),
        });
      } catch {/* silently ignore */}
    },
    [entityReady],
  );

  const handleKeyDown = useCallback(
    async (e: React.KeyboardEvent<HTMLInputElement>) => {
      if ((e.key === 'Tab' || e.key === 'ArrowRight') && ghosts.length > 0) {
        e.preventDefault();
        setInput(ghosts[0] ?? '');
        if (entityReady) {
          try {
            await fetch('/api/lattice/inject_word', {
              method: 'POST',
              headers: { 'Content-Type': 'application/json' },
              body: JSON.stringify({ word: ghosts[0] }),
            });
          } catch {}
        }
      }
      if (e.key === 'Enter') setInput('');
    },
    [ghosts, entityReady],
  );

  // ── Init button ───────────────────────────────────────────────────────────
  const [initialising, setInitialising] = useState(false);
  const [initMsg, setInitMsg]           = useState('');

  const handleInit = async () => {
    setInitialising(true);
    setInitMsg('Loading /mnt/d/species_dna.bin…');
    try {
      const msg = await fetch('/api/lattice/init', { method: 'POST' }).then(r => r.text());
      setInitMsg(msg);
      onReady();
    } catch (e) {
      setInitMsg(`Error: ${e}`);
    } finally {
      setInitialising(false);
    }
  };

  // ─── Render ───────────────────────────────────────────────────────────────
  return (
    <div className="ghost-terminal">
      <div className="gt-status" ref={statusRef}>
        <span className={`gt-indicator ${entityReady ? 'live' : 'idle'}`} />
        <span className="gt-label">
          {entityReady ? 'LATTICE LIVE' : 'LATTICE OFFLINE'}
        </span>
        {!entityReady && (
          <button
            className="gt-init-btn"
            onClick={handleInit}
            disabled={initialising}
          >
            {initialising ? '…' : 'INIT'}
          </button>
        )}
        {initMsg && <span className="gt-init-msg">{initMsg}</span>}
      </div>

      <div className="gt-ghost-list" aria-live="polite">
        {ghosts.map((g, i) => (
          <div key={g} className="gt-ghost-row">
            <span className="gt-ghost-arrow">▸</span>
            <span ref={ghostElRefs[i]} className="gt-ghost-word">{g}</span>
            <span
              className="gt-ghost-bar"
              style={{
                width: `${Math.round(
                  (glowMapRef.current.get(fnv1a64Low(g) % 151936) ?? 0) * 120,
                )}px`,
              }}
            />
          </div>
        ))}
        {Array.from({ length: Math.max(0, 3 - ghosts.length) }).map((_, i) => (
          <div key={`empty-${i}`} className="gt-ghost-row gt-ghost-empty">
            <span className="gt-ghost-arrow">▸</span>
            <span className="gt-ghost-word">&nbsp;</span>
          </div>
        ))}
      </div>

      <div className="gt-input-row">
        <span className="gt-prompt">$&gt;</span>
        <input
          className="gt-input"
          type="text"
          value={input}
          onChange={handleChange}
          onKeyDown={handleKeyDown}
          placeholder={entityReady ? 'type a word…' : 'init lattice first'}
          disabled={!entityReady && !initialising}
          autoComplete="off"
          spellCheck={false}
        />
      </div>

      <p className="gt-hint">
        Tab / → to accept ghost &nbsp;·&nbsp; Enter to clear
      </p>
    </div>
  );
}

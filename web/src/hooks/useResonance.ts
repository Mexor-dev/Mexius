// ============================================================================
//  useResonance.ts
//
//  Polls `GET /api/lattice/top_k` every 16 ms and returns:
//   • topK  — latest array of { token_id, distance } sorted ascending
//   • glowMapRef — ref to Map<tokenId, glowValue 0..1> updated in-place
//     (updated without triggering React re-renders so the WebGL loop can
//      read it every rAF without coupling to the React render cycle)
// ============================================================================

import { useEffect, useRef, useState } from 'react';

export interface TokenScore {
  token_id: number;
  distance: number;
}

/**
 * Polls the Rust backend at ~60 fps and keeps a live resonance map.
 *
 * @param active  Set to false to pause polling (e.g. while Entity is loading).
 */
export function useResonance(active: boolean) {
  const [topK, setTopK]             = useState<TokenScore[]>([]);
  const glowMapRef                  = useRef<Map<number, number>>(new Map());

  useEffect(() => {
    if (!active) return;

    let running = true;

    const poll = async () => {
      if (!running) return;
      try {
        const scores: TokenScore[] = await fetch('/api/lattice/top_k').then(r => r.json());

        if (scores.length > 0) {
          // ── Build glow map ────────────────────────────────────────────────
          // Strategy: rank-based glow so the closest token always has full
          // brightness (1.0) and the 100th has ~0.  Then square for contrast.
          const map = new Map<number, number>();
          const n   = scores.length;
          scores.forEach((s, i) => {
            const rank = 1.0 - i / n;          // 1.0 → 0.0
            map.set(s.token_id, rank * rank);   // square for more contrast
          });
          glowMapRef.current = map;
          setTopK(scores);
        }
      } catch {
        // Entity not yet initialised — silently ignore until lattice_init
        // is called and the backend becomes ready.
      }

      if (running) setTimeout(poll, 16);
    };

    poll();
    return () => { running = false; };
  }, [active]);

  return { topK, glowMapRef };
}

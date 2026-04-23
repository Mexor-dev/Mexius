/**
 * lattice_api.ts
 *
 * Tauri IPC wrapper that polls the Lattice Kernel every 16 ms and exposes
 * reactive state to the UI.
 *
 * Assumes the lattice_kernel native sidecar or plugin exposes:
 *   - Command: "lattice_init"     { path: string }  → void
 *   - Command: "lattice_thought"  { bits: number[] } → void   (1280 bytes as u8 array)
 *   - Command: "lattice_top_k"    {}                → TokenScore[]
 */

import { invoke } from "@tauri-apps/api/core";

export interface TokenScore {
  token_id: number;
  distance: number;
}

export type TopKCallback = (tokens: TokenScore[]) => void;

let pollTimer: ReturnType<typeof setInterval> | null = null;
let listeners: Set<TopKCallback> = new Set();

// ─── Init ────────────────────────────────────────────────────────────────────

/**
 * Initialize the Lattice Kernel backend and start the 16 ms polling loop.
 * Call once from your Tauri app entry point.
 */
export async function latticeInit(dnaPath: string): Promise<void> {
  await invoke<void>("lattice_init", { path: dnaPath });
  startPolling();
}

// ─── Submit a Thought Vector ─────────────────────────────────────────────────

/**
 * Submit a 1280-byte (10 240-bit) Thought Vector.
 * `bits` must be a Uint8Array of length 1280.
 */
export async function submitThought(bits: Uint8Array): Promise<void> {
  if (bits.length !== 1280) {
    throw new Error(`Thought vector must be 1280 bytes, got ${bits.length}`);
  }
  await invoke<void>("lattice_thought", { bits: Array.from(bits) });
}

// ─── Subscribe to resonance updates ─────────────────────────────────────────

/**
 * Register a callback that fires every ~16 ms with the latest Top-50 tokens.
 * Returns an unsubscribe function.
 */
export function onTopK(cb: TopKCallback): () => void {
  listeners.add(cb);
  return () => listeners.delete(cb);
}

// ─── Internal polling ────────────────────────────────────────────────────────

function startPolling() {
  if (pollTimer !== null) return;
  pollTimer = setInterval(async () => {
    try {
      const tokens = await invoke<TokenScore[]>("lattice_top_k", {});
      for (const cb of listeners) cb(tokens);
    } catch {
      // Kernel not ready yet; ignore silently
    }
  }, 16);
}

/** Stop polling and clean up. */
export function latticeDestroy() {
  if (pollTimer !== null) {
    clearInterval(pollTimer);
    pollTimer = null;
  }
  listeners.clear();
  invoke("lattice_destroy").catch(() => {});
}

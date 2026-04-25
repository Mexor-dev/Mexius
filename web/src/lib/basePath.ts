// Runtime base path injected by the Rust gateway into index.html.
// Allows the SPA to work under a reverse-proxy path prefix.
// When running inside Tauri, the frontend is served from disk so basePath is
// empty and API calls target the gateway URL directly.

import { isTauri, tauriGatewayUrl } from './tauri';

declare global {
  interface Window {
    __MEXIUS_BASE__?: string;
    __MEXIUS_GATEWAY__?: string;
  }
}

/** Gateway path prefix (e.g. "/mexius"), or empty string when served at root. */
export const basePath: string = isTauri()
  ? ''
  : (window.__MEXIUS_BASE__ ?? '').replace(/\/+$/, '');

/** Full origin for API requests. Defaults to Tauri gateway or the current origin. */
export const apiOrigin: string = isTauri()
  ? tauriGatewayUrl()
  : (window.__MEXIUS_GATEWAY__ ?? window.location.origin);

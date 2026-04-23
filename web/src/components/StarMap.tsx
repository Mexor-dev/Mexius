// ============================================================================
//  StarMap.tsx — WebGL2 Topographic Visualiser
//
//  Renders all 151,936 token dots as GL_POINTS on a black canvas.
//  Resonant tokens glow cyan and scale up; the top-5 are linked by lines
//  representing the current "train of thought."
//
//  Data sources
//  ────────────
//  • /topology_map.bin — Float32 interleaved XY for every token (static)
//  • glowMapRef        — ref updated by useResonance every 16 ms (dynamic)
//  • top5Ids           — token IDs of the 5 most resonant tokens
//
//  Render loop
//  ───────────
//  requestAnimationFrame runs at ~60 fps, independent of React renders:
//    1. Read glowMapRef.current
//    2. Build glowData Float32Array: 0 for all, 0-1 for top tokens
//    3. Upload glowData to GPU via bufferData (DYNAMIC_DRAW, ~608 KB)
//    4. Draw all tokens as GL_POINTS with additive blending
//    5. Draw LINE_STRIP through top-5 positions
// ============================================================================

import { useEffect, useRef, useState } from 'react';
import type { MutableRefObject, RefObject } from 'react';

// ─── Shader sources ──────────────────────────────────────────────────────────

const VERT_STARS = /* glsl */`#version 300 es
precision mediump float;

in vec2  a_pos;
in float a_glow;

uniform float u_aspect;  // canvas width / height

out float v_glow;

void main() {
  // Fit the square [-1,1]² topology into the viewport while preserving aspect
  vec2 pos = a_pos;
  if (u_aspect > 1.0) {
    pos.x /= u_aspect;   // wide canvas: compress x
  } else {
    pos.y *= u_aspect;   // tall canvas: compress y
  }

  gl_Position = vec4(pos, 0.0, 1.0);
  // Base size 1.5 px; resonant tokens scale up to ~15 px at glow=1
  gl_PointSize = 1.5 + a_glow * 13.5;
  v_glow = a_glow;
}`;

const FRAG_STARS = /* glsl */`#version 300 es
precision mediump float;

in float v_glow;
out vec4 fragColor;

void main() {
  // Circular point: discard corners
  vec2  c = gl_PointCoord * 2.0 - 1.0;
  float r = dot(c, c);
  if (r > 1.0) discard;

  float edge = 1.0 - r;                         // soft edge
  vec3  base = vec3(0.12, 0.15, 0.26);          // dim blue star
  vec3  glow = vec3(0.05, 0.88, 1.00);          // cyan resonance
  vec3  col  = mix(base, glow, v_glow);
  float a    = mix(0.07, 1.00, v_glow) * edge;
  fragColor  = vec4(col, a);
}`;

const VERT_LINES = /* glsl */`#version 300 es
precision mediump float;
in vec2 a_pos;
uniform float u_aspect;
void main() {
  vec2 pos = a_pos;
  if (u_aspect > 1.0) { pos.x /= u_aspect; }
  else                 { pos.y *= u_aspect; }
  gl_Position = vec4(pos, 0.0, 1.0);
}`;

const FRAG_LINES = /* glsl */`#version 300 es
precision mediump float;
out vec4 fragColor;
void main() {
  fragColor = vec4(0.15, 0.75, 1.0, 0.22);
}`;

// ─── Helpers ─────────────────────────────────────────────────────────────────

function compileShader(gl: WebGL2RenderingContext, type: number, src: string): WebGLShader {
  const s = gl.createShader(type)!;
  gl.shaderSource(s, src);
  gl.compileShader(s);
  if (!gl.getShaderParameter(s, gl.COMPILE_STATUS))
    throw new Error('Shader compile error: ' + gl.getShaderInfoLog(s));
  return s;
}

function linkProgram(
  gl: WebGL2RenderingContext,
  vert: string,
  frag: string,
): WebGLProgram {
  const prog = gl.createProgram()!;
  gl.attachShader(prog, compileShader(gl, gl.VERTEX_SHADER,   vert));
  gl.attachShader(prog, compileShader(gl, gl.FRAGMENT_SHADER, frag));
  gl.linkProgram(prog);
  if (!gl.getProgramParameter(prog, gl.LINK_STATUS))
    throw new Error('Program link error: ' + gl.getProgramInfoLog(prog));
  return prog;
}

// ─── Component ───────────────────────────────────────────────────────────────

interface StarMapProps {
  glowMapRef: MutableRefObject<Map<number, number>> | RefObject<Map<number, number>>;
  topK: { token_id: number; distance: number }[];
  className?: string;
}

type GLState = {
  gl:          WebGL2RenderingContext;
  starProg:    WebGLProgram;
  lineProg:    WebGLProgram;
  starVao:     WebGLVertexArrayObject;
  lineVao:     WebGLVertexArrayObject;
  glowVbo:     WebGLBuffer;
  lineVbo:     WebGLBuffer;
  positions:   Float32Array;   // [x0,y0,x1,y1,...] indexed by token_id
  glowData:    Float32Array;   // [glow0, glow1,...] one per token
  nTokens:     number;
  aspectLoc_s: WebGLUniformLocation;
  aspectLoc_l: WebGLUniformLocation;
};

export default function StarMap({ glowMapRef, topK, className }: StarMapProps) {
  const canvasRef  = useRef<HTMLCanvasElement>(null);
  const glStateRef = useRef<GLState | null>(null);
  const rafRef     = useRef<number>(0);
  const [status, setStatus] = useState<'loading' | 'ready' | 'error'>('loading');
  const [statusMsg, setStatusMsg] = useState('Loading topology…');

  // ── Initialise WebGL2 + load topology ──────────────────────────────────────
  useEffect(() => {
    const canvas = canvasRef.current;
    if (!canvas) return;

    const gl = canvas.getContext('webgl2', {
      antialias: false,
      alpha:     false,
      depth:     false,
      stencil:   false,
    });
    if (!gl) {
      setStatus('error');
      setStatusMsg('WebGL2 not supported in this WebView.');
      return;
    }

    let cancelled = false;

    (async () => {
      // ── Load topology binary ─────────────────────────────────────────────
      let positions: Float32Array;
      try {
        const res = await fetch('/topology_map.bin');
        if (!res.ok) throw new Error(`HTTP ${res.status}`);
        positions = new Float32Array(await res.arrayBuffer());
      } catch (e) {
        if (!cancelled) {
          setStatus('error');
          setStatusMsg(`topology_map.bin not found — run generate_topology.py first. (${e})`);
        }
        return;
      }

      if (cancelled) return;

      const nTokens = positions.length / 2;
      setStatusMsg(`Building GPU buffers for ${nTokens.toLocaleString()} tokens…`);

      // ── Shader programs ──────────────────────────────────────────────────
      let starProg: WebGLProgram, lineProg: WebGLProgram;
      try {
        starProg = linkProgram(gl, VERT_STARS, FRAG_STARS);
        lineProg = linkProgram(gl, VERT_LINES, FRAG_LINES);
      } catch (e) {
        if (!cancelled) { setStatus('error'); setStatusMsg(String(e)); }
        return;
      }

      // ── glowData buffer (all zeros initially) ────────────────────────────
      const glowData = new Float32Array(nTokens);

      // ── Star VAO ─────────────────────────────────────────────────────────
      const starVao    = gl.createVertexArray()!;
      const posVbo     = gl.createBuffer()!;
      const glowVbo    = gl.createBuffer()!;

      gl.bindVertexArray(starVao);

      // Position VBO (STATIC)
      const posLoc_s = gl.getAttribLocation(starProg, 'a_pos');
      gl.bindBuffer(gl.ARRAY_BUFFER, posVbo);
      gl.bufferData(gl.ARRAY_BUFFER, positions, gl.STATIC_DRAW);
      gl.enableVertexAttribArray(posLoc_s);
      gl.vertexAttribPointer(posLoc_s, 2, gl.FLOAT, false, 0, 0);

      // Glow VBO (DYNAMIC)
      const glowLoc = gl.getAttribLocation(starProg, 'a_glow');
      gl.bindBuffer(gl.ARRAY_BUFFER, glowVbo);
      gl.bufferData(gl.ARRAY_BUFFER, glowData, gl.DYNAMIC_DRAW);
      gl.enableVertexAttribArray(glowLoc);
      gl.vertexAttribPointer(glowLoc, 1, gl.FLOAT, false, 0, 0);

      gl.bindVertexArray(null);

      // ── Line VAO ─────────────────────────────────────────────────────────
      const lineVao = gl.createVertexArray()!;
      const lineVbo = gl.createBuffer()!;

      gl.bindVertexArray(lineVao);
      const posLoc_l = gl.getAttribLocation(lineProg, 'a_pos');
      gl.bindBuffer(gl.ARRAY_BUFFER, lineVbo);
      gl.bufferData(gl.ARRAY_BUFFER, new Float32Array(10), gl.DYNAMIC_DRAW); // 5×2
      gl.enableVertexAttribArray(posLoc_l);
      gl.vertexAttribPointer(posLoc_l, 2, gl.FLOAT, false, 0, 0);
      gl.bindVertexArray(null);

      const aspectLoc_s = gl.getUniformLocation(starProg, 'u_aspect')!;
      const aspectLoc_l = gl.getUniformLocation(lineProg, 'u_aspect')!;

      glStateRef.current = {
        gl, starProg, lineProg,
        starVao, lineVao,
        glowVbo, lineVbo,
        positions, glowData,
        nTokens,
        aspectLoc_s, aspectLoc_l,
      };

      if (!cancelled) setStatus('ready');
    })();

    return () => { cancelled = true; };
  }, []);

  // ── rAF render loop ────────────────────────────────────────────────────────
  useEffect(() => {
    if (status !== 'ready') return;

    const render = () => {
      const s = glStateRef.current;
      if (!s || !canvasRef.current) return;
      const { gl, starProg, lineProg, starVao, lineVao,
              glowVbo, lineVbo, positions, glowData,
              nTokens, aspectLoc_s, aspectLoc_l } = s;

      const canvas  = canvasRef.current;
      const w       = canvas.clientWidth;
      const h       = canvas.clientHeight;
      const aspect  = w / h;

      // Resize backing store if layout changed
      if (canvas.width !== w || canvas.height !== h) {
        canvas.width  = w;
        canvas.height = h;
        gl.viewport(0, 0, w, h);
      }

      // ── Update glow data ────────────────────────────────────────────────
      const gmap = glowMapRef.current;
      glowData.fill(0);
      gmap.forEach((v, id) => {
        if (id < nTokens) glowData[id] = v;
      });

      // ── Upload to GPU ────────────────────────────────────────────────────
      gl.bindBuffer(gl.ARRAY_BUFFER, glowVbo);
      gl.bufferData(gl.ARRAY_BUFFER, glowData, gl.DYNAMIC_DRAW);

      // ── Clear ────────────────────────────────────────────────────────────
      gl.clearColor(0.0, 0.0, 0.03, 1.0);
      gl.clear(gl.COLOR_BUFFER_BIT);

      // ── Additive blending for glow overlay ───────────────────────────────
      gl.enable(gl.BLEND);
      gl.blendFunc(gl.SRC_ALPHA, gl.ONE);   // additive: bright things stack

      // ── Draw all stars ───────────────────────────────────────────────────
      gl.useProgram(starProg);
      gl.uniform1f(aspectLoc_s, aspect);
      gl.bindVertexArray(starVao);
      gl.drawArrays(gl.POINTS, 0, nTokens);
      gl.bindVertexArray(null);

      // ── Draw connection lines for top-5 ─────────────────────────────────
      // Extract top-5 from topK (passed via closure via ref below)
      const top5 = topKRef.current.slice(0, 5);
      if (top5.length >= 2) {
        const lineXY = new Float32Array(top5.length * 2);
        top5.forEach((t, i) => {
          const idx = t.token_id;
          lineXY[i * 2]     = idx < nTokens ? (positions[idx * 2] ?? 0)     : 0;
          lineXY[i * 2 + 1] = idx < nTokens ? (positions[idx * 2 + 1] ?? 0) : 0;
        });
        gl.bindBuffer(gl.ARRAY_BUFFER, lineVbo);
        gl.bufferData(gl.ARRAY_BUFFER, lineXY, gl.DYNAMIC_DRAW);
        gl.useProgram(lineProg);
        gl.uniform1f(aspectLoc_l, aspect);
        gl.bindVertexArray(lineVao);
        gl.drawArrays(gl.LINE_STRIP, 0, top5.length);
        gl.bindVertexArray(null);
      }

      gl.disable(gl.BLEND);
      rafRef.current = requestAnimationFrame(render);
    };

    rafRef.current = requestAnimationFrame(render);
    return () => cancelAnimationFrame(rafRef.current);
  }, [status, glowMapRef]); // eslint-disable-line react-hooks/exhaustive-deps

  // Keep a ref to topK so the rAF loop can read latest without re-subscribe
  const topKRef = useRef(topK);
  useEffect(() => { topKRef.current = topK; }, [topK]);

  // ─── Render ───────────────────────────────────────────────────────────────

  return (
    <div className={`star-map-wrapper ${className ?? ''}`}>
      <canvas ref={canvasRef} className="star-map-canvas" />
      {status !== 'ready' && (
        <div className={`star-map-overlay ${status}`}>
          {status === 'loading' && <span className="blink">{statusMsg}</span>}
          {status === 'error'   && <span className="error-msg">{statusMsg}</span>}
        </div>
      )}
    </div>
  );
}

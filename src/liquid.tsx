import { useEffect, useRef } from "react";
import { invoke } from "@tauri-apps/api/core";

/**
 * Liquid glass: draws what is behind the dock, bent like a thick glass lens.
 *
 * Rust hands over the strip of screen behind the bar (half size, with a margin
 * so the rim can pull in light from just outside). This shader then, per pixel:
 *  - finds how far it is from the bar's rounded edge,
 *  - near the edge, samples the picture further out along the edge normal
 *    (the bend), splitting red and blue slightly (dispersion),
 *  - softens it with a small blur, tints it, and adds a bright rim and a faint
 *    top highlight.
 * New frames are fetched every animation frame while the picture is changing,
 * and only a few times a second once it is still; unchanged frames are not
 * redrawn at all.
 */

const VERT = `#version 300 es
in vec2 aPos;
void main() { gl_Position = vec4(aPos, 0.0, 1.0); }`;

const FRAG = `#version 300 es
precision highp float;
uniform sampler2D uTex;
uniform vec2 uTexSize;   // frame size, frame px
uniform vec2 uBarOff;    // bar's top-left inside the frame, frame px
uniform float uScale;    // canvas px -> frame px
uniform vec2 uSize;      // canvas size, px
uniform float uRadius;   // corner radius, px
uniform float uDpr;
uniform vec4 uTint;      // rgb + strength
out vec4 outColor;

float sdRoundBox(vec2 p, vec2 b, float r) {
  vec2 q = abs(p) - b + r;
  return length(max(q, 0.0)) + min(max(q.x, q.y), 0.0) - r;
}

// The frame is BGRA in memory, uploaded as RGBA: swizzle back.
vec3 sampleAt(vec2 px) {
  vec2 uv = (uBarOff + px * uScale) / uTexSize;
  return texture(uTex, uv).bgr;
}

void main() {
  vec2 p = vec2(gl_FragCoord.x, uSize.y - gl_FragCoord.y);
  vec2 half_ = uSize * 0.5;
  vec2 q = p - half_;
  float d = sdRoundBox(q, half_, uRadius);
  if (d > 1.0) { outColor = vec4(0.0); return; }

  // Edge normal from the distance field's gradient (points outwards).
  vec2 e = vec2(1.0, 0.0);
  vec2 n = normalize(vec2(
    sdRoundBox(q + e.xy, half_, uRadius) - sdRoundBox(q - e.xy, half_, uRadius),
    sdRoundBox(q + e.yx, half_, uRadius) - sdRoundBox(q - e.yx, half_, uRadius)) + 1e-6);

  // 0 in the middle of the glass, 1 right at the rim.
  float bezel = 14.0 * uDpr;
  float t = clamp(1.0 + d / bezel, 0.0, 1.0);
  float bend = pow(t, 2.4) * 26.0 * uDpr;
  float split = t * t * 2.2 * uDpr;
  vec2 sp = p + n * bend;

  vec3 col = vec3(0.0);
  float wsum = 0.0;
  for (int i = -1; i <= 1; i++) {
    for (int j = -1; j <= 1; j++) {
      vec2 o = vec2(float(i), float(j)) * 1.4 * uDpr;
      float k = (i == 0 && j == 0) ? 2.0 : 1.0;
      col += k * vec3(sampleAt(sp + o + n * split).r, sampleAt(sp + o).g, sampleAt(sp + o - n * split).b);
      wsum += k;
    }
  }
  col /= wsum;

  // A touch more colour, then the tint.
  float luma = dot(col, vec3(0.299, 0.587, 0.114));
  col = mix(vec3(luma), col, 1.22);
  col = mix(col, uTint.rgb, uTint.a);

  // Light: a bright rim where the glass curves, and a soft highlight on top.
  float rim = pow(t, 4.0) * 0.34;
  float topLight = smoothstep(0.55, 0.0, p.y / uSize.y) * 0.08;
  col += vec3(rim + topLight);

  float alpha = clamp(0.5 - d, 0.0, 1.0);
  outColor = vec4(clamp(col, 0.0, 1.0) * alpha, alpha);
}`;

function compile(gl: WebGL2RenderingContext, type: number, src: string) {
  const s = gl.createShader(type)!;
  gl.shaderSource(s, src);
  gl.compileShader(s);
  if (!gl.getShaderParameter(s, gl.COMPILE_STATUS)) throw new Error(gl.getShaderInfoLog(s) ?? "shader error");
  return s;
}

function isDark() {
  const theme = document.documentElement.dataset.theme;
  if (theme === "dark") return true;
  if (theme === "light") return false;
  return window.matchMedia("(prefers-color-scheme: dark)").matches;
}

export function LiquidGlass({ radius }: { radius: number }) {
  const canvas = useRef<HTMLCanvasElement>(null);

  useEffect(() => {
    const el = canvas.current;
    if (!el) return;
    const gl = el.getContext("webgl2", { premultipliedAlpha: true, alpha: true, antialias: false, preserveDrawingBuffer: true });
    if (!gl) return;

    const prog = gl.createProgram()!;
    gl.attachShader(prog, compile(gl, gl.VERTEX_SHADER, VERT));
    gl.attachShader(prog, compile(gl, gl.FRAGMENT_SHADER, FRAG));
    gl.linkProgram(prog);
    gl.useProgram(prog);
    const buf = gl.createBuffer();
    gl.bindBuffer(gl.ARRAY_BUFFER, buf);
    gl.bufferData(gl.ARRAY_BUFFER, new Float32Array([-1, -1, 1, -1, -1, 1, 1, 1]), gl.STATIC_DRAW);
    const loc = gl.getAttribLocation(prog, "aPos");
    gl.enableVertexAttribArray(loc);
    gl.vertexAttribPointer(loc, 2, gl.FLOAT, false, 0, 0);

    const tex = gl.createTexture();
    gl.bindTexture(gl.TEXTURE_2D, tex);
    gl.texParameteri(gl.TEXTURE_2D, gl.TEXTURE_MIN_FILTER, gl.LINEAR);
    gl.texParameteri(gl.TEXTURE_2D, gl.TEXTURE_MAG_FILTER, gl.LINEAR);
    gl.texParameteri(gl.TEXTURE_2D, gl.TEXTURE_WRAP_S, gl.CLAMP_TO_EDGE);
    gl.texParameteri(gl.TEXTURE_2D, gl.TEXTURE_WRAP_T, gl.CLAMP_TO_EDGE);

    const u = (name: string) => gl.getUniformLocation(prog, name);
    let frameW = 0;
    let frameH = 0;
    let barX = 0;
    let barY = 0;
    let scale = 2;

    const draw = () => {
      if (!frameW) return;
      const dpr = window.devicePixelRatio || 1;
      const w = Math.round(el.clientWidth * dpr);
      const h = Math.round(el.clientHeight * dpr);
      if (el.width !== w || el.height !== h) {
        el.width = w;
        el.height = h;
      }
      gl.viewport(0, 0, w, h);
      gl.uniform2f(u("uTexSize"), frameW, frameH);
      gl.uniform2f(u("uBarOff"), barX, barY);
      // Canvas px are the bar's physical px when dpr matches the screen scale.
      gl.uniform1f(u("uScale"), 1 / scale);
      gl.uniform2f(u("uSize"), w, h);
      gl.uniform1f(u("uRadius"), radius * dpr);
      gl.uniform1f(u("uDpr"), dpr);
      const dark = isDark();
      // A light veil only: the glass should read as clear, with the bend and rim doing the work.
      gl.uniform4f(u("uTint"), dark ? 0.07 : 1, dark ? 0.08 : 1, dark ? 0.1 : 1, dark ? 0.13 : 0.16);
      gl.drawArrays(gl.TRIANGLE_STRIP, 0, 4);
    };

    let alive = true;
    let lastChange = 0;
    let force = true;
    const pump = async () => {
      if (!alive) return;
      try {
        const data = await invoke<ArrayBuffer>("glass_frame", { force });
        force = false;
        const head = new Uint32Array(data, 0, 6);
        if (head[0] === 1) {
          [frameW, frameH, barX, barY, scale] = [head[1], head[2], head[3], head[4], head[5]];
          gl.bindTexture(gl.TEXTURE_2D, tex);
          gl.texImage2D(gl.TEXTURE_2D, 0, gl.RGBA, frameW, frameH, 0, gl.RGBA, gl.UNSIGNED_BYTE, new Uint8Array(data, 24));
          draw();
          lastChange = performance.now();
        }
      } catch (e) {
        console.error(e);
      }
      if (!alive) return;
      // While things move, ask again at once (the next frame is captured while
      // this one is shown); once nothing has changed for 1.5 s, check 3x a second.
      if (performance.now() - lastChange < 1500) setTimeout(pump, 0);
      else setTimeout(pump, 300);
    };
    pump();

    // Redraw (without a new frame) when the bar resizes or the theme flips.
    const ro = new ResizeObserver(() => {
      force = true;
      draw();
    });
    ro.observe(el);
    const mo = new MutationObserver(draw);
    mo.observe(document.documentElement, { attributes: true, attributeFilter: ["data-theme"] });
    return () => {
      alive = false;
      ro.disconnect();
      mo.disconnect();
    };
  }, [radius]);

  return <canvas ref={canvas} className="liquid-canvas" aria-hidden />;
}

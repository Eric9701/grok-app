/** Springs + path helpers from the Sand living-mark engine. */

export type Spring = { x: number; v: number; t: number };

export function spring(x: number): Spring {
  return { x, v: 0, t: x };
}

/** Critically-damped-ish spring step (`xl` in the original). */
export function stepSpring(s: Spring, freq: number, damp: number, dt: number): void {
  s.v += (-2 * damp * freq * s.v - freq * freq * (s.x - s.t)) * dt;
  s.x += s.v * dt;
  if (!Number.isFinite(s.x) || !Number.isFinite(s.v)) {
    s.x = s.t;
    s.v = 0;
  }
}

export function lerpPts(
  a: number[][],
  b: number[][],
  t: number,
): number[][] {
  return a.map(([x, y], i) => {
    const [bx, by] = b[i] ?? [x, y];
    return [x + (bx - x) * t, y + (by - y) * t];
  });
}

export function polyPath(pts: number[][]): string {
  if (pts.length === 0) return "";
  return (
    "M" +
    pts.map((p) => `${p[0].toFixed(2)} ${p[1].toFixed(2)}`).join("L") +
    "Z"
  );
}

export function clamp(n: number, lo: number, hi: number): number {
  return Math.min(hi, Math.max(lo, n));
}

export function randBetween(a: number, b: number): number {
  return a + Math.random() * (b - a);
}

export function expSmooth(target: number, dt: number): number {
  return 1 - Math.exp(Math.log(1 - target) * 60 * dt);
}

/**
 * Pet overlay paint cadence. The living mark used to sample + React-commit
 * every rAF (~60fps) while Rust broadcast `pet://cursor` ~15Hz to every
 * webview. On Windows WebView2 that transparent always-on-top path stalls
 * after a long day; restarting the app clears the compositor.
 */

export const PET_PAINT_SPIN_MS = 16;
export const PET_PAINT_LIVE_MS = 33;
export const PET_PAINT_IDLE_MS = 50;
export const PET_PAINT_REST_MS = 120;
export const PET_PAINT_REST_AFTER_MS = 20_000;
export const PET_LOOK_LOCAL_HOLD_MS = 280;
/** Screen-space look expires without a fresh event: a parked cursor must let
 * the overlay fall back to the idle/rest paint tier (Rust stops emitting once
 * the quantized cursor delta stops changing). */
export const PET_LOOK_SCREEN_HOLD_MS = 2_000;
export const PET_LOOK_NEAR_SCALE = 1.35;

export function petPaintMinMs(input: {
  spinning: boolean;
  morphing: boolean;
  trackingLook: boolean;
  idleMs: number;
}): number {
  if (input.spinning) return PET_PAINT_SPIN_MS;
  if (input.morphing || input.trackingLook) return PET_PAINT_LIVE_MS;
  if (input.idleMs >= PET_PAINT_REST_AFTER_MS) return PET_PAINT_REST_MS;
  return PET_PAINT_IDLE_MS;
}

/** Look stays valid while events are fresh AND (for screen space) near the mark. */
export function petLookIsNear(input: {
  fromScreen: boolean;
  at: number;
  now: number;
  dx: number;
  dy: number;
  localR: number;
  localHoldMs?: number;
  screenHoldMs?: number;
}): boolean {
  if (!(input.at > 0)) return false;
  if (
    input.now - input.at >=
    (input.fromScreen
      ? (input.screenHoldMs ?? PET_LOOK_SCREEN_HOLD_MS)
      : (input.localHoldMs ?? PET_LOOK_LOCAL_HOLD_MS))
  ) {
    return false;
  }
  if (input.fromScreen) {
    return Math.hypot(input.dx, input.dy) <= Math.max(1, input.localR) * PET_LOOK_NEAR_SCALE;
  }
  return true;
}

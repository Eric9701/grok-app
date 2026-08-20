/**
 * App verbs + prefs → bloub engine inputs.
 * The morph catalogue stays in `./bloub`; this file is the product mapping.
 */
import {
  DEFAULT_EXPRESSION,
  DEFAULT_SHAPE,
  EXPRESSION_BY_ID,
  SHAPE_BY_ID,
  STATE_BY_ID,
  type ExpressionId,
  type Look,
  type ShapeId,
  type StateId,
} from "./bloub";

export const BLOUB_PICKER_SHAPES = [
  "blob",
  "pebble",
  "squircle",
  "capsule",
  "wedge",
  "hex",
  "cloud",
  "teardrop",
] as const;

export type BloubPickerShape = (typeof BLOUB_PICKER_SHAPES)[number];

const SHAPE_TO_BLOUB: Record<string, ShapeId> = {
  blob: "cercle",
  pebble: "galet",
  bean: "galet",
  egg: "cercle",
  squircle: "squircle",
  tablet: "squircle",
  capsule: "capsule",
  cylinder: "capsule",
  hex: "hexagone",
  gem: "hexagone",
  crystal: "hexagone",
  wedge: "triangle",
  triangle: "triangle",
  shield: "triangle",
  dome: "cercle",
  arch: "squircle",
  cloud: "nuage",
  teardrop: "goutte",
  leaf: "goutte",
};

export const PET_EXPRESSIONS = [
  "neutre",
  "attentif",
  "surpris",
  "excite",
  "heureux",
  "hilare",
  "colere",
  "triste",
  "effraye",
  "mefiant",
  "confus",
  "curieux",
  "fier",
  "timide",
  "blase",
  "somnolent",
] as const;

export type PetExpression = (typeof PET_EXPRESSIONS)[number];

export function isPetExpression(v: string | null | undefined): v is PetExpression {
  return !!v && (PET_EXPRESSIONS as readonly string[]).includes(v);
}

export function normalizePetExpression(v: string | null | undefined): PetExpression {
  return isPetExpression(v) ? v : DEFAULT_EXPRESSION;
}

export function bloubShapeId(shape: string | null | undefined): ShapeId {
  return SHAPE_TO_BLOUB[shape ?? ""] ?? DEFAULT_SHAPE;
}

export function bloubShapeRadii(shape: string | null | undefined): number[] | null {
  return SHAPE_BY_ID.get(bloubShapeId(shape))?.radii ?? null;
}

const VERB_STATE: Record<string, StateId> = {
  idle: "idle",
  thinking: "thinking",
  searching: "comet",
  working: "orbit",
  writing: "hexagon",
  waiting: "alert",
  notifying: "notify",
  sleeping: "sleep",
  waking: "swirl",
  dragging: "egg",
  sad: "exclaim",
};

const VERB_EXPRESSION: Record<string, ExpressionId> = {
  happy: "heureux",
  curious: "curieux",
  playful: "excite",
  shy: "timide",
  proud: "fier",
  bored: "blase",
  drowsy: "somnolent",
  confused: "confus",
  listening: "attentif",
  excited: "excite",
  surprised: "surpris",
  laughing: "hilare",
  scared: "effraye",
  angry: "colere",
  suspicious: "mefiant",
};

export type BloubPlay = {
  state: StateId;
  expression: ExpressionId;
};

export function resolveBloubPlay(
  verb: string,
  restExpression: string | null | undefined,
): BloubPlay {
  const rest = normalizePetExpression(restExpression);
  const expr = VERB_EXPRESSION[verb];
  if (expr) return { state: "idle", expression: expr };
  const state = VERB_STATE[verb];
  if (state) return { state, expression: rest };
  return { state: "idle", expression: rest };
}

export function bloubExpressionOf(id: string | null | undefined) {
  return EXPRESSION_BY_ID.get(normalizePetExpression(id)) ?? null;
}

const HOLD_LOOP = new Set<StateId>(["orbit", "comet", "alert"]);

export function bloubStateDuration(id: StateId): number {
  return STATE_BY_ID.get(id)?.duration ?? 2;
}

/** Narrative states that should restart while the session verb stays put. */
export function bloubShouldLoop(id: StateId): boolean {
  return HOLD_LOOP.has(id);
}

const YAW_MAX = 16;
const PITCH_MAX = 13;
const REST_PITCH = 10;

/** Look at the pointer from the mark centre. No settings-panel left-turn. */
export function bloubLookAtPointer(
  nx: number,
  ny: number,
  pointer: boolean,
): Look {
  const x = Math.max(-1, Math.min(1, nx));
  const y = Math.max(-1, Math.min(1, ny));
  return {
    yaw: x * YAW_MAX,
    pitch: REST_PITCH - y * PITCH_MAX,
    mix: pointer ? 1 : 0,
    spin: 0,
    wander: pointer ? 0 : 1,
  };
}

const NOTIF_INK = "#FF3B1A";
const NOTIF_ALT = "#C8FF00";

function hexClose(a: string, b: string): boolean {
  const pa = parseInt(a.replace("#", "").slice(0, 6), 16);
  const pb = parseInt(b.replace("#", "").slice(0, 6), 16);
  if (!Number.isFinite(pa) || !Number.isFinite(pb)) return false;
  const dr = ((pa >> 16) & 255) - ((pb >> 16) & 255);
  const dg = ((pa >> 8) & 255) - ((pb >> 8) & 255);
  const db = (pa & 255) - (pb & 255);
  return dr * dr + dg * dg + db * db < 70 * 70;
}

/** Unread pastille: vivid orange-red, lime when the body is already that hot. */
export function bloubNotifFill(bodyHex: string): string {
  return hexClose(bodyHex, NOTIF_INK) ? NOTIF_ALT : NOTIF_INK;
}

export function petVerbForComposer(input: {
  sessionVerb: string;
  composing: boolean;
}): string {
  if (
    input.composing &&
    (input.sessionVerb === "idle" || input.sessionVerb === "waking")
  ) {
    return "writing";
  }
  return input.sessionVerb;
}

/** How long after the last keystroke the pet holds the catalog hexagon morph. */
export const PET_COMPOSING_HOLD_MS = 1500;

/** Typing → catalog `hexagon`; empty draft or a pause → original rest shape. */
export function petIsComposing(input: {
  empty: boolean;
  lastTypeAt: number;
  now: number;
  holdMs?: number;
}): boolean {
  if (input.empty) return false;
  if (!(input.lastTypeAt > 0)) return false;
  return input.now - input.lastTypeAt < (input.holdMs ?? PET_COMPOSING_HOLD_MS);
}

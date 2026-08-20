/**
 * Living mark — bloub BotEngine (radial morph, mask-hole eyes, measured states).
 * Overlay chrome (drag, bubbles, menu) stays outside this renderer.
 */
import { useEffect, useId, useRef, useState } from "react";
import { listen } from "@/lib/api/host";
import type { PetColor, PetEyeColor, PetShape, PetVerb } from "@/lib/pet";
import { isPetColor, resolvePetBodyInk, resolvePetEyeInk } from "@/lib/pet";
import {
  BotEngine,
  DEMI_VIEWBOX,
  POSES,
  RAYON,
  mixHex,
  type BotFrame,
} from "@/lib/pet/bloub";
import {
  bloubExpressionOf,
  bloubLookAtPointer,
  bloubNotifFill,
  bloubShapeRadii,
  bloubShouldLoop,
  bloubStateDuration,
  normalizePetExpression,
  resolveBloubPlay,
} from "@/lib/pet/bloubPlay";
import { pickRestEmote, resolveLivingMood } from "@/lib/pet/petMood";
import { MARK_CENTER, verbToMarkState } from "@/lib/pet/markTables";
import { createMarkOrbit } from "@/lib/pet/markOrbit";
import {
  beginPetSpin,
  petSpinWantsBurst,
  pickPetSpinKind,
  stepPetSpin,
  type PetSpinKind,
  type PetSpinRun,
} from "@/lib/pet/markSpin";
import { clamp } from "@/lib/pet/markMath";

function sampleDots(
  frame: BotFrame,
  ink: string,
  paper: string,
  keyPrefix: string,
) {
  return frame.dots.map((dot, i) => {
    const fill =
      dot.color ??
      (dot.depth === undefined ? ink : mixHex(paper, ink, dot.depth));
    if (dot.d) {
      return (
        <path
          key={`${keyPrefix}${i}`}
          d={dot.d}
          transform={`translate(${dot.x} ${dot.y}) rotate(${dot.rot ?? 0}) scale(${RAYON})`}
          fill={fill}
          opacity={dot.opacity}
        />
      );
    }
    return (
      <circle
        key={`${keyPrefix}${i}`}
        cx={dot.x}
        cy={dot.y}
        r={dot.r}
        fill={fill}
        opacity={dot.opacity}
      />
    );
  });
}

export function PetMark({
  shape = "hex",
  color = "green",
  verb = "idle",
  sizePx = 128,
  title,
  paused = false,
  spinSignal = 0,
  emoteSignal = 0,
  dragging = false,
  eyeColor = "auto",
  expression = "neutre",
  restOnly = false,
}: {
  shape?: PetShape | string;
  color?: PetColor;
  eyeColor?: PetEyeColor;
  verb?: PetVerb | string;
  sizePx?: number;
  title?: string;
  paused?: boolean;
  dragging?: boolean;
  spinSignal?: number;
  emoteSignal?: number;
  expression?: string;
  /** Settings picker: selected rest face only — no hover / idle bursts. */
  restOnly?: boolean;
}) {
  const fill = resolvePetBodyInk(isPetColor(color) ? color : "green");
  const eyeInk = resolvePetEyeInk(isPetColor(color) ? color : "green", eyeColor);
  const restExpr = normalizePetExpression(expression);
  const uid = useId().replace(/:/g, "");
  const maskId = `pet-mask-${uid}`;
  const svgRef = useRef<SVGSVGElement>(null);
  const bodySpinRef = useRef<SVGGElement>(null);
  const orbitBackRef = useRef<SVGGElement>(null);
  const orbitFrontRef = useRef<SVGGElement>(null);
  const engineRef = useRef<BotEngine | null>(null);
  const clockRef = useRef(0);
  const verbRef = useRef(verb);
  const shapeRef = useRef(shape);
  const restRef = useRef(restExpr);
  const pausedRef = useRef(paused);
  const draggingRef = useRef(dragging);
  const restOnlyRef = useRef(restOnly);
  const wantSpinRef = useRef(0);
  const playedSpinRef = useRef(0);
  const wantEmoteRef = useRef(0);
  const playedEmoteRef = useRef(0);
  verbRef.current = verb;
  shapeRef.current = shape;
  restRef.current = restExpr;
  pausedRef.current = paused;
  draggingRef.current = dragging;
  restOnlyRef.current = restOnly;
  if (spinSignal > 0) wantSpinRef.current = spinSignal;
  if (emoteSignal > 0) wantEmoteRef.current = emoteSignal;

  const [frame, setFrame] = useState<BotFrame>(() => {
    const engine = new BotEngine(
      RAYON,
      "idle",
      bloubShapeRadii(shape),
      bloubExpressionOf(restExpr),
    );
    engineRef.current = engine;
    const play = resolveBloubPlay(verbToMarkState(verb), restExpr);
    engine.setState(play.state, 0);
    engine.setExpression(bloubExpressionOf(play.expression), 0);
    return engine.sample(paused ? (POSES[play.state] ?? 1) : 0);
  });

  useEffect(() => {
    const reduce =
      typeof window !== "undefined" &&
      window.matchMedia("(prefers-reduced-motion: reduce)").matches;
    const engine =
      engineRef.current ??
      new BotEngine(
        RAYON,
        "idle",
        bloubShapeRadii(shapeRef.current),
        bloubExpressionOf(restRef.current),
      );
    engineRef.current = engine;

    const look = { dx: 0, dy: 0, localR: 48, at: 0, fromScreen: false };
    let unlistenCursor: (() => void) | undefined;
    let aiming = false;

    const onPointerMove = (e: PointerEvent) => {
      if (e.pointerType === "touch") return;
      if (look.fromScreen && performance.now() - look.at < 180) return;
      look.dx = e.clientX;
      look.dy = e.clientY;
      look.localR = 0;
      look.fromScreen = false;
      look.at = performance.now();
    };
    const onPointerLeave = () => {
      if (!look.fromScreen) look.at = 0;
    };

    if (!pausedRef.current && !reduce) {
      window.addEventListener("pointermove", onPointerMove, { passive: true });
      window.addEventListener("pointerleave", onPointerLeave);
      void listen<{ dx?: number; dy?: number; localR?: number }>(
        "pet://cursor",
        (p) => {
          if (p == null || typeof p.dx !== "number" || typeof p.dy !== "number") {
            return;
          }
          look.dx = p.dx;
          look.dy = p.dy;
          look.localR =
            typeof p.localR === "number" && p.localR > 0 ? p.localR : 64;
          look.fromScreen = true;
          look.at = performance.now();
        },
      ).then((u) => {
        unlistenCursor = u;
      });
    }

    let raf = 0;
    let last = 0;
    let hoverSince = 0;
    let emoteMood = "";
    let emoteUntil = 0;
    let idleBurstMood = "";
    let idleBurstUntil = 0;
    let idleBurstNextAt = 0;
    let lastPlayState = engine.state;
    let stateSince = clockRef.current;
    let spin: PetSpinRun | null = null;
    let lastSpinKind: PetSpinKind | null = null;
    const orbit = createMarkOrbit({
      back: orbitBackRef.current,
      front: orbitFrontRef.current,
      idPrefix: uid,
      reduceMotion: reduce,
      radius: () => MARK_CENTER,
    });

    const resolvePlay = (nowMs: number) => {
      const session = verbToMarkState(verbRef.current);
      if (wantEmoteRef.current !== playedEmoteRef.current) {
        playedEmoteRef.current = wantEmoteRef.current;
        emoteMood = pickRestEmote(emoteMood);
        emoteUntil = nowMs + 2600;
      }
      if (restOnlyRef.current) {
        return resolveBloubPlay(session, restRef.current);
      }
      const lookFresh = look.at > 0 && nowMs - look.at < 280;
      const nearMark = look.fromScreen
        ? Math.hypot(look.dx, look.dy) <= (look.localR || 64) * 1.35
        : lookFresh;
      const trackingLook =
        lookFresh &&
        nearMark &&
        session !== "sleeping" &&
        session !== "dragging" &&
        !draggingRef.current;
      if (trackingLook && session === "idle") {
        if (!hoverSince) hoverSince = nowMs;
      } else {
        hoverSince = 0;
      }
      if (session === "idle" && !draggingRef.current && emoteUntil <= nowMs) {
        if (!idleBurstNextAt) {
          idleBurstNextAt = nowMs + 8000 + Math.random() * 8000;
        } else if (nowMs >= idleBurstNextAt) {
          idleBurstMood = pickRestEmote(idleBurstMood);
          idleBurstUntil = nowMs + 2200 + Math.random() * 1800;
          idleBurstNextAt = idleBurstUntil + 8000 + Math.random() * 8000;
        }
      }
      const mood = resolveLivingMood({
        sessionVerb: session,
        now: nowMs,
        dragging: draggingRef.current,
        hovering: hoverSince > 0,
        hoverMs: hoverSince > 0 ? nowMs - hoverSince : 0,
        emoteMood,
        emoteUntil,
        idleBurstMood,
        idleBurstUntil,
      });
      return resolveBloubPlay(mood, restRef.current);
    };

    const paint = (clock: number, dt: number) => {
      const nowMs = performance.now();
      if (wantSpinRef.current !== playedSpinRef.current) {
        playedSpinRef.current = wantSpinRef.current;
        if (!pausedRef.current && !reduce) {
          const kind = pickPetSpinKind(lastSpinKind);
          lastSpinKind = kind;
          spin = beginPetSpin(kind, nowMs);
          if (petSpinWantsBurst(kind)) orbit.burst(16, 0.95, 0.3);
        }
      }
      const play = resolvePlay(nowMs);
      engine.setShape(bloubShapeRadii(shapeRef.current), clock);
      engine.setExpression(bloubExpressionOf(play.expression), clock);
      if (play.state !== lastPlayState) {
        engine.setState(play.state, clock);
        lastPlayState = play.state;
        stateSince = clock;
      } else if (
        bloubShouldLoop(play.state) &&
        clock - stateSince >= bloubStateDuration(play.state)
      ) {
        engine.reset(play.state, clock);
        stateSince = clock;
      }
      const frozen = pausedRef.current || reduce;
      const t = frozen ? (POSES[play.state] ?? 1) : clock;
      const baseFace =
        play.state === "idle" || play.state === "swirl";
      const fresh = look.at > 0 && nowMs - look.at < 280;
      if (!baseFace || !fresh || restOnlyRef.current) {
        if (aiming) {
          engine.setLook(null, clock);
          aiming = false;
        }
      } else {
        const box = svgRef.current?.getBoundingClientRect();
        let nx = 0;
        let ny = 0;
        if (look.fromScreen) {
          const r = look.localR || 64;
          nx = look.dx / Math.max(1, r);
          ny = look.dy / Math.max(1, r);
        } else if (box && box.width > 0 && box.height > 0) {
          nx = (look.dx - (box.left + box.width / 2)) / Math.max(1, box.width);
          ny = (look.dy - (box.top + box.height / 2)) / Math.max(1, box.height);
        }
        if (Number.isFinite(nx) && Number.isFinite(ny)) {
          engine.setLook(bloubLookAtPointer(nx, ny, true), clock);
          aiming = true;
        }
      }
      setFrame(engine.sample(t));

      let spinAngle = 0;
      let extraRot = 0;
      let wobbleTurn = 0;
      let wobbleTilt = 0;
      let wobbleBob = 0;
      let bounceY = 0;
      let wideStyle = false;
      if (spin) {
        const sw = stepPetSpin(spin, nowMs, dt);
        if (sw.done) {
          spin = null;
        } else {
          spinAngle = sw.spinAngle;
          extraRot = sw.bodyRotDeg;
          wobbleTurn = sw.wobbleTurn;
          wobbleTilt = sw.wobbleTilt;
          wobbleBob = sw.wobbleBob;
          bounceY = sw.bounceY;
          wideStyle = sw.wideStyle;
        }
      }
      if (bodySpinRef.current) {
        const rot = extraRot + wobbleTurn;
        const tx = wobbleTilt;
        const ty = wobbleBob + bounceY;
        bodySpinRef.current.setAttribute(
          "transform",
          `translate(${tx.toFixed(2)} ${ty.toFixed(2)}) rotate(${rot.toFixed(2)})`,
        );
      }
      const markPx = svgRef.current?.getBoundingClientRect().width ?? 128;
      const sizeScale = clamp((340 / Math.max(markPx, 1)) ** 0.7, 1, 2.6);
      orbit.update(nowMs, dt, {
        spinAngle,
        sizeScale,
        wideStyle,
        sustainBelts: false,
      });
    };

    if (pausedRef.current || reduce) {
      paint(clockRef.current, 0);
      return () => {
        orbit.clear();
        window.removeEventListener("pointermove", onPointerMove);
        window.removeEventListener("pointerleave", onPointerLeave);
        unlistenCursor?.();
      };
    }

    const tick = (ms: number) => {
      const dt = last ? Math.min((ms - last) / 1000, 0.064) : 0;
      last = ms;
      clockRef.current += dt;
      paint(clockRef.current, dt);
      raf = requestAnimationFrame(tick);
    };
    raf = requestAnimationFrame(tick);
    return () => {
      cancelAnimationFrame(raf);
      orbit.clear();
      window.removeEventListener("pointermove", onPointerMove);
      window.removeEventListener("pointerleave", onPointerLeave);
      unlistenCursor?.();
    };
  }, [paused, uid]);

  useEffect(() => {
    const engine = engineRef.current;
    if (!engine) return;
    const play = resolveBloubPlay(verbToMarkState(verb), restExpr);
    engine.setShape(bloubShapeRadii(shape), clockRef.current);
    engine.setExpression(bloubExpressionOf(play.expression), clockRef.current);
    if (paused) {
      engine.reset(play.state, 0);
      setFrame(engine.sample(POSES[play.state] ?? 1));
    }
  }, [paused, shape, verb, restExpr]);

  const paper = eyeInk;
  const ink = fill;
  const notifFill = bloubNotifFill(ink);
  const vb = DEMI_VIEWBOX;

  return (
    <svg
      ref={svgRef}
      className="pet-mark"
      width={sizePx}
      height={sizePx}
      viewBox={`${-vb} ${-vb} ${vb * 2} ${vb * 2}`}
      role="img"
      aria-label={title}
      data-state={verbToMarkState(verb)}
      xmlns="http://www.w3.org/2000/svg"
      style={{
        display: "block",
        overflow: "visible",
        userSelect: "none",
        ["--pet-ink" as string]: fill,
      }}
    >
      <defs>
        <mask
          id={maskId}
          maskUnits="userSpaceOnUse"
          x={-vb}
          y={-vb}
          width={vb * 2}
          height={vb * 2}
        >
          <path d={frame.bodyPath} fill="#fff" />
          {frame.eyes.map((eye, i) => (
            <path
              key={i}
              d={eye.d}
              transform={eye.matrix}
              opacity={eye.alpha}
              fill="#000"
            />
          ))}
          {frame.notch ? (
            <circle
              cx={frame.notch.x}
              cy={frame.notch.y}
              r={frame.notch.r}
              fill="#000"
            />
          ) : null}
        </mask>
        {frame.arcs.map((arc) => (
          <linearGradient
            key={arc.id}
            id={`${uid}-${arc.id}`}
            gradientUnits="userSpaceOnUse"
            x1={arc.grad.x1}
            y1={arc.grad.y1}
            x2={arc.grad.x2}
            y2={arc.grad.y2}
          >
            {arc.grad.stops.map((c, i) => (
              <stop
                key={i}
                offset={
                  arc.grad.stops.length > 1
                    ? i / (arc.grad.stops.length - 1)
                    : 0
                }
                stopColor={c}
              />
            ))}
          </linearGradient>
        ))}
      </defs>
      <g
        ref={orbitBackRef}
        aria-hidden="true"
        transform={`translate(${-MARK_CENTER} ${-MARK_CENTER})`}
      />
      <g ref={bodySpinRef}>
        <g fill="none" strokeLinecap="round">
          {frame.arcs.map((arc) => (
            <path
              key={`b${arc.id}`}
              d={arc.back}
              stroke={`url(#${uid}-${arc.id})`}
              strokeWidth={arc.width}
              opacity={arc.opacity}
            />
          ))}
        </g>
        {frame.dotsBehind ? <g>{sampleDots(frame, ink, paper, "pb")}</g> : null}
        <g opacity={frame.bodyAlpha}>
          <path d={frame.bodyPath} fill={paper} />
          <g mask={`url(#${maskId})`}>
            <rect x={-vb} y={-vb} width={vb * 2} height={vb * 2} fill={ink} />
          </g>
        </g>
        {!frame.dotsBehind ? <g>{sampleDots(frame, ink, paper, "pf")}</g> : null}
        {frame.notif ? (
          <circle
            cx={frame.notif.x}
            cy={frame.notif.y}
            r={frame.notif.r}
            fill={notifFill}
          />
        ) : null}
        <g fill="none" strokeLinecap="round">
          {frame.arcs.map((arc) => (
            <path
              key={`f${arc.id}`}
              d={arc.front}
              stroke={`url(#${uid}-${arc.id})`}
              strokeWidth={arc.width}
              opacity={arc.opacity}
            />
          ))}
        </g>
      </g>
      <g
        ref={orbitFrontRef}
        aria-hidden="true"
        transform={`translate(${-MARK_CENTER} ${-MARK_CENTER})`}
      />
    </svg>
  );
}

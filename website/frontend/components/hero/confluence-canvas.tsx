"use client";

import { PerformanceMonitor } from "@react-three/drei";
import { Canvas, useFrame, useThree } from "@react-three/fiber";
import { useEffect, useRef, useState } from "react";
import { setConsoleFunction } from "three";

import { CAMERA } from "./confluence-geometry";
import { ConfluenceScene, type Palette, type Rgb } from "./confluence-scene";

// React Three Fiber 9.8 still creates a THREE.Clock, which three r183 and later report as
// deprecated on every page load. Drop that one notice; everything else three says still reaches
// the console.
setConsoleFunction((type, message, ...params) => {
  if (type === "warn" && message.startsWith("THREE.Clock: This module has been deprecated")) return;
  console[type](message, ...params);
});

// Constant objects, so React Three Fiber never re-applies the camera and undoes its aim.
const CAMERA_OPTIONS = {
  fov: CAMERA.fov,
  near: 0.1,
  far: 60,
  position: [CAMERA.position[0], CAMERA.position[1], CAMERA.position[2]] as [number, number, number],
};
const GL_OPTIONS = { antialias: true, alpha: false, stencil: false, powerPreference: "high-performance" as const };
const CANVAS_STYLE = { position: "absolute" as const, inset: 0, pointerEvents: "none" as const };

const FALLBACK = {
  riverbed: [0.02, 0.086, 0.106],
  bank: [0.094, 0.227, 0.259],
  current: [0.184, 0.769, 0.698],
  estuary: [0.298, 0.608, 0.82],
  foam: [0.894, 0.945, 0.941],
} as const satisfies Record<string, Rgb>;

function parseHex(value: string, fallback: Rgb): Rgb {
  const match = /^#([0-9a-f]{6})$/i.exec(value.trim());
  if (!match) return fallback;
  const n = Number.parseInt(match[1], 16);
  return [((n >> 16) & 255) / 255, ((n >> 8) & 255) / 255, (n & 255) / 255];
}

/** The theme's colours, read from the page's own tokens. */
function readPalette(): Palette {
  const css = getComputedStyle(document.documentElement);
  const token = (name: keyof typeof FALLBACK) => parseHex(css.getPropertyValue(`--${name}`), FALLBACK[name]);
  const riverbed = token("riverbed");
  const luminance = 0.2126 * riverbed[0] + 0.7152 * riverbed[1] + 0.0722 * riverbed[2];
  return {
    dark: luminance < 0.5,
    riverbed,
    bank: token("bank"),
    current: token("current"),
    estuary: token("estuary"),
    foam: token("foam"),
  };
}

const clamp = (n: number) => Math.min(1, Math.max(-1, n));

function Flow({ still }: { still: boolean }) {
  const [scene] = useState(() => new ConfluenceScene());
  const invalidate = useThree((state) => state.invalidate);
  const pointer = useRef({ x: 0, y: 0 });

  useEffect(() => () => scene.dispose(), [scene]);

  // Colours follow the theme: the site's toggle announces changes, and the system setting.
  useEffect(() => {
    const apply = () => {
      scene.setPalette(readPalette());
      invalidate();
    };
    apply();
    const query = window.matchMedia("(prefers-color-scheme: light)");
    query.addEventListener("change", apply);
    window.addEventListener("tributary-theme", apply);
    return () => {
      query.removeEventListener("change", apply);
      window.removeEventListener("tributary-theme", apply);
    };
  }, [scene, invalidate]);

  // A mouse or pen leans on the streams. Touch is left alone, so scrolling a phone stays calm.
  useEffect(() => {
    if (still) return;
    const move = (event: PointerEvent) => {
      if (event.pointerType === "touch") return;
      pointer.current = {
        x: clamp((event.clientX / window.innerWidth) * 2 - 1),
        y: clamp((event.clientY / window.innerHeight) * 2 - 1),
      };
    };
    const leave = () => {
      pointer.current = { x: 0, y: 0 };
    };
    window.addEventListener("pointermove", move, { passive: true });
    document.documentElement.addEventListener("pointerleave", leave);
    return () => {
      window.removeEventListener("pointermove", move);
      document.documentElement.removeEventListener("pointerleave", leave);
    };
  }, [still]);

  useFrame((state, delta) => {
    state.scene.background = scene.background;
    state.camera.lookAt(CAMERA.target[0], CAMERA.target[1], CAMERA.target[2]);
    // After a pause the first delta is long; never jump more than a frame or two.
    scene.frame(Math.min(delta, 1 / 20), pointer.current, state.size.height * state.viewport.dpr, still);
  });

  return <primitive object={scene.root} />;
}

type Props = {
  /** Run continuously; otherwise draw only when something changes. */
  animate: boolean;
  /** Reduced motion: one still moment, no swing, no pointer response. */
  still: boolean;
  onReady: () => void;
  onLost: () => void;
  className?: string;
};

/** The live confluence. Loaded on its own, after the page, and only where WebGL 2 works. */
export default function ConfluenceCanvas({ animate, still, onReady, onLost, className }: Props) {
  // Pixel density up to 1.75; a device that can't keep up drops to 1.
  const [dpr, setDpr] = useState<number | [number, number]>([1, 1.75]);
  return (
    <Canvas
      className={className}
      style={CANVAS_STYLE}
      dpr={dpr}
      flat
      frameloop={animate ? "always" : "demand"}
      gl={GL_OPTIONS}
      camera={CAMERA_OPTIONS}
      onCreated={({ gl }) => {
        gl.domElement.addEventListener(
          "webglcontextlost",
          (event) => {
            event.preventDefault();
            onLost();
          },
          { once: true },
        );
        // Two frames later the first picture is on screen.
        requestAnimationFrame(() => requestAnimationFrame(onReady));
      }}
    >
      <PerformanceMonitor onDecline={() => setDpr(1)} />
      <Flow still={still} />
    </Canvas>
  );
}

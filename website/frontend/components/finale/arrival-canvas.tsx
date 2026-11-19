"use client";

import { PerformanceMonitor } from "@react-three/drei";
import { Canvas, useFrame, useThree } from "@react-three/fiber";
import { useEffect, useState } from "react";
import { setConsoleFunction } from "three";

import { ArrivalScene, VIEW, type Palette, type Rgb } from "./arrival-scene";

// React Three Fiber 9.8 still creates a THREE.Clock, which three r183 and later report as
// deprecated on every page load. Drop that one notice; everything else three says still reaches
// the console.
setConsoleFunction((type, message, ...params) => {
  if (type === "warn" && message.startsWith("THREE.Clock: This module has been deprecated")) return;
  console[type](message, ...params);
});

// Constant objects, so React Three Fiber never re-applies them. Giving the frustum marks the
// camera as ours: it stays square to the room whatever the canvas size.
const CAMERA_OPTIONS = { ...VIEW, near: 1, far: 400, position: [0, 0, 200] as [number, number, number] };
const GL_OPTIONS = { antialias: true, alpha: true, stencil: false, powerPreference: "high-performance" as const };
const CANVAS_STYLE = { position: "absolute" as const, inset: 0, pointerEvents: "none" as const };

const FALLBACK = {
  riverbed: [0.02, 0.086, 0.106],
  channel: [0.043, 0.141, 0.169],
  current: [0.184, 0.769, 0.698],
  estuary: [0.298, 0.608, 0.82],
  foam: [0.894, 0.945, 0.941],
} as const satisfies Record<string, Rgb>;

/** A token's hex colour. The stylesheet may come minified, so #fff is as likely as #ffffff. */
function parseHex(value: string, fallback: Rgb): Rgb {
  const match = /^#([0-9a-f]{3}|[0-9a-f]{6})$/i.exec(value.trim());
  if (!match) return fallback;
  const hex = match[1].length === 3 ? [...match[1]].map((c) => c + c).join("") : match[1];
  const n = Number.parseInt(hex, 16);
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
    channel: token("channel"),
    current: token("current"),
    estuary: token("estuary"),
    foam: token("foam"),
  };
}

function Arrival({ arrived, still }: { arrived: boolean; still: boolean }) {
  const [scene] = useState(() => new ArrivalScene());
  const invalidate = useThree((state) => state.invalidate);

  useEffect(() => () => scene.dispose(), [scene]);

  useEffect(() => {
    if (!arrived) return;
    scene.start();
    invalidate();
  }, [arrived, scene, invalidate]);

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

  useFrame((state, delta) => {
    // After a pause the first delta is long; never jump more than a frame or two.
    scene.frame(Math.min(delta, 1 / 20), state.size.height * state.viewport.dpr, still);
  });

  return <primitive object={scene.root} />;
}

type Props = {
  /** Run continuously; otherwise draw only when something changes. */
  animate: boolean;
  /** The water has reached the download: the file starts to arrive. */
  arrived: boolean;
  /** Reduced motion: the file whole and at rest, nothing moving. */
  still: boolean;
  onReady: () => void;
  onLost: () => void;
  className?: string;
};

/** The live arrival. Loaded on its own, after the page, and only where WebGL 2 works. */
export default function ArrivalCanvas({ animate, arrived, still, onReady, onLost, className }: Props) {
  // Pixel density up to 1.75; a device that can't keep up drops to 1.
  const [dpr, setDpr] = useState<number | [number, number]>([1, 1.75]);
  return (
    <Canvas
      className={className}
      style={CANVAS_STYLE}
      orthographic
      dpr={dpr}
      flat
      frameloop={animate ? "always" : "demand"}
      gl={GL_OPTIONS}
      camera={CAMERA_OPTIONS}
      onCreated={({ gl }) => {
        gl.setClearColor(0x000000, 0);
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
      <Arrival arrived={arrived} still={still} />
    </Canvas>
  );
}

"use client";

import { PerformanceMonitor } from "@react-three/drei";
import { Canvas, useFrame, useThree } from "@react-three/fiber";
import { useEffect, useRef, useState } from "react";
import { setConsoleFunction, type PerspectiveCamera } from "three";

import { fitCamera, type CameraFit } from "./geometry";
import { MechanismScene, type Palette, type Rgb } from "./scene";
import type { SimFeed } from "./simulation";

// React Three Fiber 9.8 still creates a THREE.Clock, which three r183 and later report as
// deprecated on every page load (the hero filters the same notice). Everything else three says
// still reaches the console.
setConsoleFunction((type, message, ...params) => {
  if (type === "warn" && message.startsWith("THREE.Clock: This module has been deprecated")) return;
  console[type](message, ...params);
});

const GL_OPTIONS = { antialias: true, alpha: false, stencil: false, powerPreference: "high-performance" as const };
const CANVAS_STYLE = { position: "absolute" as const, inset: 0, pointerEvents: "none" as const };
const CAMERA_OPTIONS = { fov: 28, near: 0.5, far: 120, position: [3, 4, 14] as [number, number, number] };

const FALLBACK = {
  riverbed: [0.02, 0.086, 0.106],
  channel: [0.043, 0.141, 0.169],
  bank: [0.094, 0.227, 0.259],
  current: [0.184, 0.769, 0.698],
  estuary: [0.298, 0.608, 0.82],
  foam: [0.894, 0.945, 0.941],
  ok: [0.29, 0.871, 0.502],
  warn: [0.992, 0.69, 0.133],
  fault: [0.976, 0.439, 0.4],
} as const satisfies Record<string, Rgb>;

/** A token's hex colour; the CSS build may shorten one ("#ffffff" arrives as "#fff"). */
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
    bank: token("bank"),
    current: token("current"),
    estuary: token("estuary"),
    foam: token("foam"),
    ok: token("ok"),
    warn: token("warn"),
    fault: token("fault"),
  };
}

function Mechanism({ feed, animate, still }: { feed: SimFeed; animate: boolean; still: boolean }) {
  const [scene] = useState(() => new MechanismScene());
  const invalidate = useThree((state) => state.invalidate);
  const fit = useRef<{ aspect: number; camera: CameraFit } | null>(null);

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

  // Paused or still, the picture is drawn again only when the simulation moves.
  useEffect(() => {
    feed.listeners.add(invalidate);
    return () => {
      feed.listeners.delete(invalidate);
    };
  }, [feed, invalidate]);

  useFrame((state, delta) => {
    // The view is fitted to the box's shape: from the side on a phone, more from the front when wide.
    const aspect = state.size.width / Math.max(1, state.size.height);
    let current = fit.current;
    if (!current || Math.abs(current.aspect - aspect) > 0.001) {
      current = { aspect, camera: fitCamera(aspect) };
      fit.current = current;
      const { fov, position, target } = current.camera;
      const view = state.camera as PerspectiveCamera;
      view.fov = fov;
      view.position.set(position[0], position[1], position[2]);
      view.lookAt(target[0], target[1], target[2]);
      view.updateProjectionMatrix();
    }
    state.scene.background = scene.background;
    // After a pause the first delta is long; never jump more than a frame or two.
    scene.frame(
      Math.min(delta, 1 / 20),
      feed.current,
      animate && !still,
      still,
      state.size.height * state.viewport.dpr,
      current.camera.fov,
    );
  });

  return <primitive object={scene.root} />;
}

type Props = {
  feed: SimFeed;
  /** Run continuously; otherwise draw only when something changes. */
  animate: boolean;
  /** Reduced motion: each state as one still picture. */
  still: boolean;
  onReady: () => void;
  onLost: () => void;
  className?: string;
};

/** The live mechanism. Loaded on its own, near the section, and only where WebGL 2 works. */
export default function MechanismCanvas({ feed, animate, still, onReady, onLost, className }: Props) {
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
      <Mechanism feed={feed} animate={animate} still={still} />
    </Canvas>
  );
}

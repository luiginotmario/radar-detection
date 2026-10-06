import * as THREE from 'three';
import { colors, easeOut, toWorld } from './theme';
import type { Sensor } from './types';

const fragmentEnd = '#include <colorspace_fragment>';

/** the dotted floor everything sits on: a dot every half meter, fading out with distance. */
export function makeDots(center: THREE.Vector3) {
  const geometry = new THREE.PlaneGeometry(80, 80).rotateX(-Math.PI / 2);
  const material = new THREE.ShaderMaterial({
    transparent: true,
    depthWrite: false,
    uniforms: {
      uColor: { value: new THREE.Color(colors.dots) },
      uCenter: { value: new THREE.Vector2(center.x, center.z) },
    },
    vertexShader: /* glsl */ `
      varying vec3 vWorld;
      void main() {
        vWorld = (modelMatrix * vec4(position, 1.0)).xyz;
        gl_Position = projectionMatrix * viewMatrix * vec4(vWorld, 1.0);
      }`,
    fragmentShader: /* glsl */ `
      uniform vec3 uColor;
      uniform vec2 uCenter;
      varying vec3 vWorld;
      void main() {
        vec2 cell = vWorld.xz / 0.5;
        vec2 px = abs(fract(cell - 0.5) - 0.5) / fwidth(cell);
        float dotMask = 1.0 - smoothstep(0.7, 1.5, length(px));
        float fade = 1.0 - smoothstep(5.0, 18.0, distance(vWorld.xz, uCenter));
        gl_FragColor = vec4(uColor, dotMask * fade * 0.55);
        ${fragmentEnd}
      }`,
  });
  return new THREE.Mesh(geometry, material);
}

/**
 * one tracker's view cone on the floor: a faint fan with range rings, and a pulse that
 * rolls outward every couple of seconds.
 */
export class Cone {
  readonly group = new THREE.Group();
  readonly labels: { at: THREE.Vector3; text: string }[] = [];
  private readonly material: THREE.ShaderMaterial;
  private readonly half: number;
  private pulse = -1;

  constructor(readonly sensor: Sensor) {
    const { range, fov } = sensor;
    this.half = THREE.MathUtils.degToRad(fov / 2);
    this.group.position.copy(toWorld(sensor.x, sensor.y));
    this.group.rotation.y = -THREE.MathUtils.degToRad(sensor.heading);

    const fan = new THREE.CircleGeometry(range, 128, Math.PI / 2 - this.half, this.half * 2).rotateX(-Math.PI / 2);
    this.material = new THREE.ShaderMaterial({
      transparent: true,
      depthWrite: false,
      blending: THREE.AdditiveBlending,
      uniforms: {
        uColor: { value: new THREE.Color(colors.scan) },
        uRange: { value: range },
        uHalf: { value: this.half },
        uPulse: { value: -1 },
      },
      vertexShader: /* glsl */ `
        varying vec3 vLocal;
        void main() {
          vLocal = position;
          gl_Position = projectionMatrix * modelViewMatrix * vec4(position, 1.0);
        }`,
      fragmentShader: /* glsl */ `
        uniform vec3 uColor;
        uniform float uRange, uHalf, uPulse;
        varying vec3 vLocal;
        void main() {
          float r = length(vLocal.xz);
          // atan(0, 0) is undefined, right at the sensor
          float angle = r < 1e-4 ? 0.0 : abs(atan(vLocal.x, -vLocal.z));
          float fill = 0.01 + 0.018 * smoothstep(0.0, uRange, r);
          float edge = smoothstep(uHalf - 0.05, uHalf, angle) * 0.12;
          float behind = uPulse - r;
          // squares are written out: pow() of a negative number is NaN on many GPUs,
          // and NaN pixels turn black and get smeared across the screen by the glow
          float front = exp(-(behind * behind) / 0.0025);
          float wake = behind > 0.0 ? exp(-behind / 0.8) * 0.3 : 0.0;
          float pulse = uPulse < 0.0 ? 0.0 : (front + wake) * pow(max(1.0 - uPulse / uRange, 0.0), 0.6);
          gl_FragColor = vec4(uColor, fill + edge + pulse * 0.55);
          ${fragmentEnd}
        }`,
    });
    this.group.add(new THREE.Mesh(fan, this.material));

    const minor: number[] = [];
    const major: number[] = [];
    for (let r = 1; r <= range; r++) arc(r % 2 === 0 ? major : minor, r, this.half);
    for (const deg of [-30, 0, 30]) spoke(minor, THREE.MathUtils.degToRad(deg), range);
    spoke(major, -this.half, range);
    spoke(major, this.half, range);
    this.group.add(lines(minor, 0.1), lines(major, 0.28));

    // the sensor itself: a small chevron pointing the way it looks
    const chevron = new THREE.Shape([
      new THREE.Vector2(0, 0.22),
      new THREE.Vector2(0.16, -0.1),
      new THREE.Vector2(0, -0.02),
      new THREE.Vector2(-0.16, -0.1),
    ]);
    const marker = new THREE.Mesh(
      new THREE.ShapeGeometry(chevron).rotateX(-Math.PI / 2),
      new THREE.MeshBasicMaterial({ color: colors.scan, transparent: true, opacity: 0.9 }),
    );
    marker.position.y = 0.01;
    this.group.add(marker);

    for (let r = 2; r <= range; r += 2) {
      const at = new THREE.Vector3(Math.sin(this.half) * r, 0, -Math.cos(this.half) * r);
      this.labels.push({ at, text: `${r} m` });
    }
  }

  /** advances the pulse. the cycle repeats every `period` seconds. */
  tick(time: number, enabled: boolean) {
    const period = 2.8;
    const travel = 1.9;
    const t = (time % period) / travel;
    this.pulse = enabled && t <= 1 ? easeOut(t) * this.sensor.range : -1;
    this.material.uniforms.uPulse.value = this.pulse;
  }

  /** how strongly the pulse is lighting up a spot on the floor right now, 0 to 1. */
  flareAt(spot: THREE.Vector3) {
    if (this.pulse < 0) return 0;
    const local = this.group.worldToLocal(spot.clone());
    if (Math.abs(Math.atan2(local.x, -local.z)) > this.half) return 0;
    const r = Math.hypot(local.x, local.z);
    return Math.exp(-(((r - this.pulse) / 0.45) ** 2));
  }

  dispose() {
    this.group.traverse((o) => {
      if (o instanceof THREE.Mesh || o instanceof THREE.LineSegments) {
        o.geometry.dispose();
        (o.material as THREE.Material).dispose();
      }
    });
  }
}

function arc(out: number[], r: number, half: number) {
  const steps = Math.ceil(r * 24);
  for (let i = 0; i < steps; i++) {
    const a = -half + (2 * half * i) / steps;
    const b = -half + (2 * half * (i + 1)) / steps;
    out.push(Math.sin(a) * r, 0, -Math.cos(a) * r, Math.sin(b) * r, 0, -Math.cos(b) * r);
  }
}

function spoke(out: number[], angle: number, range: number) {
  out.push(Math.sin(angle) * 0.35, 0, -Math.cos(angle) * 0.35, Math.sin(angle) * range, 0, -Math.cos(angle) * range);
}

function lines(points: number[], opacity: number) {
  const geometry = new THREE.BufferGeometry();
  geometry.setAttribute('position', new THREE.Float32BufferAttribute(points, 3));
  const material = new THREE.LineBasicMaterial({
    color: colors.scan,
    transparent: true,
    opacity,
    depthWrite: false,
    blending: THREE.AdditiveBlending,
  });
  const segments = new THREE.LineSegments(geometry, material);
  segments.position.y = 0.005;
  return segments;
}

/**
 * someone the presence radar sees standing still, but the tracker can't place: a dashed
 * arc at their distance, since all we know is how far away they are.
 */
export class StillArc {
  readonly line: THREE.Line;
  private readonly material: THREE.LineDashedMaterial;
  private opacity = 0;
  private goal = 0;
  distance = 0;

  constructor(readonly sensor: Sensor) {
    const half = THREE.MathUtils.degToRad(sensor.fov / 2);
    const points: THREE.Vector3[] = [];
    for (let i = 0; i <= 96; i++) {
      const a = -half + (2 * half * i) / 96;
      points.push(new THREE.Vector3(Math.sin(a), 0, -Math.cos(a)));
    }
    const geometry = new THREE.BufferGeometry().setFromPoints(points);
    this.material = new THREE.LineDashedMaterial({
      color: colors.still,
      dashSize: 0.025,
      gapSize: 0.02,
      transparent: true,
      opacity: 0,
      depthWrite: false,
    });
    this.line = new THREE.Line(geometry, this.material);
    this.line.computeLineDistances();
    this.line.position.copy(toWorld(sensor.x, sensor.y, 0.01));
    this.line.rotation.y = -THREE.MathUtils.degToRad(sensor.heading);
  }

  show(distance: number | null) {
    this.goal = distance === null ? 0 : 1;
    if (distance !== null) this.distance = distance;
  }

  tick(dt: number, time: number) {
    this.opacity += (this.goal - this.opacity) * Math.min(1, dt * 5);
    const breathe = 0.75 + 0.25 * Math.sin((time * Math.PI * 2) / 4);
    this.material.opacity = this.opacity * breathe;
    this.line.visible = this.opacity > 0.01;
    const r = Math.max(this.distance, 0.1);
    this.line.scale.set(r, 1, r);
  }

  /** where to put its label: straight ahead of the sensor, at the arc. */
  get anchor() {
    return this.line.localToWorld(new THREE.Vector3(0, 0, -1));
  }

  get visibility() {
    return this.opacity;
  }
}

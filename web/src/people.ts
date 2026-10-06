import * as THREE from 'three';
import { colors, easeOut, follow, toWorld } from './theme';
import type { Person } from './types';

const HEIGHT = 1.7;
const TRAIL = 48;
const fragmentEnd = '#include <colorspace_fragment>';

const movingColor = new THREE.Color(colors.moving);
const stillColor = new THREE.Color(colors.still);

// shared by every person
const bodyGeometry = new THREE.CapsuleGeometry(0.22, HEIGHT - 0.44, 8, 24);
const glowGeometry = new THREE.CircleGeometry(0.9, 48).rotateX(-Math.PI / 2);
const ringGeometry = new THREE.RingGeometry(0.3, 0.335, 64).rotateX(-Math.PI / 2);
const coreGeometry = new THREE.CircleGeometry(0.075, 24).rotateX(-Math.PI / 2);

/** a see-through body: bright at the outline, faint in the middle, with slow scanlines. */
function hologram() {
  return new THREE.ShaderMaterial({
    transparent: true,
    depthWrite: false,
    blending: THREE.AdditiveBlending,
    uniforms: {
      uColor: { value: new THREE.Color() },
      uAlpha: { value: 0 },
      uFlare: { value: 0 },
      uTime: { value: 0 },
    },
    vertexShader: /* glsl */ `
      varying vec3 vNormal;
      varying vec3 vView;
      varying float vHeight;
      void main() {
        vec4 world = modelMatrix * vec4(position, 1.0);
        vNormal = normalize(mat3(modelMatrix) * normal);
        vView = normalize(cameraPosition - world.xyz);
        vHeight = world.y;
        gl_Position = projectionMatrix * viewMatrix * world;
      }`,
    fragmentShader: /* glsl */ `
      uniform vec3 uColor;
      uniform float uAlpha, uFlare, uTime;
      varying vec3 vNormal;
      varying vec3 vView;
      varying float vHeight;
      void main() {
        float rim = pow(1.0 - abs(dot(normalize(vNormal), normalize(vView))), 2.0);
        float scan = 0.8 + 0.2 * sin(vHeight * 42.0 - uTime * 5.0);
        float alpha = (rim * 0.85 + 0.06) * scan * uAlpha * (1.0 + uFlare * 1.2);
        gl_FragColor = vec4(uColor * (1.0 + uFlare), alpha);
        ${fragmentEnd}
      }`,
  });
}

/** a soft round glow on the floor. */
function floorGlow() {
  return new THREE.ShaderMaterial({
    transparent: true,
    depthWrite: false,
    blending: THREE.AdditiveBlending,
    uniforms: { uColor: { value: new THREE.Color() }, uAlpha: { value: 0 } },
    vertexShader: /* glsl */ `
      varying vec2 vLocal;
      void main() {
        vLocal = position.xz;
        gl_Position = projectionMatrix * modelViewMatrix * vec4(position, 1.0);
      }`,
    fragmentShader: /* glsl */ `
      uniform vec3 uColor;
      uniform float uAlpha;
      varying vec2 vLocal;
      void main() {
        float glow = pow(1.0 - clamp(length(vLocal) / 0.9, 0.0, 1.0), 2.4);
        gl_FragColor = vec4(uColor, glow * 0.55 * uAlpha);
        ${fragmentEnd}
      }`,
  });
}

/** the path someone walked, fading out behind them. */
function trailMaterial() {
  return new THREE.ShaderMaterial({
    transparent: true,
    depthWrite: false,
    blending: THREE.AdditiveBlending,
    uniforms: { uColor: { value: new THREE.Color() }, uAlpha: { value: 0 } },
    vertexShader: /* glsl */ `
      attribute float age;
      varying float vAge;
      void main() {
        vAge = age;
        gl_Position = projectionMatrix * modelViewMatrix * vec4(position, 1.0);
      }`,
    fragmentShader: /* glsl */ `
      uniform vec3 uColor;
      uniform float uAlpha;
      varying float vAge;
      void main() {
        gl_FragColor = vec4(uColor, pow(vAge, 1.6) * 0.7 * uAlpha);
        ${fragmentEnd}
      }`,
  });
}

class Blip {
  readonly root = new THREE.Group();
  readonly trail: THREE.Line;
  readonly position: THREE.Vector3;
  readonly color: THREE.Color;
  person: Person;
  /** 0 → 1 while showing up */
  appear = 0;
  /** 1 → 0 once they've gone; they fade out where they were last seen */
  linger = 1;
  gone = false;

  private readonly goal: THREE.Vector3;
  private readonly body = new THREE.Mesh(bodyGeometry, hologram());
  private readonly glow = new THREE.Mesh(glowGeometry, floorGlow());
  private readonly ring: THREE.Mesh<THREE.RingGeometry, THREE.MeshBasicMaterial>;
  private readonly core: THREE.Mesh<THREE.CircleGeometry, THREE.MeshBasicMaterial>;
  private readonly trailMaterial = trailMaterial();
  private readonly trailPoints = new Float32Array(TRAIL * 3);
  private trailCount = 0;
  private sinceTrail = 0;
  private stillness = 0;

  constructor(person: Person) {
    this.person = person;
    this.goal = toWorld(person.x, person.y);
    this.position = this.goal.clone();
    this.color = (person.state === 'still' ? stillColor : movingColor).clone();

    this.body.position.y = HEIGHT / 2;
    this.glow.position.y = 0.012;
    this.ring = new THREE.Mesh(ringGeometry, new THREE.MeshBasicMaterial({ transparent: true, depthWrite: false }));
    this.ring.position.y = 0.014;
    this.core = new THREE.Mesh(coreGeometry, new THREE.MeshBasicMaterial({ transparent: true, depthWrite: false }));
    this.core.position.y = 0.016;
    this.root.add(this.glow, this.ring, this.core, this.body);
    this.root.position.copy(this.position);

    const geometry = new THREE.BufferGeometry();
    geometry.setAttribute('position', new THREE.BufferAttribute(this.trailPoints, 3));
    const age = new Float32Array(TRAIL).map((_, i) => i / (TRAIL - 1));
    geometry.setAttribute('age', new THREE.BufferAttribute(age, 1));
    this.trail = new THREE.Line(geometry, this.trailMaterial);
    this.trail.frustumCulled = false;
    this.trail.position.y = 0.008;
  }

  update(person: Person) {
    this.person = person;
    this.goal.copy(toWorld(person.x, person.y));
  }

  tick(dt: number, time: number, flare: number) {
    if (this.gone) this.linger = Math.max(0, this.linger - dt / 1.2);
    else this.appear = Math.min(1, this.appear + dt / 0.35);

    if (!this.gone) this.position.lerp(this.goal, follow(dt, 7));
    this.root.position.copy(this.position);

    const still = this.person.state === 'still' ? 1 : 0;
    this.stillness += (still - this.stillness) * follow(dt, 4);
    this.color.lerp(still ? stillColor : movingColor, follow(dt, 5));

    const alpha = easeOut(this.appear) * this.linger;
    const scale = 0.9 + 0.1 * easeOut(this.appear);
    this.body.scale.setScalar(scale);

    // standing still, the ring slowly breathes
    const breathe = 1 + 0.09 * this.stillness * Math.sin((time * Math.PI * 2) / 4);
    this.ring.scale.setScalar(scale * breathe);

    const body = this.body.material as THREE.ShaderMaterial;
    body.uniforms.uColor.value.copy(this.color);
    body.uniforms.uAlpha.value = alpha * (this.gone ? 0.5 : 1);
    body.uniforms.uFlare.value = flare;
    body.uniforms.uTime.value = time;

    const glow = this.glow.material as THREE.ShaderMaterial;
    glow.uniforms.uColor.value.copy(this.color);
    glow.uniforms.uAlpha.value = alpha * (1 + flare);

    this.ring.material.color.copy(this.color);
    this.ring.material.opacity = alpha * 0.9;
    this.core.material.color.copy(this.color).multiplyScalar(1.6 + flare * 2);
    this.core.material.opacity = alpha;

    this.tickTrail(dt, alpha);
  }

  private tickTrail(dt: number, alpha: number) {
    this.sinceTrail += dt;
    const last = this.trailCount
      ? new THREE.Vector3().fromArray(this.trailPoints, (TRAIL - 1) * 3)
      : null;
    if (!this.gone && this.sinceTrail > 0.09 && (!last || last.distanceTo(this.position) > 0.04)) {
      this.sinceTrail = 0;
      // oldest point first, newest last
      this.trailPoints.copyWithin(0, 3);
      this.position.toArray(this.trailPoints, (TRAIL - 1) * 3);
      if (this.trailCount === 0) for (let i = 0; i < TRAIL - 1; i++) this.position.toArray(this.trailPoints, i * 3);
      this.trailCount = Math.min(TRAIL, this.trailCount + 1);
      this.trail.geometry.attributes.position.needsUpdate = true;
    }
    this.trailMaterial.uniforms.uColor.value.copy(this.color);
    this.trailMaterial.uniforms.uAlpha.value = alpha;
  }

  dispose() {
    for (const mesh of [this.body, this.glow, this.ring, this.core]) (mesh.material as THREE.Material).dispose();
    this.trail.geometry.dispose();
    this.trailMaterial.dispose();
  }
}

/** everyone on the map, kept in sync with blip's feed. */
export class People {
  readonly group = new THREE.Group();
  private readonly blips = new Map<number, Blip>();

  sync(people: Person[]) {
    const seen = new Set<number>();
    for (const person of people) {
      seen.add(person.id);
      const existing = this.blips.get(person.id);
      if (existing) {
        existing.gone = false;
        existing.linger = 1;
        existing.update(person);
        continue;
      }
      const blip = new Blip(person);
      this.blips.set(person.id, blip);
      this.group.add(blip.root, blip.trail);
    }
    for (const [id, blip] of this.blips) if (!seen.has(id)) blip.gone = true;
  }

  tick(dt: number, time: number, flareAt: (spot: THREE.Vector3) => number) {
    for (const [id, blip] of this.blips) {
      blip.tick(dt, time, flareAt(blip.position));
      if (blip.gone && blip.linger <= 0) {
        this.group.remove(blip.root, blip.trail);
        blip.dispose();
        this.blips.delete(id);
      }
    }
  }

  get all() {
    return [...this.blips.values()];
  }
}

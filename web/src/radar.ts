import * as THREE from 'three';
import { OrbitControls } from 'three/addons/controls/OrbitControls.js';
import { EffectComposer } from 'three/addons/postprocessing/EffectComposer.js';
import { OutputPass } from 'three/addons/postprocessing/OutputPass.js';
import { RenderPass } from 'three/addons/postprocessing/RenderPass.js';
import { UnrealBloomPass } from 'three/addons/postprocessing/UnrealBloomPass.js';
import { Cone, StillArc, makeDots } from './floor';
import type { Hud, Tag } from './hud';
import { People } from './people';
import { colors, easeInOut, toWorld } from './theme';
import type { RadarMap } from './types';

type View = 'tilt' | 'top';
type Options = { reduceMotion: boolean; lowFx: boolean };

/** the 3d radar: floor, view cones, people, and the camera looking at them. */
export class Radar {
  private readonly renderer: THREE.WebGLRenderer;
  private readonly scene = new THREE.Scene();
  private readonly camera = new THREE.PerspectiveCamera(40, 1, 0.1, 200);
  private readonly controls: OrbitControls;
  private readonly composer: EffectComposer | null;
  private readonly timer = new THREE.Timer();
  private readonly people = new People();
  private readonly floor = new THREE.Group();
  private cones: Cone[] = [];
  private arcs: StillArc[] = [];
  private layout = '';
  private map: RadarMap | null = null;
  private view: View = 'tilt';
  private flight: { from: THREE.Vector3; to: THREE.Vector3; fromTarget: THREE.Vector3; toTarget: THREE.Vector3; t: number } | null = null;

  constructor(
    host: HTMLElement,
    private readonly hud: Hud,
    private readonly options: Options,
  ) {
    this.renderer = new THREE.WebGLRenderer({ antialias: options.lowFx, powerPreference: 'high-performance' });
    // the glow is drawn several times per frame, so full retina resolution makes it
    // crawl on big screens. 1.5x still looks sharp.
    this.renderer.setPixelRatio(Math.min(devicePixelRatio, 1.5));
    host.append(this.renderer.domElement);

    this.scene.background = new THREE.Color(colors.background);
    this.scene.add(this.floor, this.people.group);

    this.controls = new OrbitControls(this.camera, this.renderer.domElement);
    this.controls.enableDamping = true;
    this.controls.enablePan = false;
    this.controls.maxPolarAngle = THREE.MathUtils.degToRad(78);
    this.controls.rotateSpeed = 0.6;

    if (options.lowFx) {
      this.composer = null;
    } else {
      const target = new THREE.WebGLRenderTarget(1, 1, { type: THREE.HalfFloatType, samples: 4 });
      this.composer = new EffectComposer(this.renderer, target);
      this.composer.addPass(new RenderPass(this.scene, this.camera));
      this.composer.addPass(new UnrealBloomPass(new THREE.Vector2(1, 1), 0.85, 0.5, 0.12));
      this.composer.addPass(new OutputPass());
    }

    addEventListener('resize', () => this.resize());
    this.resize();
    this.renderer.setAnimationLoop(() => this.frame());
  }

  update(map: RadarMap) {
    this.map = map;
    const layout = JSON.stringify(map.sensors.map(({ id, kind, x, y, heading, range, fov }) => [id, kind, x, y, heading, range, fov]));
    if (layout !== this.layout) {
      this.layout = layout;
      this.build(map);
    }
    this.people.sync(map.people);

    for (const arc of this.arcs) {
      const sensor = map.sensors.find((s) => s.id === arc.sensor.id);
      const still = sensor?.online ? (sensor.presence?.still ?? null) : null;
      // only draw the arc for someone the tracker hasn't already placed at that distance
      const placed =
        still !== null &&
        map.people.some((p) => Math.abs(Math.hypot(p.x - arc.sensor.x, p.y - arc.sensor.y) - still) < 0.75);
      arc.show(placed ? null : still);
    }
  }

  toggleView() {
    this.view = this.view === 'tilt' ? 'top' : 'tilt';
    this.fly();
    return this.view;
  }

  private build(map: RadarMap) {
    for (const cone of this.cones) cone.dispose();
    this.floor.clear();
    this.cones = map.sensors.filter((s) => s.kind === 'tracker').map((s) => new Cone(s));
    this.arcs = map.sensors.filter((s) => s.kind === 'presence').map((s) => new StillArc(s));
    this.floor.add(makeDots(this.area().center));
    for (const cone of this.cones) this.floor.add(cone.group);
    for (const arc of this.arcs) this.floor.add(arc.line);
    this.fly(true);
  }

  /** the outline of the floor all the trackers can see. */
  private area() {
    const outline: THREE.Vector3[] = [];
    for (const cone of this.cones) {
      const { x, y, heading, range, fov } = cone.sensor;
      outline.push(toWorld(x, y));
      for (let a = -fov / 2; a <= fov / 2; a += 5) {
        const angle = THREE.MathUtils.degToRad(a + heading);
        outline.push(toWorld(x + Math.sin(angle) * range, y + Math.cos(angle) * range));
      }
    }
    if (!outline.length) outline.push(toWorld(-4, 8), toWorld(4, 8), toWorld(0, 0));
    const center = new THREE.Box3().setFromPoints(outline).getCenter(new THREE.Vector3());
    return { center, outline };
  }

  /**
   * where the camera should sit to frame everything in the current view: as close as it
   * can get while the whole area (and people standing at its edge) still fits on screen.
   */
  private framing() {
    const { center, outline } = this.area();
    const top = this.view === 'top';
    // tall screens look from higher up, so the fan stands taller and fills more of the height
    const pitch = THREE.MathUtils.degToRad(top ? 89.9 : this.camera.aspect < 1 ? 64 : 50);
    const direction = new THREE.Vector3(0, Math.sin(pitch), Math.cos(pitch));
    const points = top ? outline : outline.flatMap((p) => [p, p.clone().setY(1.8)]);
    // leave room for the hud at the top and bottom
    const room = { x: 0.92, y: 0.8 };

    const probe = this.camera.clone();
    const target = center.clone();
    const place = (distance: number) => {
      probe.position.copy(target).addScaledVector(direction, distance);
      probe.lookAt(target);
      probe.updateMatrixWorld();
      const box = new THREE.Box2();
      for (const p of points) {
        const ndc = p.clone().project(probe);
        box.expandByPoint(new THREE.Vector2(ndc.x, ndc.y));
      }
      return box;
    };

    let distance = 30;
    for (let pass = 0; pass < 3; pass++) {
      let near = 1;
      let far = 200;
      for (let i = 0; i < 32; i++) {
        distance = (near + far) / 2;
        const box = place(distance);
        const fits = box.min.x > -room.x && box.max.x < room.x && box.min.y > -room.y && box.max.y < room.y;
        if (fits) far = distance;
        else near = distance;
      }
      distance = far;
      // nudge the target so the picture sits in the middle of the screen
      const mid = place(distance).getCenter(new THREE.Vector2());
      const halfHeight = distance * Math.tan(THREE.MathUtils.degToRad(this.camera.fov / 2));
      target.z -= (mid.y * halfHeight) / Math.sin(pitch);
      target.x += mid.x * halfHeight * this.camera.aspect;
    }
    return { position: target.clone().addScaledVector(direction, distance), target };
  }

  private fly(instant = false) {
    const { position, target } = this.framing();
    this.controls.minDistance = position.distanceTo(target) * 0.4;
    this.controls.maxDistance = position.distanceTo(target) * 1.8;
    if (instant || this.options.reduceMotion) {
      this.flight = null;
      this.camera.position.copy(position);
      this.controls.target.copy(target);
      this.controls.update();
      return;
    }
    this.flight = {
      from: this.camera.position.clone(),
      to: position,
      fromTarget: this.controls.target.clone(),
      toTarget: target,
      t: 0,
    };
  }

  private resize() {
    const { innerWidth: w, innerHeight: h } = window;
    // a hidden or collapsed window reports 0, and zero-sized buffers break rendering
    if (!w || !h) return;
    this.renderer.setSize(w, h);
    this.camera.aspect = w / h;
    // tall screens get a wider lens so the fan still fits
    this.camera.fov = w < h ? 50 : 40;
    this.camera.updateProjectionMatrix();
    this.composer?.setSize(w, h);
    if (this.map) this.fly(true);
  }

  private frame() {
    this.timer.update();
    const dt = Math.min(this.timer.getDelta(), 0.05);
    const time = this.timer.getElapsed();

    if (this.flight) {
      const f = this.flight;
      f.t = Math.min(1, f.t + dt / 0.7);
      const k = easeInOut(f.t);
      this.camera.position.lerpVectors(f.from, f.to, k);
      this.controls.target.lerpVectors(f.fromTarget, f.toTarget, k);
      if (f.t >= 1) this.flight = null;
    }
    this.controls.enabled = !this.flight;
    this.controls.update();

    for (const cone of this.cones) cone.tick(time, !this.options.reduceMotion);
    for (const arc of this.arcs) arc.tick(dt, time);
    this.people.tick(dt, time, (spot) => Math.max(0, ...this.cones.map((c) => c.flareAt(spot))));

    if (this.composer) this.composer.render(dt);
    else this.renderer.render(this.scene, this.camera);

    this.hud.placeTags(this.tags());
  }

  private tags(): Tag[] {
    const tags: Tag[] = [];
    const { innerWidth: w, innerHeight: h } = window;
    const project = (at: THREE.Vector3) => {
      const p = at.clone().project(this.camera);
      return { x: (p.x * 0.5 + 0.5) * w, y: (-p.y * 0.5 + 0.5) * h, visible: p.z < 1 };
    };
    const origin = this.cones[0]?.sensor ?? { x: 0, y: 0 };

    for (const marker of this.people.all) {
      const at = project(marker.position.clone().setY(this.view === 'top' ? 0 : 1.95));
      if (!at.visible) continue;
      const { id, x, y, speed, state } = marker.person;
      const distance = Math.hypot(x - origin.x, y - origin.y);
      const detail = marker.gone ? 'last seen' : state === 'still' ? 'still' : `${speed.toFixed(1)} m/s`;
      tags.push({
        key: `p${id}`,
        kind: 'person',
        ...at,
        color: `#${marker.color.getHexString()}`,
        opacity: marker.appear * marker.linger,
        html: `<b>${String(id).padStart(2, '0')}</b><span>${distance.toFixed(1)} m</span><span>${detail}</span>`,
      });
    }

    for (const cone of this.cones) {
      cone.labels.forEach((label, i) => {
        const at = project(cone.group.localToWorld(label.at.clone()));
        if (at.visible) tags.push({ key: `r${cone.sensor.id}-${i}`, kind: 'range', ...at, color: colors.scan, opacity: 1, html: label.text });
      });
    }

    for (const arc of this.arcs) {
      if (arc.visibility < 0.01) continue;
      const at = project(arc.anchor);
      if (at.visible) {
        tags.push({
          key: `s${arc.sensor.id}`,
          kind: 'arc',
          ...at,
          color: colors.still,
          opacity: arc.visibility,
          html: `someone still · ${arc.distance.toFixed(1)} m`,
        });
      }
    }
    return tags;
  }
}

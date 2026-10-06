import type { RadarMap, Sensor } from './types';

const $ = <T extends Element>(selector: string) => document.querySelector<T>(selector)!;

export type Tag = { key: string; x: number; y: number; html: string; color: string; opacity: number; kind: string };

/** everything drawn on top of the 3d scene: status, counts, sensors and floating tags. */
export class Hud {
  onToggleView = () => {};

  private readonly status = $<HTMLElement>('.status');
  private readonly count = $<HTMLElement>('.count .n');
  private readonly countLabel = $<HTMLElement>('.count .label');
  private readonly sensors = $<HTMLUListElement>('.sensors');
  private readonly view = $<HTMLButtonElement>('.view');
  private readonly signal = $<HTMLElement>('.signal');
  private readonly tagLayer = $<HTMLElement>('#tags');
  private readonly tags = new Map<string, HTMLElement>();
  private linkTimer = 0;
  private sensorsHtml = '';

  constructor() {
    this.view.addEventListener('click', () => this.onToggleView());
    this.setLink(false);
  }

  setLink(up: boolean) {
    clearTimeout(this.linkTimer);
    if (up) {
      this.signal.hidden = true;
      return;
    }
    this.status.dataset.state = 'off';
    this.status.querySelector('b')!.textContent = 'no signal';
    // give it a moment before calling it lost, so reconnects don't flash
    this.linkTimer = window.setTimeout(() => (this.signal.hidden = false), 1500);
  }

  setView(view: 'tilt' | 'top') {
    this.view.textContent = view === 'tilt' ? '2d' : '3d';
  }

  update(map: RadarMap) {
    this.status.dataset.state = map.mode;
    this.status.querySelector('b')!.textContent = map.mode === 'sim' ? 'simulated' : 'live';

    const n = map.people.length;
    this.count.textContent = String(n);
    this.countLabel.textContent = n === 1 ? 'person' : 'people';

    const html = map.sensors.map((s) => sensorLine(s, map)).join('');
    if (html !== this.sensorsHtml) this.sensors.innerHTML = this.sensorsHtml = html;
  }

  /** places the floating tags for this frame; tags not listed are removed. */
  placeTags(tags: Tag[]) {
    const keep = new Set<string>();
    for (const tag of tags) {
      keep.add(tag.key);
      let el = this.tags.get(tag.key);
      if (!el) {
        el = document.createElement('div');
        el.className = `tag ${tag.kind}`;
        this.tagLayer.append(el);
        this.tags.set(tag.key, el);
      }
      if (el.dataset.html !== tag.html) el.innerHTML = el.dataset.html = tag.html;
      el.style.transform = `translate3d(${tag.x.toFixed(1)}px, ${tag.y.toFixed(1)}px, 0)`;
      el.style.opacity = tag.opacity.toFixed(3);
      el.style.setProperty('--c', tag.color);
    }
    for (const [key, el] of this.tags) {
      if (keep.has(key)) continue;
      el.remove();
      this.tags.delete(key);
    }
  }
}

function sensorLine(sensor: Sensor, map: RadarMap) {
  const name = sensor.model;
  if (!sensor.online) return `<li data-state="off"><i></i>${name}<span>offline</span></li>`;
  if (sensor.kind === 'tracker') {
    const n = map.people.length;
    return `<li><i></i>${name}<span>${n ? `tracking ${n}` : 'watching'}</span></li>`;
  }
  const still = sensor.presence?.still;
  const moving = sensor.presence?.moving;
  const what =
    still != null ? `still · ${still.toFixed(1)} m` : moving != null ? `moving · ${moving.toFixed(1)} m` : 'nobody';
  return `<li><i></i>${name}<span>${what}</span></li>`;
}

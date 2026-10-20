import type { View } from './radar';
import type { RadarMap, Sensor } from './types';

const $ = <T extends Element>(selector: string) => document.querySelector<T>(selector)!;

export type Tag = { key: string; x: number; y: number; html: string; color: string; opacity: number; kind: string };

/** everything drawn on top of the 3d scene: status, counts, sensors and floating tags. */
export class Hud {
  onViewChange = (_view: View) => {};

  private readonly status = $<HTMLElement>('.status');
  private readonly count = $<HTMLElement>('.count .n');
  private readonly countLabel = $<HTMLElement>('.count .label');
  private readonly sensors = $<HTMLUListElement>('.sensors');
  private readonly views = $<HTMLElement>('.views');
  private readonly viewThumb = $<HTMLElement>('.views-thumb');
  private readonly viewButtons = [...document.querySelectorAll<HTMLButtonElement>('.views button')];
  private readonly signal = $<HTMLElement>('.signal');
  private readonly tagLayer = $<HTMLElement>('#tags');
  private readonly tags = new Map<string, HTMLElement>();
  private linkTimer = 0;
  private sensorsHtml = '';

  constructor() {
    for (const button of this.viewButtons) {
      button.addEventListener('click', () => this.onViewChange(button.dataset.view as View));
    }
    // arrow keys move between the options, like any radio group
    this.views.addEventListener('keydown', (event) => {
      const step = { ArrowRight: 1, ArrowDown: 1, ArrowLeft: -1, ArrowUp: -1 }[event.key];
      if (!step) return;
      event.preventDefault();
      const current = this.viewButtons.findIndex((b) => b.getAttribute('aria-checked') === 'true');
      const next = this.viewButtons[(current + step + this.viewButtons.length) % this.viewButtons.length];
      next.focus();
      this.onViewChange(next.dataset.view as View);
    });
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

  setView(view: View) {
    const index = this.viewButtons.findIndex((b) => b.dataset.view === view);
    this.viewButtons.forEach((button, i) => {
      button.setAttribute('aria-checked', String(i === index));
      button.tabIndex = i === index ? 0 : -1;
    });
    this.viewThumb.style.transform = `translateX(${index * 100}%)`;
  }

  update(map: RadarMap) {
    const anyOnline = map.sensors.some((s) => s.online);
    this.status.dataset.state = map.mode === 'live' && !anyOnline ? 'off' : map.mode;
    this.status.querySelector('b')!.textContent =
      map.mode === 'sim'
        ? 'simulated'
        : !map.sensors.length
          ? 'looking for sensors'
          : anyOnline
            ? 'live'
            : 'sensors offline';

    const n = map.people.length;
    this.count.textContent = String(n);
    this.countLabel.textContent = n === 1 ? 'person' : 'people';

    const html = map.sensors.length
      ? map.sensors.map((s) => sensorLine(s, map)).join('')
      : '<li data-state="off"><i></i>no sensors yet<span>plug them in</span></li>';
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

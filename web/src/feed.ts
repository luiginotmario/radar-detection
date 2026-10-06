import type { RadarMap } from './types';

/** listens to blip's live map, reconnecting whenever it drops. */
export function connect(onMap: (map: RadarMap) => void, onLink: (up: boolean) => void) {
  let attempt = 0;
  const open = () => {
    const ws = new WebSocket(`${location.protocol === 'https:' ? 'wss' : 'ws'}://${location.host}/ws`);
    ws.onopen = () => {
      attempt = 0;
      onLink(true);
    };
    ws.onmessage = (event) => onMap(JSON.parse(event.data));
    ws.onclose = () => {
      onLink(false);
      setTimeout(open, Math.min(4000, 500 * 2 ** attempt++));
    };
  };
  open();
}

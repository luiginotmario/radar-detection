import type { RadarMap } from './types';

/** listens to the radar's live map, reconnecting whenever it drops. */
export function connect(onMap: (map: RadarMap) => void, onLink: (up: boolean) => void) {
  let attempt = 0;
  let connectedBefore = false;
  const open = () => {
    const ws = new WebSocket(`${location.protocol === 'https:' ? 'wss' : 'ws'}://${location.host}/ws`);
    ws.onopen = () => {
      // the radar restarted, maybe with a newer screen (after an update), so load it
      if (connectedBefore) location.reload();
      connectedBefore = true;
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

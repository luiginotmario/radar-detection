import '@fontsource-variable/jetbrains-mono';
import './style.css';
import { connect } from './feed';
import { Hud } from './hud';
import { Radar } from './radar';

const reduceMotion = matchMedia('(prefers-reduced-motion: reduce)').matches;
// `?fx=low` skips the glow, for slower screens
const lowFx = new URLSearchParams(location.search).get('fx') === 'low';

const hud = new Hud();
const radar = new Radar(document.querySelector('#stage')!, hud, { reduceMotion, lowFx });
hud.onToggleView = () => hud.setView(radar.toggleView());

connect(
  (map) => {
    radar.update(map);
    hud.update(map);
  },
  (up) => hud.setLink(up),
);

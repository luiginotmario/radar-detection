import '@fontsource-variable/jetbrains-mono';
import './style.css';
import { connect } from './feed';
import { Hud } from './hud';
import { Radar } from './radar';

const reduceMotion = matchMedia('(prefers-reduced-motion: reduce)').matches;
// `?fx=low` skips the glow, for slower screens
const params = new URLSearchParams(location.search);
const lowFx = params.get('fx') === 'low';
// `?kiosk` is how the pi opens it: a touchscreen, so no mouse pointer
if (params.has('kiosk')) document.body.classList.add('kiosk');

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

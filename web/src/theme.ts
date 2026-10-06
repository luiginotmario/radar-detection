import { Vector3 } from 'three';

export const colors = {
  background: '#030608',
  scan: '#4dffc3',
  dots: '#2f8f74',
  moving: '#ff4a3d',
  still: '#ffb340',
};

/** map meters (x right, y ahead) to the 3d floor (y up, -z ahead). */
export const toWorld = (x: number, y: number, height = 0) => new Vector3(x, height, -y);

export const easeOut = (t: number) => 1 - Math.pow(1 - Math.min(Math.max(t, 0), 1), 3);
export const easeInOut = (t: number) => (t < 0.5 ? 4 * t * t * t : 1 - Math.pow(-2 * t + 2, 3) / 2);

/** frame-rate independent smoothing factor. */
export const follow = (dt: number, rate: number) => 1 - Math.exp(-dt * rate);

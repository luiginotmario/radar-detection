// what the radar sends ten times a second. distances in meters, map x to the right and y
// straight ahead of the first sensor.

export type Presence = { moving: number | null; still: number | null };

export type Sensor = {
  id: number;
  kind: 'tracker' | 'presence';
  model: string;
  x: number;
  y: number;
  heading: number;
  range: number;
  fov: number;
  online: boolean;
  presence?: Presence;
};

export type Person = {
  id: number;
  x: number;
  y: number;
  speed: number;
  state: 'moving' | 'still';
};

export type RadarMap = {
  mode: 'sim' | 'live';
  sensors: Sensor[];
  people: Person[];
};

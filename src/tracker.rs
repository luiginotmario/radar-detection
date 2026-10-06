//! Turns raw sensor readings into a list of people on the map.
//!
//! The LD2450 says where people are but can lose someone who stands perfectly still.
//! The LD2410C can't say where, only how far, but it keeps seeing still people. So when
//! a tracked person drops out of the LD2450 and the LD2410C still sees someone at about
//! the same distance, we keep them on the map as "still".

use std::time::{Duration, Instant};

use serde::Serialize;
use tokio::sync::{mpsc, watch};

/// Where a sensor sits on the map, in meters, and which way it faces (degrees clockwise
/// from straight ahead).
#[derive(Debug, Clone, Copy, Default, Serialize)]
pub struct Pose {
    pub x: f32,
    pub y: f32,
    pub heading: f32,
}

impl Pose {
    /// Sensor-local point (x right, y forward) to map coordinates.
    fn place(&self, local: Seen) -> (f32, f32) {
        let (sin, cos) = self.heading.to_radians().sin_cos();
        (
            self.x + local.x * cos + local.y * sin,
            self.y - local.x * sin + local.y * cos,
        )
    }

    fn distance_to(&self, x: f32, y: f32) -> f32 {
        (x - self.x).hypot(y - self.y)
    }
}

#[derive(Debug, Clone, Copy, Serialize, PartialEq)]
#[serde(rename_all = "lowercase")]
pub enum SensorKind {
    Tracker,
    Presence,
}

#[derive(Debug, Clone, Serialize)]
pub struct SensorInfo {
    pub id: usize,
    pub kind: SensorKind,
    pub model: &'static str,
    #[serde(flatten)]
    pub pose: Pose,
    /// how far it sees, meters
    pub range: f32,
    /// field of view, degrees
    pub fov: f32,
}

impl SensorInfo {
    pub fn ld2450(id: usize, pose: Pose) -> Self {
        Self { id, kind: SensorKind::Tracker, model: "ld2450", pose, range: 8.0, fov: 120.0 }
    }

    pub fn ld2410c(id: usize, pose: Pose) -> Self {
        Self { id, kind: SensorKind::Presence, model: "ld2410c", pose, range: 6.0, fov: 120.0 }
    }
}

/// A point the tracker radar sees, sensor-local meters.
#[derive(Debug, Clone, Copy)]
pub struct Seen {
    pub x: f32,
    pub y: f32,
}

/// What the presence radar sees: distance to the nearest moving and still person.
#[derive(Debug, Clone, Copy, Default, Serialize)]
pub struct Presence {
    pub moving: Option<f32>,
    pub still: Option<f32>,
}

pub enum Reading {
    Tracker { sensor: usize, targets: Vec<Seen> },
    Presence { sensor: usize, presence: Presence },
}

#[derive(Debug, Clone, Copy, Serialize, PartialEq)]
#[serde(rename_all = "lowercase")]
pub enum State {
    Moving,
    Still,
}

// how close a new point must be to an existing person to be the same person
const MATCH_RADIUS: f32 = 1.0;
// readings needed before a new person shows up, to skip one-off ghost points
const HITS_TO_SHOW: u32 = 2;
// how long someone stays on the map after the tracker loses them
const FORGET_AFTER: Duration = Duration::from_millis(1500);
// a lost person counts as held by the presence radar if their distance is this close
const HOLD_TOLERANCE: f32 = 0.75;
// slower than this (m/s) counts as standing still
const STILL_SPEED: f32 = 0.15;
const OFFLINE_AFTER: Duration = Duration::from_secs(2);

struct Track {
    id: u32,
    x: f32,
    y: f32,
    vx: f32,
    vy: f32,
    hits: u32,
    last_seen: Instant,
}

#[derive(Serialize)]
struct Person {
    id: u32,
    x: f32,
    y: f32,
    speed: f32,
    state: State,
}

#[derive(Serialize)]
struct SensorStatus<'a> {
    #[serde(flatten)]
    info: &'a SensorInfo,
    online: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    presence: Option<Presence>,
}

#[derive(Serialize)]
struct Snapshot<'a> {
    mode: &'static str,
    sensors: Vec<SensorStatus<'a>>,
    people: Vec<Person>,
}

pub struct Tracker {
    sensors: Vec<SensorInfo>,
    last_reading: Vec<Option<Instant>>,
    presence: Vec<Option<Presence>>,
    tracks: Vec<Track>,
    next_id: u32,
}

impl Tracker {
    pub fn new(sensors: Vec<SensorInfo>) -> Self {
        let n = sensors.len();
        Self { sensors, last_reading: vec![None; n], presence: vec![None; n], tracks: Vec::new(), next_id: 1 }
    }

    pub fn ingest(&mut self, reading: Reading, now: Instant) {
        match reading {
            Reading::Tracker { sensor, targets } => {
                self.last_reading[sensor] = Some(now);
                let pose = self.sensors[sensor].pose;
                let points: Vec<_> = targets.into_iter().map(|t| pose.place(t)).collect();
                self.associate(&points, now);
            }
            Reading::Presence { sensor, presence } => {
                self.last_reading[sensor] = Some(now);
                self.presence[sensor] = Some(presence);
            }
        }
    }

    fn associate(&mut self, points: &[(f32, f32)], now: Instant) {
        let mut claimed = vec![false; self.tracks.len()];
        for &(x, y) in points {
            let nearest = self
                .tracks
                .iter()
                .enumerate()
                .filter(|(i, _)| !claimed[*i])
                .map(|(i, t)| (i, (t.x - x).hypot(t.y - y)))
                .filter(|(_, d)| *d < MATCH_RADIUS)
                .min_by(|a, b| a.1.total_cmp(&b.1));

            match nearest {
                Some((i, _)) => {
                    claimed[i] = true;
                    let t = &mut self.tracks[i];
                    let dt = now.duration_since(t.last_seen).as_secs_f32().max(0.05);
                    let (nx, ny) = (t.x + (x - t.x) * 0.6, t.y + (y - t.y) * 0.6);
                    t.vx += ((nx - t.x) / dt - t.vx) * 0.3;
                    t.vy += ((ny - t.y) / dt - t.vy) * 0.3;
                    t.x = nx;
                    t.y = ny;
                    t.hits += 1;
                    t.last_seen = now;
                }
                None => {
                    self.tracks.push(Track { id: self.next_id, x, y, vx: 0.0, vy: 0.0, hits: 1, last_seen: now });
                    claimed.push(true);
                    self.next_id += 1;
                }
            }
        }
    }

    fn online(&self, sensor: usize, now: Instant) -> bool {
        self.last_reading[sensor].is_some_and(|t| now.duration_since(t) < OFFLINE_AFTER)
    }

    /// Is a presence radar seeing someone still at about this spot's distance?
    fn held(&self, x: f32, y: f32, now: Instant) -> bool {
        self.sensors.iter().filter(|s| s.kind == SensorKind::Presence).any(|s| {
            self.online(s.id, now)
                && self.presence[s.id]
                    .and_then(|p| p.still)
                    .is_some_and(|d| (s.pose.distance_to(x, y) - d).abs() < HOLD_TOLERANCE)
        })
    }

    /// Forgets people nobody can see anymore and returns the map as JSON.
    pub fn snapshot(&mut self, simulated: bool, now: Instant) -> String {
        let mut held: Vec<bool> = self.tracks.iter().map(|t| self.held(t.x, t.y, now)).collect();
        let keep: Vec<bool> = self
            .tracks
            .iter()
            .zip(&held)
            .map(|(t, h)| *h || now.duration_since(t.last_seen) < FORGET_AFTER)
            .collect();
        let mut flags = keep.iter();
        self.tracks.retain(|_| *flags.next().unwrap());
        let mut flags = keep.iter();
        held.retain(|_| *flags.next().unwrap());

        let people = self
            .tracks
            .iter()
            .zip(held)
            .filter(|(t, _)| t.hits >= HITS_TO_SHOW)
            .map(|(t, held)| {
                let lost = now.duration_since(t.last_seen) > Duration::from_millis(400);
                let speed = if lost { 0.0 } else { t.vx.hypot(t.vy) };
                let state = if (lost && held) || speed < STILL_SPEED { State::Still } else { State::Moving };
                Person { id: t.id, x: t.x, y: t.y, speed, state }
            })
            .collect();

        let sensors = self
            .sensors
            .iter()
            .map(|info| {
                let online = self.online(info.id, now);
                SensorStatus { info, online, presence: if online { self.presence[info.id] } else { None } }
            })
            .collect();

        let snapshot = Snapshot { mode: if simulated { "sim" } else { "live" }, sensors, people };
        serde_json::to_string(&snapshot).expect("snapshot serializes")
    }
}

/// Collects readings and publishes the map ten times a second.
pub async fn run(
    sensors: Vec<SensorInfo>,
    simulated: bool,
    mut readings: mpsc::Receiver<Reading>,
    map: watch::Sender<String>,
) {
    let mut tracker = Tracker::new(sensors);
    let mut tick = tokio::time::interval(Duration::from_millis(100));
    loop {
        tokio::select! {
            Some(reading) = readings.recv() => tracker.ingest(reading, Instant::now()),
            _ = tick.tick() => {
                map.send_replace(tracker.snapshot(simulated, Instant::now()));
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tracker() -> Tracker {
        Tracker::new(vec![SensorInfo::ld2450(0, Pose::default()), SensorInfo::ld2410c(1, Pose::default())])
    }

    fn people(json: &str) -> Vec<serde_json::Value> {
        let v: serde_json::Value = serde_json::from_str(json).unwrap();
        v["people"].as_array().unwrap().clone()
    }

    #[test]
    fn places_points_using_the_sensor_heading() {
        let facing_right = Pose { x: 1.0, y: 2.0, heading: 90.0 };
        let (x, y) = facing_right.place(Seen { x: 0.0, y: 3.0 });
        assert!((x - 4.0).abs() < 1e-5 && (y - 2.0).abs() < 1e-5);
    }

    #[test]
    fn needs_two_readings_before_someone_shows_up() {
        let mut t = tracker();
        let now = Instant::now();
        t.ingest(Reading::Tracker { sensor: 0, targets: vec![Seen { x: 0.5, y: 3.0 }] }, now);
        assert!(people(&t.snapshot(false, now)).is_empty());
        let later = now + Duration::from_millis(100);
        t.ingest(Reading::Tracker { sensor: 0, targets: vec![Seen { x: 0.55, y: 3.0 }] }, later);
        assert_eq!(people(&t.snapshot(false, later)).len(), 1);
    }

    #[test]
    fn presence_radar_keeps_a_still_person_on_the_map() {
        let mut t = tracker();
        let start = Instant::now();
        for i in 0..3 {
            let at = start + Duration::from_millis(100 * i);
            t.ingest(Reading::Tracker { sensor: 0, targets: vec![Seen { x: 0.0, y: 3.0 }] }, at);
        }
        let later = start + Duration::from_secs(5);
        t.ingest(Reading::Presence { sensor: 1, presence: Presence { moving: None, still: Some(3.1) } }, later);
        let seen = people(&t.snapshot(false, later));
        assert_eq!(seen.len(), 1);
        assert_eq!(seen[0]["state"], "still");

        // the presence radar sees nobody anymore, so they're gone
        let gone = later + Duration::from_millis(100);
        t.ingest(Reading::Presence { sensor: 1, presence: Presence::default() }, gone);
        assert!(people(&t.snapshot(false, gone)).is_empty());
    }
}

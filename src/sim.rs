//! Made-up people walking around, so the screen can be built without any hardware.
//!
//! The fake sensors behave like the real ones: the tracker sees at most 3 people, only
//! inside its cone, and tends to lose people who stand still; the presence radar keeps
//! seeing them.

use std::time::{Duration, SystemTime, UNIX_EPOCH};

use tokio::sync::mpsc;

use crate::tracker::{Pose, Presence, Reading, Seen, SensorInfo};

const TRACKER: usize = 0;
const PRESENCE: usize = 1;

pub fn start(tx: mpsc::Sender<Reading>) -> Vec<SensorInfo> {
    tokio::spawn(run(tx));
    vec![SensorInfo::ld2450(TRACKER, Pose::default()), SensorInfo::ld2410c(PRESENCE, Pose::default())]
}

struct Rng(u64);

impl Rng {
    fn next(&mut self) -> f32 {
        self.0 ^= self.0 << 13;
        self.0 ^= self.0 >> 7;
        self.0 ^= self.0 << 17;
        (self.0 >> 40) as f32 / (1u64 << 24) as f32
    }

    fn range(&mut self, lo: f32, hi: f32) -> f32 {
        lo + (hi - lo) * self.next()
    }

    fn chance(&mut self, p: f32) -> bool {
        self.next() < p
    }
}

struct Person {
    x: f32,
    y: f32,
    goal: (f32, f32),
    pace: f32,
    pause: f32,
    still_for: f32,
    sway: f32,
}

impl Person {
    fn new(rng: &mut Rng) -> Self {
        let (x, y) = spot(rng);
        Self { x, y, goal: spot(rng), pace: rng.range(0.6, 1.3), pause: 0.0, still_for: 0.0, sway: rng.range(0.0, 6.0) }
    }

    fn step(&mut self, dt: f32, t: f32, rng: &mut Rng) {
        if self.pause > 0.0 {
            self.pause -= dt;
            self.still_for += dt;
            return;
        }
        self.still_for = 0.0;
        let (dx, dy) = (self.goal.0 - self.x, self.goal.1 - self.y);
        let dist = dx.hypot(dy);
        if dist < 0.2 {
            if rng.chance(0.5) {
                self.pause = rng.range(2.0, 8.0);
            }
            // now and then walk out of view and come back later
            self.goal = if rng.chance(0.12) { (rng.range(-4.0, 4.0), 9.5) } else { spot(rng) };
            self.pace = rng.range(0.6, 1.3);
            return;
        }
        let (ux, uy) = (dx / dist, dy / dist);
        let wobble = (t * 1.7 + self.sway).sin() * 0.25;
        let step = (self.pace * dt).min(dist);
        self.x += (ux - uy * wobble) * step;
        self.y += (uy + ux * wobble) * step;
    }

    fn distance(&self) -> f32 {
        self.x.hypot(self.y)
    }

    fn in_view(&self, range: f32) -> bool {
        self.y > 0.3 && self.distance() < range && self.x.atan2(self.y).abs() < 60f32.to_radians()
    }
}

/// a random spot inside the tracker's cone
fn spot(rng: &mut Rng) -> (f32, f32) {
    let angle = rng.range(-50.0, 50.0).to_radians();
    let dist = rng.range(1.0, 7.0);
    (dist * angle.sin(), dist * angle.cos())
}

async fn run(tx: mpsc::Sender<Reading>) {
    let seed = SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_nanos() as u64).unwrap_or(1) | 1;
    let mut rng = Rng(seed);
    let mut people: Vec<Person> = (0..3).map(|_| Person::new(&mut rng)).collect();
    let mut tick = tokio::time::interval(Duration::from_millis(100));
    let mut t = 0.0;

    loop {
        tick.tick().await;
        t += 0.1;
        for p in &mut people {
            p.step(0.1, t, &mut rng);
        }

        let mut targets = Vec::new();
        for p in people.iter().filter(|p| p.in_view(8.0)) {
            if targets.len() < 3 && (p.still_for < 1.5 || rng.chance(0.1)) {
                targets.push(Seen { x: p.x + rng.range(-0.04, 0.04), y: p.y + rng.range(-0.04, 0.04) });
            }
        }

        let nearest = |still: bool| {
            people
                .iter()
                .filter(|p| p.in_view(6.0) && (p.still_for > 0.5) == still)
                .map(Person::distance)
                .min_by(f32::total_cmp)
        };
        let presence = Presence { moving: nearest(false), still: nearest(true) };

        if tx.send(Reading::Tracker { sensor: TRACKER, targets }).await.is_err()
            || tx.send(Reading::Presence { sensor: PRESENCE, presence }).await.is_err()
        {
            return;
        }
    }
}

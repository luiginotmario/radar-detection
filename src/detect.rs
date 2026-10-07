//! Finds the sensors by itself. On the Pi the adapters show up as ttyUSB0 and ttyUSB1 in
//! whatever order they woke up, so instead of trusting names we listen to every USB
//! serial port for a moment and tell the sensors apart by how their data starts.

use std::time::{Duration, Instant};

use tokio::io::AsyncReadExt;
use tokio::time::timeout;
use tokio_serial::{SerialPortBuilderExt, SerialPortType};

use crate::{ld2410, ld2450};

const LISTEN_FOR: Duration = Duration::from_millis(1500);

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Model {
    Ld2450,
    Ld2410,
}

#[derive(Debug, Clone)]
pub struct Found {
    pub path: String,
    pub model: Model,
}

/// USB serial ports and what they call themselves.
pub fn usb_ports() -> Vec<(String, String)> {
    let Ok(ports) = tokio_serial::available_ports() else { return Vec::new() };
    ports
        .into_iter()
        .filter_map(|p| match p.port_type {
            SerialPortType::UsbPort(info) => Some((p.port_name, info.product.unwrap_or_else(|| "usb serial".into()))),
            _ => None,
        })
        // on a mac every adapter shows up twice; the cu.* one is the one to use
        .filter(|(name, _)| !name.starts_with("/dev/tty."))
        .collect()
}

/// Every sensor plugged in right now, trackers first.
pub async fn scan() -> Vec<Found> {
    let listening: Vec<_> = usb_ports()
        .into_iter()
        .map(|(path, _)| tokio::spawn(async move { identify(&path).await.map(|model| Found { path, model }) }))
        .collect();
    let mut found = Vec::new();
    for task in listening {
        if let Ok(Some(sensor)) = task.await {
            found.push(sensor);
        }
    }
    found.sort_by(|a, b| (a.model, &a.path).cmp(&(b.model, &b.path)));
    found
}

async fn identify(path: &str) -> Option<Model> {
    // both sensors talk at the same speed, so one setting hears either
    let mut port = tokio_serial::new(path, ld2450::BAUD).open_native_async().ok()?;
    let mut listener = Listener::default();
    let mut buf = [0u8; 256];
    let until = Instant::now() + LISTEN_FOR;
    loop {
        let left = until.saturating_duration_since(Instant::now());
        let n = timeout(left, port.read(&mut buf)).await.ok()?.ok()?;
        if n == 0 {
            return None;
        }
        if let Some(model) = listener.feed(&buf[..n]) {
            return Some(model);
        }
    }
}

/// Runs bytes through both sensors' parsers until one recognises a whole frame.
#[derive(Default)]
struct Listener {
    tracker: ld2450::Parser,
    presence: ld2410::Parser,
}

impl Listener {
    fn feed(&mut self, bytes: &[u8]) -> Option<Model> {
        self.tracker.push(bytes);
        self.presence.push(bytes);
        if self.tracker.next_frame().is_some() {
            Some(Model::Ld2450)
        } else if self.presence.next_report().is_some() {
            Some(Model::Ld2410)
        } else {
            None
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const LD2450_FRAME: [u8; 30] = [
        0xAA, 0xFF, 0x03, 0x00, 0x0E, 0x03, 0xB1, 0x86, 0x10, 0x00, 0x40, 0x01, 0x00, 0x00,
        0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
        0x55, 0xCC,
    ];
    const LD2410_FRAME: [u8; 23] = [
        0xF4, 0xF3, 0xF2, 0xF1, 0x0D, 0x00, 0x02, 0xAA, 0x02, 0x00, 0x00, 0x00, 0xF5, 0x00,
        0x40, 0x2C, 0x01, 0x55, 0x00, 0xF8, 0xF7, 0xF6, 0xF5,
    ];

    #[test]
    fn tells_the_sensors_apart() {
        assert_eq!(Listener::default().feed(&LD2450_FRAME), Some(Model::Ld2450));
        assert_eq!(Listener::default().feed(&LD2410_FRAME), Some(Model::Ld2410));
    }

    #[test]
    fn waits_for_a_whole_frame() {
        let mut listener = Listener::default();
        assert_eq!(listener.feed(&[0x13, 0x37]), None);
        assert_eq!(listener.feed(&LD2410_FRAME[..10]), None);
        assert_eq!(listener.feed(&LD2410_FRAME[10..]), Some(Model::Ld2410));
    }
}

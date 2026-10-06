//! HLK-LD2410C: 24 GHz presence radar. It can't say where someone is, only how far, but
//! it keeps seeing people who stand completely still.
//!
//! Frames (256000 baud): `F4 F3 F2 F1` · length (u16) · data · `F8 F7 F6 F5`.
//! Data: type, `AA`, state (bit 0 moving, bit 1 still), moving distance (u16 cm),
//! moving energy, still distance (u16 cm), still energy, detection distance (u16 cm), `55 00`.

use std::time::Duration;

use anyhow::{Result, bail};
use tokio::io::AsyncReadExt;
use tokio::sync::mpsc;
use tokio::time::{sleep, timeout};
use tokio_serial::{SerialPortBuilderExt, SerialStream};

use crate::tracker::{Presence, Reading};

pub const BAUD: u32 = 256_000;

const HEADER: [u8; 4] = [0xF4, 0xF3, 0xF2, 0xF1];
const TAIL: [u8; 4] = [0xF8, 0xF7, 0xF6, 0xF5];
const MAX_DATA: usize = 128;

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Report {
    pub moving_cm: Option<u16>,
    pub still_cm: Option<u16>,
}

impl From<Report> for Presence {
    fn from(r: Report) -> Self {
        Presence {
            moving: r.moving_cm.map(|cm| cm as f32 / 100.0),
            still: r.still_cm.map(|cm| cm as f32 / 100.0),
        }
    }
}

#[derive(Default)]
pub struct Parser {
    buf: Vec<u8>,
}

impl Parser {
    pub fn push(&mut self, bytes: &[u8]) {
        self.buf.extend_from_slice(bytes);
        if self.buf.len() > 4096 {
            let excess = self.buf.len() - 4096;
            self.buf.drain(..excess);
        }
    }

    pub fn next_report(&mut self) -> Option<Report> {
        loop {
            let Some(start) = self.buf.windows(HEADER.len()).position(|w| w == HEADER) else {
                let keep = self.buf.len().min(HEADER.len() - 1);
                self.buf.drain(..self.buf.len() - keep);
                return None;
            };
            self.buf.drain(..start);
            if self.buf.len() < 6 {
                return None;
            }
            let len = u16::from_le_bytes([self.buf[4], self.buf[5]]) as usize;
            if len > MAX_DATA {
                self.buf.drain(..1);
                continue;
            }
            let total = 6 + len + TAIL.len();
            if self.buf.len() < total {
                return None;
            }
            if self.buf[total - TAIL.len()..total] != TAIL {
                self.buf.drain(..1);
                continue;
            }
            let report = decode(&self.buf[6..6 + len]);
            self.buf.drain(..total);
            if report.is_some() {
                return report;
            }
        }
    }
}

fn decode(data: &[u8]) -> Option<Report> {
    if data.len() < 13 || data[1] != 0xAA {
        return None;
    }
    let state = data[2];
    let word = |i: usize| u16::from_le_bytes([data[i], data[i + 1]]);
    Some(Report {
        moving_cm: (state & 0b01 != 0).then(|| word(3)),
        still_cm: (state & 0b10 != 0).then(|| word(6)),
    })
}

/// Reads one LD2410C forever, reconnecting if it's unplugged.
pub async fn run(sensor: usize, path: String, tx: mpsc::Sender<Reading>) {
    let mut warned = false;
    loop {
        match tokio_serial::new(&path, BAUD).open_native_async() {
            Ok(port) => {
                warned = false;
                eprintln!("ld2410c connected on {path}");
                if let Err(e) = read(sensor, port, &tx).await {
                    eprintln!("ld2410c on {path}: {e}");
                }
            }
            Err(e) if !warned => {
                eprintln!("can't open ld2410c on {path}: {e} (will keep trying)");
                warned = true;
            }
            Err(_) => {}
        }
        sleep(Duration::from_secs(2)).await;
    }
}

async fn read(sensor: usize, mut port: SerialStream, tx: &mpsc::Sender<Reading>) -> Result<()> {
    let mut parser = Parser::default();
    let mut buf = [0u8; 256];
    loop {
        let n = timeout(Duration::from_secs(3), port.read(&mut buf))
            .await
            .map_err(|_| anyhow::anyhow!("no data for 3s"))??;
        if n == 0 {
            bail!("port closed");
        }
        parser.push(&buf[..n]);
        while let Some(report) = parser.next_report() {
            tx.send(Reading::Presence { sensor, presence: report.into() }).await?;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn frame(state: u8, moving_cm: u16, still_cm: u16) -> Vec<u8> {
        let mut f = HEADER.to_vec();
        f.extend_from_slice(&13u16.to_le_bytes());
        f.extend_from_slice(&[0x02, 0xAA, state]);
        f.extend_from_slice(&moving_cm.to_le_bytes());
        f.push(60);
        f.extend_from_slice(&still_cm.to_le_bytes());
        f.push(40);
        f.extend_from_slice(&300u16.to_le_bytes());
        f.extend_from_slice(&[0x55, 0x00]);
        f.extend_from_slice(&TAIL);
        f
    }

    #[test]
    fn decodes_still_and_moving() {
        let mut p = Parser::default();
        p.push(&frame(0b11, 180, 245));
        assert_eq!(p.next_report(), Some(Report { moving_cm: Some(180), still_cm: Some(245) }));
    }

    #[test]
    fn ignores_distances_for_absent_states() {
        let mut p = Parser::default();
        p.push(&frame(0b10, 180, 245));
        p.push(&frame(0, 0, 0));
        assert_eq!(p.next_report(), Some(Report { moving_cm: None, still_cm: Some(245) }));
        assert_eq!(p.next_report(), Some(Report { moving_cm: None, still_cm: None }));
    }

    #[test]
    fn survives_noise_and_split_reads() {
        let f = frame(0b01, 90, 0);
        let mut p = Parser::default();
        p.push(&[0xF4, 0xF3, 0x00, 0xFD, 0xFC, 0xFB, 0xFA]);
        p.push(&f[..9]);
        assert_eq!(p.next_report(), None);
        p.push(&f[9..]);
        assert_eq!(p.next_report(), Some(Report { moving_cm: Some(90), still_cm: None }));
    }
}

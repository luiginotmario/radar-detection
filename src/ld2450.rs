//! HLK-LD2450: 24 GHz radar that tracks up to 3 moving people.
//!
//! It streams a 30-byte frame ten times a second over serial (256000 baud):
//! `AA FF 03 00` · 3 targets × 8 bytes · `55 CC`. Each target is x, y (mm), speed (cm/s)
//! and distance resolution (mm). x, y and speed use their own sign format: the top bit
//! set means positive, clear means negative, and the low 15 bits are the size.

use std::time::Duration;

use anyhow::{Result, bail};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::sync::mpsc;
use tokio::time::{sleep, timeout};
use tokio_serial::{SerialPortBuilderExt, SerialStream};

use crate::tracker::{Reading, Seen};

pub const BAUD: u32 = 256_000;

const HEADER: [u8; 4] = [0xAA, 0xFF, 0x03, 0x00];
const TAIL: [u8; 2] = [0x55, 0xCC];
const FRAME: usize = 30;

// config commands: open config mode, switch to multi-target tracking, close config mode.
const ENABLE_CONFIG: [u8; 14] = [
    0xFD, 0xFC, 0xFB, 0xFA, 0x04, 0x00, 0xFF, 0x00, 0x01, 0x00, 0x04, 0x03, 0x02, 0x01,
];
const MULTI_TARGET: [u8; 12] = [
    0xFD, 0xFC, 0xFB, 0xFA, 0x02, 0x00, 0x90, 0x00, 0x04, 0x03, 0x02, 0x01,
];
const END_CONFIG: [u8; 12] = [
    0xFD, 0xFC, 0xFB, 0xFA, 0x02, 0x00, 0xFE, 0x00, 0x04, 0x03, 0x02, 0x01,
];

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Target {
    pub x_mm: i16,
    pub y_mm: i16,
    // decoded for completeness; the tracker works out speed from movement instead,
    // since this one only counts motion toward or away from the sensor
    #[allow(dead_code)]
    pub speed_cms: i16,
    #[allow(dead_code)]
    pub resolution_mm: u16,
}

/// Turns a byte stream into frames, skipping noise and config replies.
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

    pub fn next_frame(&mut self) -> Option<Vec<Target>> {
        loop {
            let Some(start) = find(&self.buf, &HEADER) else {
                // keep a few bytes in case a header is split across reads
                let keep = self.buf.len().min(HEADER.len() - 1);
                self.buf.drain(..self.buf.len() - keep);
                return None;
            };
            self.buf.drain(..start);
            if self.buf.len() < FRAME {
                return None;
            }
            if self.buf[FRAME - 2..FRAME] != TAIL {
                self.buf.drain(..1);
                continue;
            }
            let targets = decode(&self.buf[HEADER.len()..FRAME - TAIL.len()]);
            self.buf.drain(..FRAME);
            return Some(targets);
        }
    }
}

fn find(haystack: &[u8], needle: &[u8]) -> Option<usize> {
    haystack.windows(needle.len()).position(|w| w == needle)
}

fn signed(raw: u16) -> i16 {
    let size = (raw & 0x7FFF) as i16;
    if raw & 0x8000 != 0 { size } else { -size }
}

fn decode(body: &[u8]) -> Vec<Target> {
    body.chunks_exact(8)
        .filter(|chunk| chunk.iter().any(|b| *b != 0))
        .map(|c| {
            let word = |i: usize| u16::from_le_bytes([c[i], c[i + 1]]);
            Target {
                x_mm: signed(word(0)),
                y_mm: signed(word(2)),
                speed_cms: signed(word(4)),
                resolution_mm: word(6),
            }
        })
        .collect()
}

/// Reads one LD2450 forever, reconnecting if it's unplugged.
pub async fn run(sensor: usize, path: String, tx: mpsc::Sender<Reading>) {
    let mut warned = false;
    loop {
        match tokio_serial::new(&path, BAUD).open_native_async() {
            Ok(port) => {
                warned = false;
                eprintln!("ld2450 connected on {path}");
                if let Err(e) = read(sensor, port, &tx).await {
                    eprintln!("ld2450 on {path}: {e}");
                }
            }
            Err(e) if !warned => {
                eprintln!("can't open ld2450 on {path}: {e} (will keep trying)");
                warned = true;
            }
            Err(_) => {}
        }
        sleep(Duration::from_secs(2)).await;
    }
}

async fn read(sensor: usize, mut port: SerialStream, tx: &mpsc::Sender<Reading>) -> Result<()> {
    for command in [&ENABLE_CONFIG[..], &MULTI_TARGET, &END_CONFIG] {
        port.write_all(command).await?;
        sleep(Duration::from_millis(100)).await;
    }

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
        while let Some(targets) = parser.next_frame() {
            let targets = targets
                .iter()
                .map(|t| Seen {
                    x: t.x_mm as f32 / 1000.0,
                    y: t.y_mm as f32 / 1000.0,
                })
                .collect();
            tx.send(Reading::Tracker { sensor, targets }).await?;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // the example frame from the LD2450 manual
    const EXAMPLE: [u8; 30] = [
        0xAA, 0xFF, 0x03, 0x00, 0x0E, 0x03, 0xB1, 0x86, 0x10, 0x00, 0x40, 0x01, 0x00, 0x00,
        0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
        0x55, 0xCC,
    ];

    #[test]
    fn decodes_manual_example() {
        let mut p = Parser::default();
        p.push(&EXAMPLE);
        let targets = p.next_frame().unwrap();
        assert_eq!(
            targets,
            vec![Target { x_mm: -782, y_mm: 1713, speed_cms: -16, resolution_mm: 320 }]
        );
        assert!(p.next_frame().is_none());
    }

    #[test]
    fn handles_noise_and_split_reads() {
        let mut p = Parser::default();
        p.push(&[0x01, 0xAA, 0x55, 0xFD, 0xFC]);
        p.push(&EXAMPLE[..11]);
        assert!(p.next_frame().is_none());
        p.push(&EXAMPLE[11..]);
        p.push(&EXAMPLE);
        assert_eq!(p.next_frame().unwrap().len(), 1);
        assert_eq!(p.next_frame().unwrap().len(), 1);
        assert!(p.next_frame().is_none());
    }

    #[test]
    fn skips_frames_with_a_bad_tail() {
        let mut broken = EXAMPLE;
        broken[29] = 0x00;
        let mut p = Parser::default();
        p.push(&broken);
        p.push(&EXAMPLE);
        assert_eq!(p.next_frame().unwrap()[0].x_mm, -782);
    }

    #[test]
    fn empty_frame_has_no_targets() {
        let mut empty = [0u8; 30];
        empty[..4].copy_from_slice(&HEADER);
        empty[28..].copy_from_slice(&TAIL);
        let mut p = Parser::default();
        p.push(&empty);
        assert!(p.next_frame().unwrap().is_empty());
    }
}

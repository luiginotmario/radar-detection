mod ld2410;
mod ld2450;
mod server;
mod sim;
mod tracker;

use std::net::{Ipv4Addr, SocketAddr};
use std::str::FromStr;

use anyhow::{Context, Result, bail};
use clap::{Parser, Subcommand};
use tokio::sync::{mpsc, watch};

use tracker::{Pose, SensorInfo};

/// a little radar that shows where people are, even behind a wall.
///
/// with no sensors given, blip makes up people walking around so you can try the screen.
#[derive(Parser)]
#[command(version, about)]
struct Cli {
    #[command(subcommand)]
    command: Option<Command>,

    /// an LD2450 (tracks where people are). repeat for more sensors
    #[arg(long, value_name = "PORT[@X,Y,DEG]")]
    ld2450: Vec<SensorArg>,

    /// an LD2410C (sees people standing still). repeat for more sensors
    #[arg(long, value_name = "PORT[@X,Y,DEG]")]
    ld2410: Vec<SensorArg>,

    /// port for the radar screen
    #[arg(long, default_value_t = 7070)]
    port: u16,

    /// let other devices on your network open the screen too
    #[arg(long)]
    lan: bool,
}

#[derive(Subcommand)]
enum Command {
    /// list serial ports, to find where the sensors are plugged in
    Ports,
}

/// A serial port, and optionally where the sensor sits: `@x,y,heading` in meters and
/// degrees clockwise. Leave it off when there's just one sensor.
#[derive(Clone)]
struct SensorArg {
    port: String,
    pose: Pose,
}

impl FromStr for SensorArg {
    type Err = anyhow::Error;

    fn from_str(s: &str) -> Result<Self> {
        let Some((port, at)) = s.split_once('@') else {
            return Ok(Self { port: s.to_string(), pose: Pose::default() });
        };
        let parts: Vec<f32> = at
            .split(',')
            .map(|n| n.trim().parse::<f32>())
            .collect::<Result<_, _>>()
            .context("placement should look like @x,y,heading, e.g. @0,0,0")?;
        let [x, y, heading] = parts[..] else {
            bail!("placement should look like @x,y,heading, e.g. @0,0,0");
        };
        Ok(Self { port: port.to_string(), pose: Pose { x, y, heading } })
    }
}

#[tokio::main]
async fn main() -> Result<()> {
    let cli = Cli::parse();
    if let Some(Command::Ports) = cli.command {
        return list_ports();
    }

    let (tx, rx) = mpsc::channel(64);
    let simulated = cli.ld2450.is_empty() && cli.ld2410.is_empty();
    let sensors = if simulated {
        eprintln!("no sensors given, so making up people (see `blip --help` to use real ones)");
        sim::start(tx)
    } else {
        let mut sensors = Vec::new();
        for arg in cli.ld2450 {
            let id = sensors.len();
            sensors.push(SensorInfo::ld2450(id, arg.pose));
            tokio::spawn(ld2450::run(id, arg.port, tx.clone()));
        }
        for arg in cli.ld2410 {
            let id = sensors.len();
            sensors.push(SensorInfo::ld2410c(id, arg.pose));
            tokio::spawn(ld2410::run(id, arg.port, tx.clone()));
        }
        sensors
    };

    let (map_tx, map_rx) = watch::channel(String::new());
    tokio::spawn(tracker::run(sensors, simulated, rx, map_tx));

    let host = if cli.lan { Ipv4Addr::UNSPECIFIED } else { Ipv4Addr::LOCALHOST };
    server::serve(SocketAddr::from((host, cli.port)), map_rx).await
}

fn list_ports() -> Result<()> {
    let ports = tokio_serial::available_ports()?;
    let usb: Vec<_> = ports
        .iter()
        .filter_map(|p| match &p.port_type {
            tokio_serial::SerialPortType::UsbPort(info) => Some((p, info)),
            _ => None,
        })
        // on a mac every adapter shows up twice; the cu.* one is the one to use
        .filter(|(p, _)| !p.port_name.starts_with("/dev/tty."))
        .collect();

    if usb.is_empty() {
        println!("no usb serial ports found. is the adapter plugged in?");
        return Ok(());
    }
    for (port, info) in usb {
        let name = info.product.as_deref().unwrap_or("usb serial");
        println!("{}  ({name})", port.port_name);
    }
    Ok(())
}

mod detect;
mod ld2410;
mod ld2450;
mod server;
mod sim;
mod tracker;

use std::net::{Ipv4Addr, SocketAddr};
use std::str::FromStr;
use std::time::{Duration, Instant};

use anyhow::{Context, Result, bail};
use clap::{Parser, Subcommand};
use tokio::sync::{mpsc, watch};

use detect::Model;
use tracker::{Pose, Reading, SensorInfo, Tracker};

/// a little radar that shows where people are, even behind a wall.
///
/// it finds the sensors by itself. no sensors? `--sim` makes up people walking around.
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

    /// make up people walking around instead of using real sensors
    #[arg(long)]
    sim: bool,

    /// port for the radar screen
    #[arg(long, default_value_t = 7070)]
    port: u16,

    /// let other devices on your network open the screen too
    #[arg(long)]
    lan: bool,
}

#[derive(Subcommand)]
enum Command {
    /// list usb serial ports and which sensor is on each
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
        list_ports().await;
        return Ok(());
    }

    let (tx, rx) = mpsc::channel(64);
    // until the sensors are found, the screen shows an empty radar that says so
    let (map_tx, map_rx) = watch::channel(Tracker::new(Vec::new()).snapshot(cli.sim, Instant::now()));

    if cli.sim {
        eprintln!("making up people walking around");
        tokio::spawn(tracker::run(sim::start(tx), true, rx, map_tx));
    } else {
        let given: Vec<_> = cli
            .ld2450
            .into_iter()
            .map(|arg| (Model::Ld2450, arg))
            .chain(cli.ld2410.into_iter().map(|arg| (Model::Ld2410, arg)))
            .collect();
        tokio::spawn(async move {
            let plan = if given.is_empty() {
                find_sensors()
                    .await
                    .into_iter()
                    .map(|f| (f.model, SensorArg { port: f.path, pose: Pose::default() }))
                    .collect()
            } else {
                given
            };
            let sensors = start_readers(plan, &tx);
            tracker::run(sensors, false, rx, map_tx).await;
        });
    }

    let host = if cli.lan { Ipv4Addr::UNSPECIFIED } else { Ipv4Addr::LOCALHOST };
    server::serve(SocketAddr::from((host, cli.port)), map_rx).await
}

fn start_readers(plan: Vec<(Model, SensorArg)>, tx: &mpsc::Sender<Reading>) -> Vec<SensorInfo> {
    plan.into_iter()
        .enumerate()
        .map(|(id, (model, arg))| match model {
            Model::Ld2450 => {
                tokio::spawn(ld2450::run(id, arg.port, tx.clone()));
                SensorInfo::ld2450(id, arg.pose)
            }
            Model::Ld2410 => {
                tokio::spawn(ld2410::run(id, arg.port, tx.clone()));
                SensorInfo::ld2410c(id, arg.pose)
            }
        })
        .collect()
}

/// Keeps looking until at least one sensor is plugged in.
async fn find_sensors() -> Vec<detect::Found> {
    let mut told = false;
    loop {
        let mut found = detect::scan().await;
        if !found.is_empty() {
            // the other adapter may still be waking up, so look once more
            tokio::time::sleep(Duration::from_secs(2)).await;
            for late in detect::scan().await {
                if !found.iter().any(|f| f.path == late.path) {
                    found.push(late);
                }
            }
            found.sort_by(|a, b| (a.model, &a.path).cmp(&(b.model, &b.path)));
            for f in &found {
                eprintln!("found {} on {}", label(f.model), f.path);
            }
            return found;
        }
        if !told {
            eprintln!("looking for sensors... plug them in (or run with --sim to try it without)");
            told = true;
        }
        tokio::time::sleep(Duration::from_secs(3)).await;
    }
}

fn label(model: Model) -> &'static str {
    match model {
        Model::Ld2450 => "ld2450",
        Model::Ld2410 => "ld2410c",
    }
}

async fn list_ports() {
    let ports = detect::usb_ports();
    if ports.is_empty() {
        println!("no usb serial ports found. is the adapter plugged in?");
        return;
    }
    let found = detect::scan().await;
    for (path, product) in ports {
        let sensor = found.iter().find(|f| f.path == path).map_or("no sensor data", |f| label(f.model));
        println!("{path}  ({product})  {sensor}");
    }
}

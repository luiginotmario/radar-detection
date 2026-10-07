# hi, this is radar-detection

a little radar that shows where people are, even through a wall. a tiny radar board sends out radio waves, people bounce them back (we're mostly water), and radar-detection turns the echoes into glowing people on a 3d map.

- free and open source (MIT)
- 100% local. it runs on your own device, no account, no cloud.
- sees up to ~8 m, through drywall, wood and glass (not concrete or metal)
- works without any hardware too: run it and it makes up people walking around

## how it works

the radar sends out a chirp, a signal that sweeps in frequency. how long the echo takes to come back says how far away someone is, and the tiny difference between its antennas says which direction. everything that never moves (walls, furniture) gets filtered out, so what's left is people. nobody is perfectly still, because we breathe, so they stay on the map.

two sensors work together:

- **ld2450** tracks up to 3 people: where they are and how fast they're moving
- **ld2410c** can't say where, only how far, but it keeps seeing people who stand completely still. when the ld2450 loses someone who stopped moving, it keeps them on the map in amber.

## what you need

| part | what it's for |
|---|---|
| raspberry pi 5 (+ power supply, sd card, fan) | runs the radar and the screen |
| raspberry pi touch display 2 (7") | the radar screen |
| hlk-ld2450 | tracks where people are |
| hlk-ld2410c | sees people standing still |
| 2× cp2102 usb-to-ttl adapter | plugs each sensor in over usb |
| female-to-female jumper wires | sensor → adapter |

about $400 for everything. you can start with just a sensor and an adapter plugged into your mac (with a usb-c to usb-a adapter if you need one).

wiring is the same for both sensors. tx and rx cross over:

| sensor | adapter |
|---|---|
| 5V | 5V |
| GND | GND |
| TX | RX |
| RX | TX |

## try it

- install [rust](https://rustup.rs), [node 22](https://nodejs.org) and [pnpm](https://pnpm.io)
- build the screen once, then run it with made-up people walking around:

```
cd web && pnpm install && pnpm build && cd ..
cargo run -- --sim
```

- open http://localhost:7070 — done.

with real sensors, plug them in (any usb port, any order) and leave off `--sim`. it listens to each port for a moment and works out which sensor is which by itself. `cargo run -- ports` shows what it found.

more than one tracker? say where each one sits: `--ld2450 /dev/ttyUSB2@4,0,-90` is 4 m to the right, facing left (meters, then degrees clockwise).

on the screen: tap **2d / 3d** to switch views, drag to look around, pinch or scroll to zoom.

## working on the screen

```
cargo run -- --sim
cd web && pnpm dev
```

run those in two terminals and open http://localhost:5173. changes show up instantly. add `?fx=low` to the address to turn off the glow on slower screens.

## on the raspberry pi

the pi runs the radar and shows it full screen on the touch display as soon as it turns on.

1. put the sd card in, connect the display, plug the sensors into any usb ports, and turn it on
2. go through the welcome screens (wifi, a user name and password)
3. turn on ssh: menu → preferences → control centre → interfaces → ssh
4. on your mac, from this folder (use the user name you picked, and the pi's name — `raspberrypi` unless you changed it):

```
./scripts/deploy.sh you@raspberrypi.local
```

5. reboot the pi — done. the radar opens full screen and finds the sensors by itself.

to update it later, run `deploy.sh` again. the screen on the pi reloads on its own.

- **watch it from your phone too:** `./scripts/deploy.sh you@raspberrypi.local --lan`, then open http://raspberrypi.local:7070
- **turn the display sideways:** menu → preferences → screen configuration → right-click the display → orientation. the radar fits whichever way it faces.
- **something off?** `ssh you@raspberrypi.local journalctl -u radar-detection -f` shows what it's doing
- tip: run `ssh-copy-id you@raspberrypi.local` once so it stops asking for your password

## how it's built

```
src/detect.rs    rust   finds the sensors and tells them apart
src/ld2450.rs    rust   reads the tracking radar
src/ld2410.rs    rust   reads the presence radar
src/tracker.rs   rust   turns both into people on a map
src/sim.rs       rust   made-up people, for trying it without hardware
src/server.rs    rust   serves the screen and streams the map to it, ten times a second
web/             ts     the radar screen, drawn with three.js
pi/setup.sh      bash   sets up the pi: starts at boot, opens full screen
scripts/         bash   build for the pi and deploy to it from your mac
```

the screen gets baked into the rust binary, so on the pi it's a single file.

## if you wanna make an addition + pr, or just wanna remix it for yourself, go for it

clone it, run `cargo test`, make your changes on a pr :)

## please be cool

point it at your own space. tracking people through someone else's wall isn't okay, and in a lot of places it's illegal.

## license

[MIT](LICENSE) — made by [@giginotmario](https://x.com/giginotmario)

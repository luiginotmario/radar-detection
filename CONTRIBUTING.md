# contributing

thanks for wanting to help. radar-detection stays useful by staying simple, so here's the deal:

**yes please**
- more sensors (other radars, a lidar for the walls)
- better tracking: fewer lost people, smoother paths
- making the screen faster on the pi
- bug fixes

**probably not**
- accounts, cloud, recording people — it runs on your own device and shows what's there right now, nothing more

## getting set up

```
cd web && pnpm install && pnpm build && cd ..
cargo run -- --sim       # made-up people, no hardware needed
cargo test
```

- each sensor has its own reader in `src/` (`ld2450.rs`, `ld2410.rs`); `tracker.rs` turns their readings into people on the map.
- the screen is `web/` (three.js); it gets baked into the binary, so rebuild it before `cargo build`.
- keep it light: it has to run smoothly on a raspberry pi.

open a pr with what you changed and how you tested it (with real sensors if you can). small prs get merged fastest.

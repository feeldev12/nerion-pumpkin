# nerion-vr (Pumpkin plugin)

Forwards Nerion VR players' head and hand poses (`nerion:vr`, format v1) to the players around
who registered the channel, on the Pumpkin Nerion ships for Android servers. Same checks and
distance thinning as NerionServerSDK's relay: at most 25 poses a second per player, a body's
reach, every pose within 16 blocks, one in two within 48, one in four up to 128.

Wasm plugin, Pumpkin plugin API v0.1, pinned to the Pumpkin commit of `pumpkin.properties`
(`9c04f71`). `src/proto.rs` is a copy of Nerion Nativo's `nerion-vr-proto`, checked by the shared
vectors in `tests/`.

```sh
cargo test                                   # the shared vectors
cargo build --release --target wasm32-wasip2 # target/wasm32-wasip2/release/nerion_vr_pumpkin.wasm
```

The server loads it from its `plugins/` folder.

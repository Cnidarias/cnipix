# cnipix

cnipix is a browser-based pixel animation editor built with Dioxus and Rust. It exports the animated-sprite JSON and horizontal PNG sprite sheet consumed by `../corrupted-scythe`.

## Run locally

Install the Dioxus CLI and the WebAssembly target, then start the development server:

```sh
rustup target add wasm32-unknown-unknown
cargo install dioxus-cli --locked
dx serve
```

Open the local address printed by `dx serve` in a browser. Dioxus uses WebAssembly for its web renderer; see the [Dioxus Web guide](https://dioxuslabs.com/learn/0.7/guides/platforms/web/) and [getting-started guide](https://dioxuslabs.com/learn/0.7/getting_started/) for setup details.

## Editing

- Draw with click or pointer-drag using the palette. Palette slot zero clears pixels and exports transparent alpha.
- Set the frame cell width and height independently from 1–64 pixels. Resizing keeps the top-left portion of existing artwork.
- Set frame timing from 10–2000 ms. **Preview animation** opens a stage that loops through the timeline at that speed.
- Onion skin is on by default and shows the previous frame beneath transparent pixels in the selected frame.
- Use the mouse wheel or zoom controls to enlarge the canvas. Drag the viewport scrollbars to pan around the enlarged region.
- Add, duplicate, select, and delete frames in the timeline.

## Game export

**Export game files** downloads `cnipix_animated_sprite.json` and `cnipix_spritesheet.png` from the browser. Copy both into `corrupted-scythe/assets/player/`. The JSON uses the game's expected `spritesheet`, `width`, `height`, and `anims` fields. `frametime` is expressed in seconds, and the numeric frame indices select cells from the exported horizontal strip.

The exported sprite path is `assets/player/cnipix_spritesheet.png`.

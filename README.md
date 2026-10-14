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

## Run with Docker

Build the production web app and serve it locally with Docker Compose:

```sh
docker compose up --build
```

Open `http://localhost:8080`. Set `CNIPIX_PORT` to change the host port, for example `CNIPIX_PORT=3000 docker compose up --build`. Stop the app with `docker compose down`.

The GitHub Actions workflow builds the image for pull requests and publishes it to GitHub Container Registry on pushes to the default branch and on version tags (`v*`). The image is `ghcr.io/cnidarias/cnipix`; the default branch publishes the `latest` tag. After the first publish, make the package public in its GitHub Packages settings if it is not already public.

## Publish with GitHub Pages

The **Deploy GitHub Pages** workflow builds on pushes to `main` or `master` and deploys from the repository's default branch. In the repository, open **Settings → Pages** and set **Build and deployment → Source** to **GitHub Actions**. Then push or merge to the default branch; the site will be available at `https://cnidarias.github.io/cnipix/`.

## Editing

- Draw with click or pointer-drag using the palette. Palette slot zero clears pixels and exports transparent alpha.
- Use Select to drag out a rectangular region, drag an existing selection to move it, then Copy and Paste to duplicate it elsewhere. Fill replaces the connected region under the cursor with the selected color.
- Use `Ctrl+C` / `Ctrl+V` (or `Cmd+C` / `Cmd+V` on macOS) to copy and paste a selection. Paste lands at the pointer when it is over the canvas; otherwise click the canvas to place it.
- Press `Delete` or `Backspace` to clear the selected region to transparency.
- Press `Escape` to deselect.
- Right-click or right-drag on the canvas to erase pixels. Press `0` for transparent, `1`–`9` for palette colors 1–9, and `Shift+1`–`Shift+6` for palette colors 10–15. Press `B` for Pencil, `M` for Select, and `G` for Fill. Keyboard shortcuts are ignored while editing text or numeric inputs.
- Set the frame cell width and height independently from 1–64 pixels. Resizing keeps the top-left portion of existing artwork.
- Set frame timing from 10–2000 ms. **Preview animation** opens a stage that loops through the timeline at that speed.
- Onion skin is on by default and shows the previous frame beneath transparent pixels in the selected frame.
- Use the mouse wheel or zoom controls to enlarge the canvas. Drag the viewport scrollbars to pan around the enlarged region.
- Add, duplicate, select, and delete frames in the timeline.

## Game export

**Export game files** downloads `cnipix_animated_sprite.json` and `cnipix_spritesheet.png` from the browser. Copy both into `corrupted-scythe/assets/player/`. The JSON uses the game's expected `spritesheet`, `width`, `height`, and `anims` fields. `frametime` is expressed in seconds, and the numeric frame indices select cells from the exported horizontal strip.

The exported sprite path is `assets/player/cnipix_spritesheet.png`.

## Load a project

Choose **Load project** and select the matching animation JSON and PNG sprite sheet together. The editor loads the first animation listed in the JSON. Opaque RGB colors used by the imported frames are added to the working palette and kept exactly; transparent pixels remain transparent. The editor supports up to 255 opaque colors per loaded project.

Choose **Project folder** to map a game folder or its `assets` directory. The project browser finds animation JSON files with a matching sprite sheet and lists every animation in each file. Choose an animation to open it, then use **Save to folder** to write edits back to its JSON and PNG. Saving preserves the other animations and sprite sheet cells in those files. Folder mapping and direct saving require a Chromium based browser with File System Access support. A mapped sprite's cell width and height must stay the same while saving; use **Export game files** for a resized copy.

## Palettes

Use **Import** or **Export** beside the palette to load or download a JSON palette. Palette JSON can be an array of hex colors or an object with a `colors` array. The transparent erase slot is kept separately; provide 1–15 opaque colors. Replacing a palette remaps existing artwork to the closest new color.

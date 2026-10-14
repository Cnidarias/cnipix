use dioxus::prelude::*;
use serde::Serialize;
use std::fmt::Write as _;

const INITIAL_WIDTH: usize = 24;
const INITIAL_HEIGHT: usize = 24;
const CELL_LIMIT: usize = 64;
const INITIAL_FRAME_TIME_MS: u32 = 120;
const SPRITESHEET_PATH: &str = "assets/player/cnipix_spritesheet.png";
const PALETTE: [&str; 16] = [
    "#171923", "#f7f3e8", "#ef476f", "#ff9f1c", "#ffd166", "#06d6a0", "#118ab2", "#4361ee",
    "#8338ec", "#ff70a6", "#8d6e63", "#b7c4cf", "#59656f", "#7cb518", "#c1121f", "#ffffff",
];
const MAIN_CSS: Asset = asset!("/assets/main.css");

#[derive(Clone, PartialEq)]
struct Frame {
    /// Row-major palette indices. Index zero is transparent.
    pixels: Vec<u8>,
}

#[derive(Clone, Copy, PartialEq)]
enum Tool {
    Pencil,
    Select,
    Fill,
    Paste,
}

#[derive(Clone, Copy, PartialEq)]
struct Selection {
    x: usize,
    y: usize,
    width: usize,
    height: usize,
}

#[derive(Clone, PartialEq)]
struct Clipboard {
    width: usize,
    height: usize,
    pixels: Vec<u8>,
}

#[derive(Clone, PartialEq)]
struct SelectionDrag {
    start_x: usize,
    start_y: usize,
    source: Selection,
    pixels: Vec<u8>,
}

#[derive(Serialize)]
struct AnimationExport {
    spritesheet: String,
    width: usize,
    height: usize,
    anims: Vec<AnimationDefinition>,
}

#[derive(Serialize)]
struct AnimationDefinition {
    name: String,
    frametime: f32,
    frames: Vec<usize>,
}

fn main() {
    dioxus::launch(App);
}

#[component]
fn App() -> Element {
    let mut frames = use_signal(|| {
        vec![Frame {
            pixels: vec![0; INITIAL_WIDTH * INITIAL_HEIGHT],
        }]
    });
    let mut width = use_signal(|| INITIAL_WIDTH);
    let mut height = use_signal(|| INITIAL_HEIGHT);
    let mut frame_time_ms = use_signal(|| INITIAL_FRAME_TIME_MS);
    let mut selected_frame = use_signal(|| 0usize);
    let mut selected_color = use_signal(|| 2usize);
    let mut right_erasing = use_signal(|| false);
    let mut tool = use_signal(|| Tool::Pencil);
    let mut selection = use_signal(|| None::<Selection>);
    let mut selection_anchor = use_signal(|| None::<(usize, usize)>);
    let mut selection_drag = use_signal(|| None::<SelectionDrag>);
    let mut clipboard = use_signal(|| None::<Clipboard>);
    let mut hovered_pixel = use_signal(|| None::<usize>);
    let mut painting = use_signal(|| false);
    let mut playing = use_signal(|| false);
    let mut preview_open = use_signal(|| false);
    let mut onion_skin = use_signal(|| true);
    let mut zoom = use_signal(|| 1.0f32);
    let mut animation_name = use_signal(|| String::from("CNIPIX_ANIMATION"));
    let mut status = use_signal(|| String::from("Ready · choose a color and draw"));

    let play_state = playing;
    let delay_state = frame_time_ms;
    let mut index_state = selected_frame;
    let frame_state = frames;
    use_future(move || async move {
        let mut elapsed = 0u32;
        loop {
            gloo_timers::future::TimeoutFuture::new(16).await;
            if play_state() {
                elapsed += 16;
                let delay = *delay_state.read();
                if elapsed >= delay {
                    elapsed = 0;
                    let count = frame_state.read().len();
                    if count > 0 {
                        index_state.with_mut(|index| *index = (*index + 1) % count);
                    }
                }
            } else {
                elapsed = 0;
            }
        }
    });

    let w = *width.read();
    let h = *height.read();
    let frame_time = *frame_time_ms.read();
    let active = (*selected_frame.read()).min(frames.read().len().saturating_sub(1));
    let selected = *selected_color.read();
    let active_tool = *tool.read();
    let selected_region = *selection.read();
    let clipboard_available = clipboard.read().is_some();
    let zoom_level = *zoom.read();
    let is_onion = *onion_skin.read();
    let is_playing = *playing.read();
    let is_preview_open = *preview_open.read();
    let name = animation_name.read().clone();
    let frame_list = frames.read().clone();
    let current_pixels = frame_list[active].pixels.clone();
    let selected_color_label = if selected == 0 {
        "Transparent / erase"
    } else {
        PALETTE[selected]
    };
    let frame_position_label = format!("Frame {} of {}", active + 1, frame_list.len());
    let preview_frame_label = format!("Frame {} of {}", active + 1, frame_list.len());
    let zoom_label = format!("{}%", (zoom_level * 100.0).round() as u32);
    let fps_label = format!("{} fps", (1000.0 / frame_time as f32).round() as u32);
    let previous_frame = if frame_list.len() > 1 {
        Some((active + frame_list.len() - 1) % frame_list.len())
    } else {
        None
    };
    let previous_pixels = previous_frame.map(|index| frame_list[index].pixels.clone());
    let zoomed_pixel = (24.0 * zoom_level).round().max(6.0);
    let preview_pixel = (70.0_f32 * 8.0 / w as f32)
        .min(62.0_f32 * 8.0 / h as f32)
        .clamp(4.0, 28.0);
    let preview_grid_style = format!(
        "grid-template-columns: repeat({w}, {preview_pixel}px); --preview-pixel-size: {preview_pixel}px;"
    );
    let grid_style = format!(
        "grid-template-columns: repeat({w}, {zoomed_pixel}px); --pixel-size: {zoomed_pixel}px;"
    );

    rsx! {
        document::Title { "cnipix · pixel animation studio" }
        document::Link { rel: "stylesheet", href: MAIN_CSS }
        main {
            class: "app-shell",
            tabindex: "0",
            onkeydown: move |event| {
                use wasm_bindgen::JsCast;
                let data = event.data();
                let Some(keyboard) = data.downcast::<web_sys::KeyboardEvent>() else { return; };
                if let Some(target) = keyboard.target().and_then(|target| target.dyn_into::<web_sys::Element>().ok()) {
                    if matches!(target.tag_name().as_str(), "INPUT" | "TEXTAREA" | "SELECT") { return; }
                }
                let key = keyboard.key().to_ascii_lowercase();
                if keyboard.ctrl_key() || keyboard.meta_key() {
                    match key.as_str() {
                        "c" => {
                            event.prevent_default();
                            if let Some(region) = *selection.read() {
                                let frame = *selected_frame.read();
                                let pixels = frames.read()[frame].pixels.clone();
                                clipboard.set(Some(copy_region(&pixels, *width.read(), region)));
                                status.set("Selection copied".to_owned());
                            } else {
                                status.set("Select an area before copying".to_owned());
                            }
                        }
                        "v" => {
                            event.prevent_default();
                            if let Some(copy) = clipboard.read().clone() {
                                if let Some(index) = *hovered_pixel.read() {
                                    let frame = *selected_frame.read();
                                    let canvas_width = *width.read();
                                    let region = paste_clipboard(
                                        &mut frames,
                                        frame,
                                        canvas_width,
                                        *height.read(),
                                        index % canvas_width,
                                        index / canvas_width,
                                        &copy,
                                    );
                                    selection.set(Some(region));
                                    tool.set(Tool::Select);
                                    status.set("Selection pasted".to_owned());
                                } else {
                                    tool.set(Tool::Paste);
                                    status.set("Copied pixels ready · click the canvas to place them".to_owned());
                                }
                            }
                        }
                        _ => {}
                    }
                    return;
                }
                if keyboard.alt_key() { return; }
                if keyboard.shift_key() {
                    let shifted_color = match keyboard.code().as_str() {
                        "Digit1" => Some(10),
                        "Digit2" => Some(11),
                        "Digit3" => Some(12),
                        "Digit4" => Some(13),
                        "Digit5" => Some(14),
                        "Digit6" => Some(15),
                        _ => None,
                    };
                    if let Some(color) = shifted_color {
                        selected_color.set(color);
                        return;
                    }
                }
                match key.as_str() {
                    "0" => selected_color.set(0),
                    "escape" => {
                        if selection.read().is_some() {
                            event.prevent_default();
                            selection.set(None);
                            selection_anchor.set(None);
                            selection_drag.set(None);
                            status.set("Selection cleared".to_owned());
                        }
                    }
                    "delete" | "backspace" => {
                        if let Some(region) = *selection.read() {
                            event.prevent_default();
                            let frame = *selected_frame.read();
                            let canvas_width = *width.read();
                            frames.with_mut(|all| {
                                for y in region.y..region.y + region.height {
                                    for x in region.x..region.x + region.width {
                                        all[frame].pixels[y * canvas_width + x] = 0;
                                    }
                                }
                            });
                            status.set("Selection cleared to transparent".to_owned());
                        }
                    }
                    "b" => tool.set(Tool::Pencil),
                    "m" => tool.set(Tool::Select),
                    "g" => tool.set(Tool::Fill),
                    _ => {
                        if key.len() == 1 {
                            if let Ok(color) = key.parse::<usize>() {
                                if color <= 9 { selected_color.set(color); }
                            }
                        }
                    }
                }
            },
            onpointerup: move |_| {
                painting.set(false);
                right_erasing.set(false);
                selection_anchor.set(None);
                if let (Some(drag), Some(destination)) = (selection_drag.read().clone(), *selection.read()) {
                    let frame = *selected_frame.read();
                    commit_selection_move(&mut frames, frame, *width.read(), *height.read(), drag, destination);
                }
                selection_drag.set(None);
            },
            onpointercancel: move |_| { painting.set(false); right_erasing.set(false); selection_anchor.set(None); selection_drag.set(None); },
            header {
                class: "topbar",
                div { class: "brand-lockup",
                    div { class: "brand-mark", "c" }
                    div { class: "brand-copy",
                        strong { "cnipix" }
                        span { "pixel animation studio" }
                    }
                }
                div { class: "topbar-project",
                    span { class: "live-dot" }
                    span { "Untitled sprite" }
                    span { class: "project-size", "{w} × {h}" }
                }
                div { class: "topbar-actions",
                    label { class: "name-label", "ANIMATION" }
                    input {
                        class: "name-input",
                        value: "{name}",
                        oninput: move |event| animation_name.set(event.value()),
                        placeholder: "Animation name"
                    }
                    button {
                        class: "button button-primary export-button",
                        onclick: move |_| {
                            let export = AnimationExport {
                                spritesheet: SPRITESHEET_PATH.to_owned(),
                                width: *width.read(),
                                height: *height.read(),
                                anims: vec![AnimationDefinition {
                                    name: if animation_name.read().trim().is_empty() { "CNIPIX_ANIMATION".to_owned() } else { animation_name.read().trim().to_owned() },
                                    frametime: *frame_time_ms.read() as f32 / 1000.0,
                                    frames: (0..frames.read().len()).collect(),
                                }],
                            };
                            let json = serde_json::to_string_pretty(&export);
                            let png = encode_spritesheet(&frames.read(), *width.read(), *height.read());
                            match (json, png) {
                                (Ok(json), Ok(png)) => {
                                    let json_result = download_file("cnipix_animated_sprite.json", "application/json", json.as_bytes());
                                    let png_result = download_file("cnipix_spritesheet.png", "image/png", &png);
                                    status.set(if json_result.is_ok() && png_result.is_ok() { "Downloaded game JSON and sprite sheet".to_owned() } else { "Export could not start in this browser".to_owned() });
                                }
                                (Err(error), _) => status.set(format!("JSON export failed: {error}")),
                                (_, Err(error)) => status.set(format!("PNG export failed: {error}")),
                            }
                        },
                        span { class: "download-icon", "↓" }
                        "Export game files"
                    }
                }
            }

            section { class: "toolbar",
                div { class: "tool-group",
                    button { class: "button button-soft", onclick: move |_| {
                        let at = *selected_frame.read() + 1;
                        frames.with_mut(|items| items.insert(at, Frame { pixels: vec![0; *width.read() * *height.read()] }));
                        selected_frame.set(at);
                        playing.set(false);
                    }, span { class: "plus-icon", "+" } "New frame" }
                    button { class: "button button-quiet", onclick: move |_| {
                        let at = *selected_frame.read() + 1;
                        let copy = frames.read()[*selected_frame.read()].clone();
                        frames.with_mut(|items| items.insert(at, copy));
                        selected_frame.set(at);
                    }, "Duplicate" }
                    button { class: if active_tool == Tool::Pencil { "button button-tool active" } else { "button button-tool" }, onclick: move |_| tool.set(Tool::Pencil), "✎ Pencil (B)" }
                    button { class: if active_tool == Tool::Select { "button button-tool active" } else { "button button-tool" }, onclick: move |_| tool.set(Tool::Select), "▧ Select (M)" }
                    button { class: if active_tool == Tool::Fill { "button button-tool active" } else { "button button-tool" }, onclick: move |_| tool.set(Tool::Fill), "▨ Fill (G)" }
                    button { class: "button button-quiet", disabled: selected_region.is_none(), onclick: move |_| {
                        if let Some(region) = *selection.read() {
                            let frame = *selected_frame.read();
                            let pixels = frames.read()[frame].pixels.clone();
                            clipboard.set(Some(copy_region(&pixels, *width.read(), region)));
                            status.set("Selection copied · choose Paste and click a destination".to_owned());
                        }
                    }, "Copy" }
                    button { class: "button button-quiet", disabled: !clipboard_available, onclick: move |_| {
                        if clipboard.read().is_some() {
                            tool.set(Tool::Paste);
                            status.set("Click the canvas to place the copied pixels".to_owned());
                        }
                    }, "Paste" }
                    button { class: "button button-quiet", disabled: selected_region.is_none(), onclick: move |_| {
                        selection.set(None);
                        selection_anchor.set(None);
                        selection_drag.set(None);
                    }, "Deselect" }
                    button { class: if is_playing { "button button-play active" } else { "button button-play" }, onclick: move |_| {
                        if is_playing {
                            playing.set(false);
                            preview_open.set(false);
                        } else {
                            selected_frame.set(0);
                            preview_open.set(true);
                            playing.set(true);
                        }
                    },
                        if is_playing { "■  Stop preview" } else { "▶  Preview animation" }
                    }
                }
                div { class: "toolbar-divider" }
                button { class: if is_onion { "button button-quiet toggle-on" } else { "button button-quiet" }, onclick: move |_| onion_skin.set(!onion_skin()),
                    span { class: "onion-icon", "◉" }
                    if is_onion { "Onion skin" } else { "Onion skin off" }
                }
                div { class: "toolbar-spacer" }
                div { class: "zoom-controls",
                    button { class: "icon-button", title: "Zoom out", onclick: move |_| zoom.set((zoom() - 0.25).max(0.25)), "−" }
                    span { class: "zoom-label", "{zoom_label}" }
                    button { class: "icon-button", title: "Zoom in", onclick: move |_| zoom.set((zoom() + 0.25).min(8.0)), "+" }
                    button { class: "button button-quiet fit-button", onclick: move |_| zoom.set(1.0), "Reset zoom" }
                }
            }

            section { class: "editor-layout",
                aside { class: "sidebar",
                    section { class: "side-section palette-section",
                        div { class: "section-heading", span { "Palette" } span { class: "section-hint", "0 erase · 1–9 / ⇧1–6" } }
                        div { class: "palette-grid",
                            for (index, color) in PALETTE.iter().enumerate() {
                                button {
                                    key: "swatch-{index}",
                                    class: if index == selected { "swatch selected" } else { "swatch" },
                                    style: if index == 0 { "--swatch: transparent".to_owned() } else { format!("--swatch: {color}") },
                                    title: if index == 0 { "0 · Transparent / erase".to_owned() } else if index <= 9 { format!("{index} · Palette color {index}") } else { format!("Palette color {index}") },
                                    onclick: move |_| selected_color.set(index),
                                    if index == 0 { span { class: "clear-mark", "×" } }
                                }
                            }
                        }
                        div { class: "selected-color-info",
                            span { class: "color-chip", style: if selected == 0 { "background: repeating-conic-gradient(#3d4351 0% 25%, #242832 0% 50%) 50% / 10px 10px".to_owned() } else { format!("background:{}", PALETTE[selected]) } }
                            span { "{selected_color_label}" }
                        }
                    }
                    div { class: "side-rule" }
                    section { class: "side-section settings-section",
                        div { class: "section-heading", span { "Sprite cell" } span { class: "section-hint", "1–64 px" } }
                        label { class: "field-label", "Width" }
                        div { class: "stepper",
                            button { class: "stepper-button", onclick: move |_| {
                                let old_w = *width.read();
                                if old_w > 1 { let new_w = old_w - 1; resize_frames(&mut frames, old_w, *height.read(), new_w, *height.read()); width.set(new_w); }
                            }, "−" }
                            input { class: "number-input", r#type: "number", min: "1", max: "64", value: "{w}", oninput: move |event| {
                                if let Ok(next) = event.value().parse::<usize>() { let next = next.clamp(1, CELL_LIMIT); let old = *width.read(); if old != next { resize_frames(&mut frames, old, *height.read(), next, *height.read()); width.set(next); } }
                            } }
                            button { class: "stepper-button", onclick: move |_| {
                                let old_w = *width.read();
                                if old_w < CELL_LIMIT { let new_w = old_w + 1; resize_frames(&mut frames, old_w, *height.read(), new_w, *height.read()); width.set(new_w); }
                            }, "+" }
                        }
                        label { class: "field-label field-label-spaced", "Height" }
                        div { class: "stepper",
                            button { class: "stepper-button", onclick: move |_| {
                                let old_h = *height.read();
                                if old_h > 1 { let new_h = old_h - 1; resize_frames(&mut frames, *width.read(), old_h, *width.read(), new_h); height.set(new_h); }
                            }, "−" }
                            input { class: "number-input", r#type: "number", min: "1", max: "64", value: "{h}", oninput: move |event| {
                                if let Ok(next) = event.value().parse::<usize>() { let next = next.clamp(1, CELL_LIMIT); let old = *height.read(); if old != next { resize_frames(&mut frames, *width.read(), old, *width.read(), next); height.set(next); } }
                            } }
                            button { class: "stepper-button", onclick: move |_| {
                                let old_h = *height.read();
                                if old_h < CELL_LIMIT { let new_h = old_h + 1; resize_frames(&mut frames, *width.read(), old_h, *width.read(), new_h); height.set(new_h); }
                            }, "+" }
                        }
                    }
                    div { class: "side-rule" }
                    section { class: "side-section timing-section",
                        div { class: "section-heading", span { "Frame timing" } }
                        div { class: "timing-control",
                            input { r#type: "range", min: "10", max: "2000", step: "10", value: "{frame_time}", oninput: move |event| { if let Ok(ms) = event.value().parse::<u32>() { frame_time_ms.set(ms.clamp(10, 2000)); } } }
                            div { class: "timing-readout", span { "{frame_time} ms" } span { "{fps_label}" } }
                        }
                    }
                    div { class: "sidebar-note", span { class: "note-icon", "i" } span { "Transparent pixels show the onion layer and export with alpha." } }
                }

                section { class: "workspace",
                    div { class: "workspace-head",
                        div { class: "workspace-title", strong { "Canvas" } span { "{frame_position_label}" } }
                        div { class: "workspace-tools",
                            span { class: "dimension-badge", "{w} × {h}" }
                            span { class: "workspace-separator" }
                            span { class: "hint-text", "Scroll to zoom · drag scrollbars to pan" }
                        }
                    }
                    div { class: "canvas-viewport",
                        onwheel: move |event| {
                            event.prevent_default();
                            let delta = event.data().delta().strip_units().y;
                            let factor = (-delta * 0.0015).exp() as f32;
                            zoom.set((zoom() * factor).clamp(0.25, 8.0));
                        },
                        div {
                            class: "pixel-grid",
                            style: "{grid_style}",
                            tabindex: "0",
                            onpointerdown: move |event| {
                                use wasm_bindgen::JsCast;
                                if let Some(pointer) = event.data().downcast::<web_sys::PointerEvent>() {
                                    if let Some(element) = pointer.current_target().and_then(|target| target.dyn_into::<web_sys::HtmlElement>().ok()) {
                                        let _ = element.focus();
                                    }
                                }
                            },
                            oncontextmenu: move |event| event.prevent_default(),
                            onpointerleave: move |_| { painting.set(false); hovered_pixel.set(None); },
                            for (index, pixel) in current_pixels.iter().enumerate() {
                                PixelCell {
                                    key: "pixel-{index}",
                                    index,
                                    width: w,
                                    pixel: *pixel,
                                    ghost: previous_pixels.as_ref().map(|pixels| pixels[index]).unwrap_or(0),
                                    onion: is_onion,
                                    frames,
                                    selected_frame,
                                    selected_color,
                                    painting,
                                    right_erasing,
                                    tool,
                                    selection,
                                    selection_anchor,
                                    selection_drag,
                                    clipboard,
                                    hovered_pixel,
                                    height: h,
                                }
                            }
                        }
                    }
                    div { class: "canvas-caption",
                        span { class: "caption-left", if is_onion && previous_frame.is_some() { "◉ Previous frame ghosted beneath current frame" } else { "Transparent canvas" } }
                        span { "Pixel grid · {zoomed_pixel} px per pixel" }
                    }
                }
            }

            section { class: "timeline-panel",
                div { class: "timeline-heading",
                    div { class: "timeline-title", strong { "Timeline" } span { "{frame_list.len()} frames" } }
                    div { class: "timeline-actions",
                        button { class: "button button-quiet small-button", onclick: move |_| {
                            if frames.read().len() > 1 { let remove = *selected_frame.read(); frames.with_mut(|items| { items.remove(remove); }); selected_frame.set(remove.min(frames.read().len() - 1)); }
                        }, "Delete frame" }
                    }
                }
                div { class: "frame-strip",
                    for (frame_index, frame) in frame_list.iter().enumerate() {
                        button {
                            key: "frame-{frame_index}",
                            class: if frame_index == active { "frame-card active" } else { "frame-card" },
                            onclick: move |_| { selected_frame.set(frame_index); playing.set(false); },
                            div { class: "frame-card-top", FrameNumber { number: frame_index + 1 } span { class: "frame-duration", "{frame_time}ms" } }
                            div { class: "frame-thumb", style: format!("grid-template-columns: repeat({}, minmax(0, 1fr)); grid-template-rows: repeat({}, minmax(0, 1fr))", *width.read(), *height.read()),
                                for (pixel_index, color_index) in frame.pixels.iter().enumerate() {
                                    span { key: "thumb-{frame_index}-{pixel_index}", style: if *color_index == 0 { "background: transparent".to_owned() } else { format!("background:{}", PALETTE[*color_index as usize]) } }
                                }
                            }
                        }
                    }
                }
            }
            if is_preview_open {
                div { class: "preview-overlay",
                    div { class: "preview-dialog", role: "dialog", aria_label: "Animation preview",
                        div { class: "preview-header",
                            div { class: "preview-heading", strong { "Animation preview" } span { "{name}" } }
                            button { class: "icon-button preview-close", title: "Close preview", onclick: move |_| { preview_open.set(false); playing.set(false); }, "×" }
                        }
                        div { class: "preview-stage",
                            div { class: "preview-grid", style: "{preview_grid_style}",
                                for (pixel_index, color_index) in current_pixels.iter().enumerate() {
                                    span {
                                        key: "preview-pixel-{pixel_index}",
                                        style: if *color_index == 0 { "background: transparent".to_owned() } else { format!("background:{}", PALETTE[*color_index as usize]) }
                                    }
                                }
                            }
                        }
                        div { class: "preview-footer",
                            span { "{preview_frame_label}" }
                            span { "{frame_time} ms · {fps_label}" }
                            button { class: "button button-quiet small-button", onclick: move |_| { preview_open.set(false); playing.set(false); }, "Close preview" }
                        }
                    }
                }
            }
            footer { class: "statusbar",
                span { class: "status-ready", span { class: "live-dot" } "{status}" }
                span { class: "status-shortcuts", "Erase: right click · Tools: B / M / G · Colors: 0–9 · Copy / paste: Ctrl+C / Ctrl+V" }
                span { class: "status-version", "cnipix  ·  browser edition" }
            }
        }
    }
}

#[component]
fn FrameNumber(number: usize) -> Element {
    let label = format!("{number:02}");
    rsx! { span { "{label}" } }
}

fn resize_frames(
    frames: &mut Signal<Vec<Frame>>,
    old_width: usize,
    old_height: usize,
    new_width: usize,
    new_height: usize,
) {
    frames.with_mut(|all| {
        for frame in all {
            let mut resized = vec![0; new_width * new_height];
            for y in 0..old_height.min(new_height) {
                for x in 0..old_width.min(new_width) {
                    resized[y * new_width + x] = frame.pixels[y * old_width + x];
                }
            }
            frame.pixels = resized;
        }
    });
}

fn selection_from_points(x1: usize, y1: usize, x2: usize, y2: usize) -> Selection {
    Selection {
        x: x1.min(x2),
        y: y1.min(y2),
        width: x1.abs_diff(x2) + 1,
        height: y1.abs_diff(y2) + 1,
    }
}

fn copy_region(pixels: &[u8], canvas_width: usize, region: Selection) -> Clipboard {
    let mut copied = Vec::with_capacity(region.width * region.height);
    for y in region.y..region.y + region.height {
        for x in region.x..region.x + region.width {
            copied.push(pixels[y * canvas_width + x]);
        }
    }
    Clipboard {
        width: region.width,
        height: region.height,
        pixels: copied,
    }
}

fn paste_clipboard(
    frames: &mut Signal<Vec<Frame>>,
    frame: usize,
    canvas_width: usize,
    canvas_height: usize,
    x: usize,
    y: usize,
    clipboard: &Clipboard,
) -> Selection {
    let pasted_width = clipboard.width.min(canvas_width.saturating_sub(x));
    let pasted_height = clipboard.height.min(canvas_height.saturating_sub(y));
    frames.with_mut(|all| {
        for dy in 0..pasted_height {
            for dx in 0..pasted_width {
                all[frame].pixels[(y + dy) * canvas_width + x + dx] =
                    clipboard.pixels[dy * clipboard.width + dx];
            }
        }
    });
    Selection {
        x,
        y,
        width: pasted_width,
        height: pasted_height,
    }
}

fn commit_selection_move(
    frames: &mut Signal<Vec<Frame>>,
    frame: usize,
    canvas_width: usize,
    canvas_height: usize,
    drag: SelectionDrag,
    destination: Selection,
) {
    if drag.source.x == destination.x && drag.source.y == destination.y {
        return;
    }
    frames.with_mut(|all| {
        let pixels = &mut all[frame].pixels;
        for y in drag.source.y..drag.source.y + drag.source.height {
            for x in drag.source.x..drag.source.x + drag.source.width {
                pixels[y * canvas_width + x] = 0;
            }
        }
        for dy in 0..drag
            .source
            .height
            .min(canvas_height.saturating_sub(destination.y))
        {
            for dx in 0..drag
                .source
                .width
                .min(canvas_width.saturating_sub(destination.x))
            {
                pixels[(destination.y + dy) * canvas_width + destination.x + dx] =
                    drag.pixels[dy * drag.source.width + dx];
            }
        }
    });
}

fn flood_fill(
    frames: &mut Signal<Vec<Frame>>,
    frame: usize,
    width: usize,
    height: usize,
    start: usize,
    replacement: u8,
) {
    frames.with_mut(|all| {
        let pixels = &mut all[frame].pixels;
        let target = pixels[start];
        if target == replacement {
            return;
        }
        let mut stack = vec![start];
        while let Some(index) = stack.pop() {
            if pixels[index] != target {
                continue;
            }
            pixels[index] = replacement;
            let x = index % width;
            let y = index / width;
            if x > 0 {
                stack.push(index - 1);
            }
            if x + 1 < width {
                stack.push(index + 1);
            }
            if y > 0 {
                stack.push(index - width);
            }
            if y + 1 < height {
                stack.push(index + width);
            }
        }
    });
}

#[component]
fn PixelCell(
    index: usize,
    width: usize,
    height: usize,
    pixel: u8,
    ghost: u8,
    onion: bool,
    mut frames: Signal<Vec<Frame>>,
    selected_frame: Signal<usize>,
    selected_color: Signal<usize>,
    mut painting: Signal<bool>,
    mut right_erasing: Signal<bool>,
    mut tool: Signal<Tool>,
    mut selection: Signal<Option<Selection>>,
    mut selection_anchor: Signal<Option<(usize, usize)>>,
    mut selection_drag: Signal<Option<SelectionDrag>>,
    mut clipboard: Signal<Option<Clipboard>>,
    mut hovered_pixel: Signal<Option<usize>>,
) -> Element {
    let is_current = pixel > 0;
    let show_ghost = onion && !is_current && ghost > 0;
    let background = if is_current {
        PALETTE[pixel as usize].to_owned()
    } else if show_ghost {
        ghost_color(PALETTE[ghost as usize])
    } else {
        "transparent".to_owned()
    };
    let row = index / width;
    let col = index % width;
    let title = format!("Pixel {}, {}", col + 1, row + 1);
    let selected_here = selection.read().is_some_and(|region| {
        col >= region.x
            && col < region.x + region.width
            && row >= region.y
            && row < region.y + region.height
    });
    let class = if show_ghost {
        "pixel ghost-pixel"
    } else if is_current {
        "pixel inked"
    } else {
        "pixel"
    };
    let class = if selected_here {
        format!("{class} selected-pixel")
    } else {
        class.to_owned()
    };
    rsx! {
        div {
            class: "{class}",
            style: "background-color: {background}",
            title: "{title}",
            onpointerdown: move |event| {
                if event
                    .data()
                    .downcast::<web_sys::PointerEvent>()
                    .is_some_and(|pointer| pointer.button() == 2)
                {
                    event.prevent_default();
                    right_erasing.set(true);
                    frames.with_mut(|all| all[*selected_frame.read()].pixels[index] = 0);
                    return;
                }
                let frame = *selected_frame.read();
                let x = index % width;
                let y = index / width;
                let current_tool = *tool.read();
                match current_tool {
                    Tool::Pencil => {
                        painting.set(true);
                        let color = *selected_color.read() as u8;
                        frames.with_mut(|all| all[frame].pixels[index] = color);
                    }
                    Tool::Fill => {
                        let color = *selected_color.read() as u8;
                        flood_fill(&mut frames, frame, width, height, index, color);
                        tool.set(Tool::Pencil);
                    }
                    Tool::Select => {
                        let current_selection = *selection.read();
                        if let Some(region) = current_selection {
                            if x >= region.x && x < region.x + region.width && y >= region.y && y < region.y + region.height {
                                let pixels = copy_region(&frames.read()[frame].pixels, width, region).pixels;
                                selection_drag.set(Some(SelectionDrag { start_x: x, start_y: y, source: region, pixels }));
                            } else {
                                selection_anchor.set(Some((x, y)));
                                selection.set(Some(Selection { x, y, width: 1, height: 1 }));
                            }
                        } else {
                            selection_anchor.set(Some((x, y)));
                            selection.set(Some(Selection { x, y, width: 1, height: 1 }));
                        }
                    }
                    Tool::Paste => {
                        if let Some(copy) = clipboard.read().clone() {
                            let region = paste_clipboard(&mut frames, frame, width, height, x, y, &copy);
                            selection.set(Some(region));
                            tool.set(Tool::Select);
                        }
                    }
                }
            },
            onpointerenter: move |_| {
                hovered_pixel.set(Some(index));
                let frame = *selected_frame.read();
                if right_erasing() {
                    frames.with_mut(|all| all[frame].pixels[index] = 0);
                } else {
                    let current_tool = *tool.read();
                    match current_tool {
                        Tool::Pencil if painting() => {
                            let color = *selected_color.read() as u8;
                            frames.with_mut(|all| all[frame].pixels[index] = color);
                        }
                        Tool::Select => {
                            if let Some((anchor_x, anchor_y)) = *selection_anchor.read() {
                                selection.set(Some(selection_from_points(anchor_x, anchor_y, col, row)));
                            } else if let Some(drag) = selection_drag.read().clone() {
                                let new_x = (drag.source.x as isize + col as isize - drag.start_x as isize)
                                    .clamp(0, width.saturating_sub(drag.source.width) as isize) as usize;
                                let new_y = (drag.source.y as isize + row as isize - drag.start_y as isize)
                                    .clamp(0, height.saturating_sub(drag.source.height) as isize) as usize;
                                selection.set(Some(Selection { x: new_x, y: new_y, ..drag.source }));
                            }
                        }
                        _ => {}
                    }
                }
            },
        }
    }
}

fn ghost_color(hex: &str) -> String {
    let value = u32::from_str_radix(hex.trim_start_matches('#'), 16).unwrap_or(0);
    let r = (value >> 16) & 0xff;
    let g = (value >> 8) & 0xff;
    let b = value & 0xff;
    format!("rgba({r}, {g}, {b}, 0.42)")
}

fn encode_spritesheet(frames: &[Frame], width: usize, height: usize) -> Result<Vec<u8>, String> {
    let sheet_width = width
        .checked_mul(frames.len())
        .ok_or("sprite sheet is too large")?;
    let mut rgba = vec![0u8; sheet_width * height * 4];
    for (frame_index, frame) in frames.iter().enumerate() {
        for y in 0..height {
            for x in 0..width {
                let color_index = frame.pixels[y * width + x] as usize;
                let offset = (y * sheet_width + frame_index * width + x) * 4;
                if color_index > 0 {
                    let hex = u32::from_str_radix(PALETTE[color_index].trim_start_matches('#'), 16)
                        .unwrap_or(0);
                    rgba[offset] = ((hex >> 16) & 0xff) as u8;
                    rgba[offset + 1] = ((hex >> 8) & 0xff) as u8;
                    rgba[offset + 2] = (hex & 0xff) as u8;
                    rgba[offset + 3] = 255;
                }
            }
        }
    }
    let mut bytes = Vec::new();
    {
        let mut encoder = png::Encoder::new(&mut bytes, sheet_width as u32, height as u32);
        encoder.set_color(png::ColorType::Rgba);
        encoder.set_depth(png::BitDepth::Eight);
        let mut writer = encoder.write_header().map_err(|error| error.to_string())?;
        writer
            .write_image_data(&rgba)
            .map_err(|error| error.to_string())?;
    }
    Ok(bytes)
}

fn download_file(name: &str, mime: &str, bytes: &[u8]) -> Result<(), String> {
    use wasm_bindgen::JsCast;
    let payload = if mime == "image/png" {
        base64(bytes)
    } else {
        percent_encode(bytes)
    };
    let separator = if mime == "image/png" {
        ";base64,"
    } else {
        ";charset=utf-8,"
    };
    let data_url = format!("data:{mime}{separator}{payload}");
    let window = web_sys::window().ok_or("browser window is unavailable")?;
    let document = window.document().ok_or("browser document is unavailable")?;
    let anchor = document
        .create_element("a")
        .map_err(|error| format!("could not create download link: {error:?}"))?
        .dyn_into::<web_sys::HtmlElement>()
        .map_err(|_| "could not create download link")?;
    anchor
        .set_attribute("href", &data_url)
        .map_err(|error| format!("could not set download URL: {error:?}"))?;
    anchor
        .set_attribute("download", name)
        .map_err(|error| format!("could not set download name: {error:?}"))?;
    anchor.click();
    Ok(())
}

fn base64(bytes: &[u8]) -> String {
    const TABLE: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut output = String::with_capacity(bytes.len().div_ceil(3) * 4);
    for chunk in bytes.chunks(3) {
        let a = chunk[0];
        let b = *chunk.get(1).unwrap_or(&0);
        let c = *chunk.get(2).unwrap_or(&0);
        output.push(TABLE[(a >> 2) as usize] as char);
        output.push(TABLE[(((a & 0x03) << 4) | (b >> 4)) as usize] as char);
        output.push(if chunk.len() > 1 {
            TABLE[(((b & 0x0f) << 2) | (c >> 6)) as usize] as char
        } else {
            '='
        });
        output.push(if chunk.len() > 2 {
            TABLE[(c & 0x3f) as usize] as char
        } else {
            '='
        });
    }
    output
}

fn percent_encode(bytes: &[u8]) -> String {
    let mut output = String::with_capacity(bytes.len() * 3);
    for byte in bytes {
        if byte.is_ascii_alphanumeric() || matches!(*byte, b'-' | b'_' | b'.' | b'~') {
            output.push(*byte as char);
        } else {
            let _ = write!(output, "%{byte:02X}");
        }
    }
    output
}

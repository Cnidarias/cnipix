use dioxus::prelude::*;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fmt::Write as _;
use std::io::Cursor;

const INITIAL_WIDTH: usize = 24;
const INITIAL_HEIGHT: usize = 24;
const CELL_LIMIT: usize = 64;
const INITIAL_FRAME_TIME_MS: u32 = 120;
const SPRITESHEET_PATH: &str = "assets/player/cnipix_spritesheet.png";
const DEFAULT_PALETTE: [&str; 16] = [
    "#171923", "#f7f3e8", "#ef476f", "#ff9f1c", "#ffd166", "#06d6a0", "#118ab2", "#4361ee",
    "#8338ec", "#ff70a6", "#8d6e63", "#b7c4cf", "#59656f", "#7cb518", "#c1121f", "#ffffff",
];
const MAIN_CSS: Asset = asset!("/assets/main.css");
const FAVICON: Asset = asset!("/assets/favicon.svg");

#[derive(Clone, PartialEq, Serialize, Deserialize)]
struct Frame {
    /// Row-major palette indices. Index zero is transparent.
    pixels: Vec<u8>,
}

#[derive(Clone, PartialEq)]
struct DocumentSnapshot {
    frames: Vec<Frame>,
    width: usize,
    height: usize,
    palette: Vec<String>,
    selected_frame: usize,
}

#[derive(Default)]
struct History {
    undo: Vec<DocumentSnapshot>,
    redo: Vec<DocumentSnapshot>,
}

const HISTORY_LIMIT: usize = 100;

#[derive(Clone, Copy, PartialEq)]
enum Tool {
    Pencil,
    Select,
    Fill,
    Paste,
}

#[derive(Clone, Copy, PartialEq)]
enum SaveState {
    Unsaved,
    BrowserSaved,
    FileSaved,
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

#[derive(Serialize, Deserialize)]
struct AnimationDefinition {
    name: String,
    frametime: f32,
    frames: Vec<usize>,
}

#[derive(Deserialize)]
struct ProjectManifest {
    spritesheet: String,
    width: usize,
    height: usize,
    anims: Vec<AnimationDefinition>,
}

struct LoadedProject {
    width: usize,
    height: usize,
    name: String,
    frame_time_ms: u32,
    frames: Vec<Frame>,
    animation_count: usize,
    palette: Vec<String>,
}

#[derive(Serialize, Deserialize)]
struct AutosaveProject {
    version: u8,
    width: usize,
    height: usize,
    frame_time_ms: u32,
    frames: Vec<Frame>,
    palette: Vec<String>,
    animation_name: String,
    project_label: String,
    #[serde(default)]
    mapped_json_path: Option<String>,
    #[serde(default)]
    mapped_animation_index: Option<usize>,
}

#[derive(Clone, PartialEq, Deserialize)]
struct MappedProject {
    id: u32,
    path: String,
    animations: Vec<MappedAnimation>,
}

#[derive(Clone, PartialEq, Deserialize)]
struct MappedAnimation {
    name: String,
    index: usize,
    preview: String,
}

enum DirectoryRestore {
    NoSavedFolder,
    PermissionRequired,
    Mapped(Vec<MappedProject>),
}

#[derive(Clone, PartialEq)]
struct ActiveMapping {
    id: u32,
    animation_index: usize,
    json_path: String,
    manifest: String,
    png: Vec<u8>,
    original_frame_indices: Vec<usize>,
    original_width: usize,
    original_height: usize,
}

#[wasm_bindgen::prelude::wasm_bindgen(inline_js = r#"
let cnipixRoot = null;
let cnipixFiles = [];
let cnipixCanvasPan = null;
let cnipixCanvasPanOffset = { x: 0, y: 0 };
let cnipixCanvasPanInstalled = false;

export function cnipixInstallCanvasPan() {
  if (cnipixCanvasPanInstalled) return;
  cnipixCanvasPanInstalled = true;
  document.addEventListener('pointerdown', event => {
    if (!event.shiftKey || event.button !== 0 || !(event.target instanceof Element)) return;
    const viewport = event.target.closest('.canvas-viewport');
    if (!viewport) return;
    const canvas = viewport.querySelector('.pixel-grid');
    if (!canvas) return;
    event.preventDefault();
    event.stopPropagation();
    viewport.setPointerCapture(event.pointerId);
    viewport.classList.add('panning');
    cnipixCanvasPan = {
      viewport,
      canvas,
      pointerId: event.pointerId,
      startX: event.clientX,
      startY: event.clientY,
      offsetX: cnipixCanvasPanOffset.x,
      offsetY: cnipixCanvasPanOffset.y,
    };
  }, true);
  document.addEventListener('pointermove', event => {
    const pan = cnipixCanvasPan;
    if (!pan || event.pointerId !== pan.pointerId) return;
    event.preventDefault();
    cnipixCanvasPanOffset = {
      x: pan.offsetX + event.clientX - pan.startX,
      y: pan.offsetY + event.clientY - pan.startY,
    };
    pan.viewport.style.setProperty('--canvas-pan-x', `${cnipixCanvasPanOffset.x}px`);
    pan.viewport.style.setProperty('--canvas-pan-y', `${cnipixCanvasPanOffset.y}px`);
  }, true);
  const stopPan = event => {
    if (!cnipixCanvasPan || event.pointerId !== cnipixCanvasPan.pointerId) return;
    cnipixCanvasPan.viewport.classList.remove('panning');
    cnipixCanvasPan = null;
  };
  document.addEventListener('pointerup', stopPan, true);
  document.addEventListener('pointercancel', stopPan, true);
}

function cnipixHandleDatabase() {
  return new Promise((resolve, reject) => {
    const request = indexedDB.open('cnipix-browser-state', 1);
    request.onupgradeneeded = () => request.result.createObjectStore('handles');
    request.onsuccess = () => resolve(request.result);
    request.onerror = () => reject(request.error);
  });
}

async function cnipixStoreDirectoryHandle(handle) {
  const db = await cnipixHandleDatabase();
  await new Promise((resolve, reject) => {
    const request = db.transaction('handles', 'readwrite').objectStore('handles').put(handle, 'project-root');
    request.onsuccess = resolve;
    request.onerror = () => reject(request.error);
  });
  db.close();
}

async function cnipixGetDirectoryHandle() {
  const db = await cnipixHandleDatabase();
  const handle = await new Promise((resolve, reject) => {
    const request = db.transaction('handles', 'readonly').objectStore('handles').get('project-root');
    request.onsuccess = () => resolve(request.result || null);
    request.onerror = () => reject(request.error);
  });
  db.close();
  return handle;
}

export function cnipixReadAutosave() {
  try { return localStorage.getItem('cnipix.autosave.v1'); }
  catch { return null; }
}

export function cnipixWriteAutosave(content) {
  try { localStorage.setItem('cnipix.autosave.v1', content); return true; }
  catch (error) { console.warn('cnipix could not autosave this project', error); return false; }
}

export function cnipixClearAutosave() {
  try { localStorage.removeItem('cnipix.autosave.v1'); }
  catch { /* storage may be unavailable in private browsing */ }
}

export async function cnipixMapDirectory() {
  if (!window.showDirectoryPicker) throw new Error('Folder mapping requires a Chromium browser with File System Access support.');
  cnipixRoot = await window.showDirectoryPicker({ mode: 'readwrite' });
  try { await cnipixStoreDirectoryHandle(cnipixRoot); }
  catch (error) { console.warn('cnipix could not remember the selected folder', error); }
  return cnipixScanDirectory(cnipixRoot);
}

export async function cnipixRestoreDirectory() {
  const root = await cnipixGetDirectoryHandle();
  if (!root) return '';
  cnipixRoot = root;
  if (await root.queryPermission({ mode: 'readwrite' }) !== 'granted') return '__CNIPIX_PERMISSION_REQUIRED__';
  return cnipixScanDirectory(root);
}

export async function cnipixReconnectDirectory() {
  const root = cnipixRoot || await cnipixGetDirectoryHandle();
  if (!root) return '';
  const permission = root.requestPermission({ mode: 'readwrite' });
  if (await permission !== 'granted') return '__CNIPIX_PERMISSION_REQUIRED__';
  return cnipixScanDirectory(root);
}

async function cnipixScanDirectory(root) {
  cnipixRoot = root;
  cnipixFiles = [];
  const entries = [];
  async function walk(directory, prefix = '') {
    for await (const [name, handle] of directory.entries()) {
      const path = prefix ? `${prefix}/${name}` : name;
      if (handle.kind === 'directory') { await walk(handle, path); continue; }
      if (!name.toLowerCase().endsWith('.json')) continue;
      try {
        const data = JSON.parse(await (await handle.getFile()).text());
        if (typeof data.spritesheet !== 'string' || !Array.isArray(data.anims) || !data.anims.length) continue;
        const sheetPath = data.spritesheet.replaceAll(String.fromCharCode(92), '/');
        const candidates = [sheetPath, sheetPath.startsWith('assets/') ? sheetPath.slice(7) : sheetPath, `${path.slice(0, path.lastIndexOf('/') + 1)}${sheetPath.split('/').pop()}`];
        let sheet = null;
        for (const candidate of candidates) {
          try {
            let parent = cnipixRoot;
            const parts = candidate.split('/').filter(Boolean);
            for (const part of parts.slice(0, -1)) parent = await parent.getDirectoryHandle(part);
            sheet = await parent.getFileHandle(parts.at(-1));
            break;
          }
          catch { /* try the next project-relative path */ }
        }
        if (!sheet) continue;
        const animations = [];
        try {
          const bitmap = await createImageBitmap(await sheet.getFile());
          const cellWidth = Number(data.width), cellHeight = Number(data.height);
          const columns = Math.floor(bitmap.width / cellWidth);
          for (let index = 0; index < data.anims.length; index++) {
            const anim = data.anims[index];
            let preview = '';
            try {
              const frame = Number(anim.frames?.[0] ?? 0);
              if (cellWidth > 0 && cellHeight > 0 && columns > 0 && frame >= 0) {
                const canvas = document.createElement('canvas');
                canvas.width = cellWidth; canvas.height = cellHeight;
                canvas.getContext('2d').drawImage(bitmap, (frame % columns) * cellWidth, Math.floor(frame / columns) * cellHeight, cellWidth, cellHeight, 0, 0, cellWidth, cellHeight);
                preview = canvas.toDataURL('image/png');
              }
            } catch { /* the project remains openable without a thumbnail */ }
            animations.push({ name: String(anim.name || `Animation ${index + 1}`), index, preview });
          }
          bitmap.close();
        } catch {
          for (let index = 0; index < data.anims.length; index++) {
            const anim = data.anims[index];
            animations.push({ name: String(anim.name || `Animation ${index + 1}`), index, preview: '' });
          }
        }
        const id = cnipixFiles.push({ json: handle, sheet }) - 1;
        entries.push({ id, path, animations });
      } catch { /* ignore unrelated or malformed JSON files */ }
    }
  }
  await walk(cnipixRoot);
  entries.sort((a, b) => a.path.localeCompare(b.path));
  return JSON.stringify(entries);
}

export async function cnipixReadProject(id, animationIndex) {
  const entry = cnipixFiles[id];
  if (!entry) throw new Error('This mapped project is no longer available. Map the directory again.');
  const [manifest, pngFile] = await Promise.all([entry.json.getFile(), entry.sheet.getFile()]);
  const png = new Uint8Array(await pngFile.arrayBuffer());
  return { manifest: await manifest.text(), png, animationIndex };
}

export async function cnipixWriteProject(id, manifest, png) {
  const entry = cnipixFiles[id];
  if (!entry) throw new Error('This mapped project is no longer available. Map the directory again.');
  for (const handle of [entry.json, entry.sheet]) {
    if (!await handle.requestPermission({ mode: 'readwrite' }).then(state => state === 'granted')) {
      throw new Error('Write permission was not granted for the mapped files.');
    }
  }
  const pngBytes = new Uint8Array(png);
  const pngWriter = await entry.sheet.createWritable();
  await pngWriter.write(pngBytes);
  await pngWriter.close();
  const jsonWriter = await entry.json.createWritable();
  await jsonWriter.write(manifest);
  await jsonWriter.close();
  return true;
}
"#)]
extern "C" {
    #[wasm_bindgen::prelude::wasm_bindgen]
    fn cnipixInstallCanvasPan();
    #[wasm_bindgen::prelude::wasm_bindgen(catch)]
    fn cnipixRestoreDirectory() -> Result<js_sys::Promise, wasm_bindgen::JsValue>;
    #[wasm_bindgen::prelude::wasm_bindgen(catch)]
    fn cnipixReconnectDirectory() -> Result<js_sys::Promise, wasm_bindgen::JsValue>;
    #[wasm_bindgen::prelude::wasm_bindgen]
    fn cnipixReadAutosave() -> Option<String>;
    #[wasm_bindgen::prelude::wasm_bindgen]
    fn cnipixWriteAutosave(content: &str) -> bool;
    #[wasm_bindgen::prelude::wasm_bindgen]
    fn cnipixClearAutosave();
    #[wasm_bindgen::prelude::wasm_bindgen(catch)]
    fn cnipixMapDirectory() -> Result<js_sys::Promise, wasm_bindgen::JsValue>;
    #[wasm_bindgen::prelude::wasm_bindgen(catch)]
    fn cnipixReadProject(
        id: u32,
        animation_index: u32,
    ) -> Result<js_sys::Promise, wasm_bindgen::JsValue>;
    #[wasm_bindgen::prelude::wasm_bindgen(catch)]
    fn cnipixWriteProject(
        id: u32,
        manifest: &str,
        png: &js_sys::Uint8Array,
    ) -> Result<js_sys::Promise, wasm_bindgen::JsValue>;
}

#[derive(Serialize, Deserialize)]
struct PaletteFile {
    name: Option<String>,
    colors: Vec<String>,
}

#[derive(Deserialize)]
#[serde(untagged)]
enum PaletteDocument {
    Named(PaletteFile),
    Colors(Vec<String>),
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
    let mut history = use_signal(History::default);
    let mut width = use_signal(|| INITIAL_WIDTH);
    let mut height = use_signal(|| INITIAL_HEIGHT);
    let mut frame_time_ms = use_signal(|| INITIAL_FRAME_TIME_MS);
    let mut selected_frame = use_signal(|| 0usize);
    let mut selected_color = use_signal(|| 2usize);
    let mut color_editor_open = use_signal(|| false);
    let mut new_color_value = use_signal(|| String::from("#ffffff"));
    let mut palette = use_signal(|| {
        DEFAULT_PALETTE
            .iter()
            .map(|color| (*color).to_owned())
            .collect::<Vec<_>>()
    });
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
    let mut project_label = use_signal(|| String::from("Untitled sprite"));
    let mut directory_projects = use_signal(Vec::<MappedProject>::new);
    let mut directory_mapped = use_signal(|| false);
    let mut directory_browser_open = use_signal(|| false);
    let mut directory_permission_needed = use_signal(|| false);
    let mut active_mapping = use_signal(|| None::<ActiveMapping>);
    let mut mapping_target = use_signal(|| None::<(String, usize)>);
    let mut status = use_signal(|| String::from("Ready · choose a color and draw"));
    let mut autosave_ready = use_signal(|| false);
    let mut save_state = use_signal(|| SaveState::Unsaved);
    let mut autosave_revision = use_signal(|| 0u64);

    cnipixInstallCanvasPan();

    use_future(move || async move {
        match cnipixReadAutosave() {
            Some(contents) => match serde_json::from_str::<AutosaveProject>(&contents) {
                Ok(draft) if validate_autosave(&draft).is_ok() => {
                    width.set(draft.width);
                    height.set(draft.height);
                    frame_time_ms.set(draft.frame_time_ms.clamp(10, 2000));
                    frames.set(draft.frames);
                    palette.set(draft.palette);
                    animation_name.set(draft.animation_name);
                    project_label.set(draft.project_label);
                    selected_frame.set(0);
                    mapping_target.set(draft.mapped_json_path.zip(draft.mapped_animation_index));
                    status.set("Restored autosaved project".to_owned());
                }
                Ok(_) | Err(_) => {
                    cnipixClearAutosave();
                    status.set("Ignored an invalid autosave draft".to_owned());
                }
            },
            None => {}
        }
        autosave_ready.set(true);
        if let Ok(promise) = cnipixRestoreDirectory() {
            match restore_project_directory(promise).await {
                Ok(DirectoryRestore::NoSavedFolder) => {}
                Ok(DirectoryRestore::PermissionRequired) => {
                    directory_permission_needed.set(true);
                    status.set(
                        "Saved folder needs permission · open Project folder to reconnect"
                            .to_owned(),
                    );
                }
                Ok(DirectoryRestore::Mapped(projects)) => {
                    if let Ok(Some(mapping)) = reopen_saved_mapping(
                        &projects,
                        mapping_target.read().clone(),
                        palette.read().clone(),
                    )
                    .await
                    {
                        active_mapping.set(Some(mapping));
                    }
                    let count = projects
                        .iter()
                        .map(|project| project.animations.len())
                        .sum::<usize>();
                    directory_projects.set(projects);
                    directory_mapped.set(true);
                    status.set(format!("Restored mapped folder · {count} animations ready"));
                }
                Err(error) => status.set(format!("Could not restore saved folder: {error}")),
            }
        }
    });

    use_effect(move || {
        if !*autosave_ready.read() {
            return;
        }
        autosave_revision.with_mut(|current| *current += 1);
        if *save_state.peek() != SaveState::Unsaved {
            save_state.set(SaveState::Unsaved);
        }
    });

    let autosave_revision_state = autosave_revision;
    let autosave_ready_state = autosave_ready;
    let mut autosave_save_state = save_state;
    use_future(move || async move {
        let mut observed_revision = 0u64;
        let mut saved_revision = 0u64;
        let mut idle_ms = 0u32;
        loop {
            gloo_timers::future::TimeoutFuture::new(100).await;
            if !*autosave_ready_state.read() {
                continue;
            }
            let revision = *autosave_revision_state.read();
            if revision != observed_revision {
                observed_revision = revision;
                idle_ms = 0;
                continue;
            }
            if revision == 0 || revision == saved_revision {
                continue;
            }
            idle_ms += 100;
            if idle_ms < 500 {
                continue;
            }

            let draft = AutosaveProject {
                version: 1,
                width: *width.read(),
                height: *height.read(),
                frame_time_ms: *frame_time_ms.read(),
                frames: frames.read().clone(),
                palette: palette.read().clone(),
                animation_name: animation_name.read().clone(),
                project_label: project_label.read().clone(),
                mapped_json_path: mapping_target.read().as_ref().map(|(path, _)| path.clone()),
                mapped_animation_index: mapping_target.read().as_ref().map(|(_, index)| *index),
            };
            if let Ok(contents) = serde_json::to_string(&draft) {
                if cnipixWriteAutosave(&contents)
                    && *autosave_save_state.peek() == SaveState::Unsaved
                {
                    autosave_save_state.set(SaveState::BrowserSaved);
                }
            }
            saved_revision = revision;
            idle_ms = 0;
        }
    });

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
    let current_palette = palette.read().clone();
    let active_tool = *tool.read();
    let selected_region = *selection.read();
    let clipboard_available = clipboard.read().is_some();
    let can_undo = !history.read().undo.is_empty();
    let can_redo = !history.read().redo.is_empty();
    let zoom_level = *zoom.read();
    let is_onion = *onion_skin.read();
    let is_playing = *playing.read();
    let is_preview_open = *preview_open.read();
    let is_directory_browser_open = *directory_browser_open.read();
    let mapped_projects = directory_projects.read().clone();
    let has_mapped_projects = *directory_mapped.read();
    let mapped_animation_count = mapped_projects
        .iter()
        .map(|project| project.animations.len())
        .sum::<usize>();
    let active_mapping_value = active_mapping.read().clone();
    let mapped_project_active = active_mapping.read().is_some();
    let name = animation_name.read().clone();
    let (save_indicator_class, save_indicator_label) = match *save_state.read() {
        SaveState::Unsaved => ("live-dot save-unsaved", "Unsaved changes"),
        SaveState::BrowserSaved => ("live-dot save-browser", "Saved in this browser"),
        SaveState::FileSaved => ("live-dot save-file", "Saved to project or exported"),
    };
    let frame_list = frames.read().clone();
    let current_pixels = frame_list[active].pixels.clone();
    let selected_color_label = if selected == 0 {
        "Transparent / erase"
    } else {
        current_palette[selected].as_str()
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
        document::Link { rel: "icon", href: FAVICON, r#type: "image/svg+xml" }
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
                        "z" if keyboard.shift_key() => {
                            event.prevent_default();
                            if redo_document(&mut history, &mut frames, &mut width, &mut height, &mut frame_time_ms, &mut palette, &mut animation_name, &mut selected_frame) {
                                status.set("Redo".to_owned());
                            }
                        }
                        "z" => {
                            event.prevent_default();
                            if undo_document(&mut history, &mut frames, &mut width, &mut height, &mut frame_time_ms, &mut palette, &mut animation_name, &mut selected_frame) {
                                status.set("Undo".to_owned());
                            }
                        }
                        "y" => {
                            event.prevent_default();
                            if redo_document(&mut history, &mut frames, &mut width, &mut height, &mut frame_time_ms, &mut palette, &mut animation_name, &mut selected_frame) {
                                status.set("Redo".to_owned());
                            }
                        }
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
                                    record_document(&mut history, &frames, &width, &height, &frame_time_ms, &palette, &animation_name, &selected_frame);
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
                        if color < palette.read().len() {
                            selected_color.set(color);
                        }
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
                            record_document(&mut history, &frames, &width, &height, &frame_time_ms, &palette, &animation_name, &selected_frame);
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
                                if color <= 9 && color < palette.read().len() {
                                    selected_color.set(color);
                                }
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
                    img { class: "brand-mark", src: FAVICON, alt: "cnipix jellyfish" }
                    div { class: "brand-copy",
                        strong { "cnipix" }
                        span { "pixel animation studio" }
                    }
                }
                div { class: "topbar-project",
                    span { class: "{save_indicator_class}", title: "{save_indicator_label}", aria_label: "{save_indicator_label}" }
                    span { "{project_label}" }
                    span { class: "project-size", "{w} × {h}" }
                }
                div { class: "topbar-actions",
                    button { class: "button button-quiet", onclick: move |_| {
                        directory_browser_open.set(true);
                        if directory_projects.read().is_empty() {
                            status.set("Choose a game or assets folder to map".to_owned());
                        }
                    }, "▣ Project folder" }
                    label { class: "button button-quiet load-project-button",
                        span { "↑ Load project" }
                        input {
                            class: "project-file-input",
                            r#type: "file",
                            accept: ".json,.png",
                            multiple: true,
                            onchange: move |event| {
                                let files = event.files();
                                if files.is_empty() { return; }
                                status.set("Loading project files…".to_owned());
                                let import_palette = palette.read().clone();
                                spawn(async move {
                                    match load_project_files(files, import_palette).await {
                                        Ok(project) => {
                                            let loaded_message = if project.animation_count > 1 {
                                                format!("Loaded {} · first of {} animations", project.name, project.animation_count)
                                            } else {
                                                format!("Loaded {}", project.name)
                                            };
                                            width.set(project.width);
                                            height.set(project.height);
                                            frame_time_ms.set(project.frame_time_ms);
                                            project_label.set(project.name.clone());
                                            animation_name.set(project.name);
                                            palette.set(project.palette.clone());
                                            frames.set(project.frames);
                                            history.set(History::default());
                                            active_mapping.set(None);
                                            mapping_target.set(None);
                                            selected_frame.set(0);
                                            playing.set(false);
                                            preview_open.set(false);
                                            zoom.set(1.0);
                                            selection.set(None);
                                            selection_anchor.set(None);
                                            selection_drag.set(None);
                                            clipboard.set(None);
                                            tool.set(Tool::Pencil);
                                            status.set(loaded_message);
                                        }
                                        Err(error) => status.set(format!("Project load failed: {error}")),
                                    }
                                });
                            }
                        }
                    }
                    label { class: "name-label", "ANIMATION" }
                    input {
                        class: "name-input",
                        value: "{name}",
                        oninput: move |event| animation_name.set(event.value()),
                        placeholder: "Animation name"
                    }
                    if mapped_project_active {
                        button { class: "button button-soft", onclick: move |_| {
                            let Some(mapping) = active_mapping.read().clone() else { return; };
                            let current_frames = frames.read().clone();
                            let current_width = *width.read();
                            let current_height = *height.read();
                            let current_timing = *frame_time_ms.read();
                            let current_name = animation_name.read().clone();
                            let current_palette = palette.read().clone();
                            let saved_revision = *autosave_revision.read();
                            match prepare_mapped_save(&mapping, &current_frames, current_width, current_height, current_timing, &current_name, &current_palette) {
                                Ok((manifest, png)) => {
                                    status.set("Saving changes to the mapped JSON and sprite sheet…".to_owned());
                                    spawn(async move {
                                        match write_mapped_project(mapping.id, &manifest, &png).await {
                                            Ok(()) => {
                                                let saved_frame_indices = mapped_frame_indices(&manifest, mapping.animation_index);
                                                active_mapping.with_mut(|active| if let Some(active) = active { active.manifest = manifest; active.png = png; active.original_frame_indices = saved_frame_indices; });
                                                if *autosave_revision.read() == saved_revision {
                                                    save_state.set(SaveState::FileSaved);
                                                }
                                                status.set("Saved changes to the mapped JSON and sprite sheet".to_owned());
                                            }
                                            Err(error) => status.set(format!("Save failed: {error}")),
                                        }
                                    });
                                }
                                Err(error) => status.set(format!("Cannot save this edit: {error}")),
                            }
                        }, "↓ Save to folder" }
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
                            let png = encode_spritesheet(&frames.read(), *width.read(), *height.read(), &palette.read());
                            match (json, png) {
                                (Ok(json), Ok(png)) => {
                                    let json_result = download_file("cnipix_animated_sprite.json", "application/json", json.as_bytes());
                                    let png_result = download_file("cnipix_spritesheet.png", "image/png", &png);
                                    if json_result.is_ok() && png_result.is_ok() {
                                        save_state.set(SaveState::FileSaved);
                                        status.set("Downloaded game JSON and sprite sheet".to_owned());
                                    } else {
                                        status.set("Export could not start in this browser".to_owned());
                                    }
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

            if is_directory_browser_open {
                div { class: "project-browser-backdrop", onclick: move |_| directory_browser_open.set(false),
                    section { class: "project-browser", onclick: move |event| event.stop_propagation(),
                        header { class: "project-browser-header",
                            div { strong { "Mapped projects" } span { "Open animation files from a game folder" } }
                            div { class: "project-browser-actions",
                                if *directory_permission_needed.read() {
                                    button { class: "button button-soft", onclick: move |_| {
                                        status.set("Requesting access to the saved folder…".to_owned());
                                        match cnipixReconnectDirectory() {
                                            Ok(promise) => { spawn(async move {
                                                match restore_project_directory(promise).await {
                                                    Ok(DirectoryRestore::Mapped(projects)) => {
                                                        match reopen_saved_mapping(&projects, mapping_target.read().clone(), palette.read().clone()).await {
                                                            Ok(Some(mapping)) => active_mapping.set(Some(mapping)),
                                                            Ok(None) => {},
                                                            Err(error) => status.set(format!("Could not reconnect the active project: {error}")),
                                                        }
                                                        let count = projects.iter().map(|project| project.animations.len()).sum::<usize>();
                                                        directory_projects.set(projects);
                                                        directory_mapped.set(true);
                                                        directory_permission_needed.set(false);
                                                        status.set(format!("Reconnected saved folder · {count} animations ready"));
                                                    }
                                                    Ok(DirectoryRestore::PermissionRequired) => status.set("Folder access was not granted".to_owned()),
                                                    Ok(DirectoryRestore::NoSavedFolder) => status.set("No saved folder handle is available · choose the folder again".to_owned()),
                                                    Err(error) => status.set(format!("Could not reconnect saved folder: {error}")),
                                                }
                                            }); },
                                            Err(error) => status.set(format!("Could not reconnect saved folder: {}", js_error_message(error))),
                                        }
                                    }, "Reconnect saved folder" }
                                }
                                button { class: "button button-soft", onclick: move |_| {
                                    status.set("Choose a game root or its assets folder…".to_owned());
                                    match cnipixMapDirectory() {
                                        Ok(promise) => {
                                            spawn(async move {
                                                match map_project_directory(promise).await {
                                                    Ok(projects) => {
                                                        let target = mapping_target.read().clone();
                                                        let saved_palette = palette.read().clone();
                                                        match reopen_saved_mapping(&projects, target, saved_palette).await {
                                                            Ok(Some(mapping)) => active_mapping.set(Some(mapping)),
                                                            Ok(None) => {
                                                                active_mapping.set(None);
                                                                mapping_target.set(None);
                                                            }
                                                            Err(error) => status.set(format!("Folder mapped, but the active project could not be reconnected: {error}")),
                                                        }
                                                        let count = projects.iter().map(|project| project.animations.len()).sum::<usize>();
                                                        directory_projects.set(projects);
                                                        directory_mapped.set(true);
                                                        directory_permission_needed.set(false);
                                                        directory_browser_open.set(false);
                                                        status.set(format!("Mapped folder · {count} animations ready"));
                                                    }
                                                    Err(error) => status.set(format!("Folder mapping failed: {error}")),
                                                }
                                            });
                                        }
                                        Err(error) => status.set(format!("Folder mapping failed: {}", js_error_message(error))),
                                    }
                                }, "Choose folder / Remap" }
                                button { class: "icon-button", title: "Close", onclick: move |_| directory_browser_open.set(false), "×" }
                            }
                        }
                        if mapped_projects.is_empty() {
                            div { class: "project-browser-empty",
                                div { class: "empty-icon", "▣" }
                                if *directory_mapped.read() {
                                    strong { "No editable animations found" }
                                    span { "This folder has no animation JSON files paired with a matching sprite sheet." }
                                } else {
                                    strong { "No folder mapped" }
                                    span { "Choose the game folder or its assets directory. cnipix will find animation JSON files and their sprite sheets." }
                                }
                            }
                        } else {
                            div { class: "project-browser-list",
                                for project in mapped_projects.iter() {
                                    div { class: "mapped-project-card",
                                        div { class: "mapped-project-file",
                                            span { class: "file-glyph", "▤" }
                                            div { strong { "{project.path}" } span { "{project.animations.len()} animations" } }
                                        }
                                        for animation in project.animations.iter() {
                                            {
                                                let project_id = project.id;
                                                let source_path = project.path.clone();
                                                let animation_index = animation.index;
                                                let animation_label = animation.name.clone();
                                                let display_animation = animation.name.clone();
                                                rsx! {
                                                    button { class: "mapped-animation-row", onclick: move |_| {
                                                        let project_name = animation_label.clone();
                                                        let source_path = source_path.clone();
                                                        let project_id = project_id;
                                                        let animation_index = animation_index;
                                                        let import_palette = palette.read().clone();
                                                        status.set(format!("Opening {project_name}…"));
                                                        spawn(async move {
                                                            match open_mapped_project(project_id, animation_index, &source_path, import_palette).await {
                                                                Ok((project, mapping)) => {
                                                                    width.set(project.width);
                                                                    height.set(project.height);
                                                                    frame_time_ms.set(project.frame_time_ms);
                                                                    project_label.set(mapping.json_path.clone());
                                                                    animation_name.set(project.name);
                                                                    palette.set(project.palette.clone());
                                                                    frames.set(project.frames);
                                                                    history.set(History::default());
                                                                    selected_frame.set(0);
                                                                    playing.set(false);
                                                                    preview_open.set(false);
                                                                    zoom.set(1.0);
                                                                    selection.set(None);
                                                                    selection_anchor.set(None);
                                                                    selection_drag.set(None);
                                                                    clipboard.set(None);
                                                                    mapping_target.set(Some((mapping.json_path.clone(), mapping.animation_index)));
                                                                    active_mapping.set(Some(mapping));
                                                                    directory_browser_open.set(false);
                                                                    status.set(format!("Opened {project_name} · edits can be saved to the mapped files"));
                                                                }
                                                                Err(error) => status.set(format!("Could not open project: {error}")),
                                                            }
                                                        });
                                                    },
                                                        span { class: "mapped-animation-name", "▶ {display_animation}" }
                                                        span { class: "mapped-open-label", "Open →" }
                                                    }
                                                }
                                            }
                                        }
                                    }
                                }
                            }
                        }
                        footer { class: "project-browser-footer",
                            span { "Folder access is remembered in this browser. Saving writes the JSON and PNG back into this folder." }
                            if mapped_project_active { span { class: "mapped-ready", "● Active project is mapped" } }
                        }
                    }
                }
            }

            section { class: "toolbar",
                div { class: "tool-group",
                    button { class: "button button-quiet", disabled: !can_undo, title: "Undo (Ctrl/⌘+Z)", onclick: move |_| { if undo_document(&mut history, &mut frames, &mut width, &mut height, &mut frame_time_ms, &mut palette, &mut animation_name, &mut selected_frame) { status.set("Undo".to_owned()); } }, "↶ Undo" }
                    button { class: "button button-quiet", disabled: !can_redo, title: "Redo (Ctrl/⌘+Y or Ctrl/⌘+Shift+Z)", onclick: move |_| { if redo_document(&mut history, &mut frames, &mut width, &mut height, &mut frame_time_ms, &mut palette, &mut animation_name, &mut selected_frame) { status.set("Redo".to_owned()); } }, "↷ Redo" }
                    button { class: "button button-soft", onclick: move |_| {
                        record_document(&mut history, &frames, &width, &height, &frame_time_ms, &palette, &animation_name, &selected_frame);
                        let at = *selected_frame.read() + 1;
                        frames.with_mut(|items| items.insert(at, Frame { pixels: vec![0; *width.read() * *height.read()] }));
                        selected_frame.set(at);
                        playing.set(false);
                    }, span { class: "plus-icon", "+" } "New frame" }
                    button { class: "button button-quiet", onclick: move |_| {
                        record_document(&mut history, &frames, &width, &height, &frame_time_ms, &palette, &animation_name, &selected_frame);
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

            section { class: if has_mapped_projects { "editor-layout with-projects" } else { "editor-layout" },
                if has_mapped_projects {
                    aside { class: "project-sidebar",
                        div { class: "project-sidebar-heading",
                            strong { "PROJECTS" }
                            button { class: "project-remap", title: "Map another folder", onclick: move |_| directory_browser_open.set(true), "↻" }
                        }
                        div { class: "project-sidebar-list",
                            if mapped_projects.is_empty() {
                                div { class: "project-sidebar-empty", "No matching animation JSON + PNG pairs found." }
                            }
                            for project in mapped_projects.iter() {
                                section { class: "project-sidebar-group",
                                    div { class: "project-sidebar-path", title: "{project.path}", "{project.path}" }
                                    for animation in project.animations.iter() {
                                        {
                                            let project_id = project.id;
                                            let source_path = project.path.clone();
                                            let animation_index = animation.index;
                                            let animation_label = animation.name.clone();
                                            let import_palette = palette.read().clone();
                                            let is_active = active_mapping_value.as_ref().is_some_and(|active| active.id == project_id && active.animation_index == animation_index);
                                            rsx! {
                                                button { class: if is_active { "project-sidebar-item active" } else { "project-sidebar-item" }, title: "{animation_label}", onclick: move |_| {
                                                    let project_name = animation_label.clone();
                                                    let source_path = source_path.clone();
                                                    let import_palette = import_palette.clone();
                                                    status.set(format!("Opening {project_name}…"));
                                                    spawn(async move {
                                                        match open_mapped_project(project_id, animation_index, &source_path, import_palette).await {
                                                            Ok((project, mapping)) => {
                                                                width.set(project.width);
                                                                height.set(project.height);
                                                                frame_time_ms.set(project.frame_time_ms);
                                                                project_label.set(mapping.json_path.clone());
                                                                animation_name.set(project.name);
                                                                palette.set(project.palette.clone());
                                                                frames.set(project.frames);
                                                                history.set(History::default());
                                                                selected_frame.set(0);
                                                                playing.set(false);
                                                                preview_open.set(false);
                                                                zoom.set(1.0);
                                                                selection.set(None);
                                                                selection_anchor.set(None);
                                                                selection_drag.set(None);
                                                                clipboard.set(None);
                                                                tool.set(Tool::Pencil);
                                                                mapping_target.set(Some((mapping.json_path.clone(), mapping.animation_index)));
                                                                active_mapping.set(Some(mapping));
                                                                status.set(format!("Opened {project_name} · edits can be saved to the mapped files"));
                                                            }
                                                            Err(error) => status.set(format!("Could not open project: {error}")),
                                                        }
                                                    });
                                                },
                                                    if animation.preview.is_empty() {
                                                        div { class: "project-sidebar-thumbnail empty-thumb", "◇" }
                                                    } else {
                                                        img { class: "project-sidebar-thumbnail", src: "{animation.preview}", alt: "First frame preview" }
                                                    }
                                                    span { class: "project-sidebar-name", "{animation.name}" }
                                                }
                                            }
                                        }
                                    }
                                }
                            }
                        }
                        div { class: "project-sidebar-footer", "{mapped_animation_count} animations" }
                    }
                }
                aside { class: "sidebar",
                    section { class: "side-section palette-section",
                        div { class: "section-heading", span { "Palette" } span { class: "section-hint", "{current_palette.len() - 1} colors" } }
                        div { class: "palette-actions",
                                label { class: "palette-action", title: "Import palette JSON",
                                    "↑ Import"
                                    input {
                                        class: "palette-upload-input",
                                        r#type: "file",
                                        accept: ".json,application/json",
                                        onchange: move |event| {
                                            let Some(file) = event.files().into_iter().next() else { return; };
                                            status.set("Loading palette…".to_owned());
                                            spawn(async move {
                                                match file.read_string().await {
                                                    Ok(contents) => match parse_palette_file(&contents) {
                                                        Ok(new_palette) => {
                                                            let old_palette = palette.read().clone();
                                                            record_document(&mut history, &frames, &width, &height, &frame_time_ms, &palette, &animation_name, &selected_frame);
                                                            frames.with_mut(|all| remap_frames_palette(all, &old_palette, &new_palette));
                                                            let next_selected = (*selected_color.read()).min(new_palette.len() - 1);
                                                            selected_color.set(next_selected);
                                                            palette.set(new_palette);
                                                            status.set("Palette loaded · artwork remapped to the new colors".to_owned());
                                                        }
                                                        Err(error) => status.set(format!("Palette load failed: {error}")),
                                                    },
                                                    Err(error) => status.set(format!("Could not read palette file: {error:?}")),
                                                }
                                            });
                                        }
                                    }
                                }
                                button { class: "palette-action", title: "Download palette JSON", onclick: move |_| {
                                    let palette_file = PaletteFile {
                                        name: Some("cnipix palette".to_owned()),
                                        colors: palette.read().iter().skip(1).cloned().collect(),
                                    };
                                    match serde_json::to_string_pretty(&palette_file) {
                                        Ok(json) => match download_file("cnipix_palette.json", "application/json", json.as_bytes()) {
                                            Ok(()) => status.set("Palette downloaded".to_owned()),
                                            Err(error) => status.set(format!("Palette download failed: {error}")),
                                        },
                                        Err(error) => status.set(format!("Could not create palette JSON: {error}")),
                                    }
                                }, "↓ Export" }
                        }
                        div { class: "palette-grid",
                            for (index, color) in current_palette.iter().enumerate() {
                                button {
                                    key: "swatch-{index}",
                                    class: if index == selected { "swatch selected" } else { "swatch" },
                                    style: if index == 0 { "--swatch: transparent".to_owned() } else { format!("--swatch: {color}") },
                                    title: if index == 0 { "0 · Transparent / erase".to_owned() } else if index <= 9 { format!("{index} · Palette color {index}") } else { format!("Palette color {index}") },
                                    onclick: move |_| selected_color.set(index),
                                    if index == 0 { span { class: "clear-mark", "×" } }
                                }
                            }
                            button {
                                class: "swatch add-color-swatch",
                                title: "Add a palette color",
                                disabled: current_palette.len() >= 256,
                                onclick: move |_| {
                                    if palette.read().len() >= 256 {
                                        status.set("The palette can contain at most 255 opaque colors".to_owned());
                                        return;
                                    }
                                    new_color_value.set(if selected > 0 { palette.read()[selected].clone() } else { "#ffffff".to_owned() });
                                    color_editor_open.set(true);
                                },
                                span { "+" }
                            }
                        }
                        if *color_editor_open.read() {
                            div { class: "palette-color-editor",
                                strong { "Add palette color" }
                                div { class: "palette-color-controls",
                                    input {
                                        class: "native-color-picker",
                                        r#type: "color",
                                        value: "{new_color_value}",
                                        oninput: move |event| new_color_value.set(event.value()),
                                        aria_label: "Choose a color"
                                    }
                                    input {
                                        class: "custom-color-input",
                                        r#type: "text",
                                        value: "{new_color_value}",
                                        placeholder: "#RRGGBB or rgb(…)",
                                        oninput: move |event| new_color_value.set(event.value()),
                                        aria_label: "Color as hex or RGB"
                                    }
                                }
                                div { class: "palette-color-actions",
                                    button { class: "button button-quiet", onclick: move |_| color_editor_open.set(false), "Cancel" }
                                    button { class: "button button-primary", onclick: move |_| {
                                        match parse_color_input(&new_color_value.read()) {
                                            Ok(color) => {
                                                let existing = palette.read().iter().enumerate().skip(1)
                                                    .find(|(_, entry)| entry.eq_ignore_ascii_case(&color))
                                                    .map(|(index, _)| index);
                                                let index = if let Some(index) = existing {
                                                    index
                                                } else if palette.read().len() < 256 {
                                                    let index = palette.read().len();
                                                    palette.with_mut(|colors| colors.push(color.clone()));
                                                    index
                                                } else {
                                                    status.set("The palette can contain at most 255 opaque colors".to_owned());
                                                    return;
                                                };
                                                selected_color.set(index);
                                                color_editor_open.set(false);
                                                status.set(format!("Selected palette color {color}"));
                                            }
                                            Err(error) => status.set(error),
                                        }
                                    }, "Add color" }
                                }
                            }
                        }
                        div { class: "selected-color-info",
                            span { class: "color-chip", style: if selected == 0 { "background: repeating-conic-gradient(#3d4351 0% 25%, #242832 0% 50%) 50% / 10px 10px".to_owned() } else { format!("background:{}", current_palette[selected]) } }
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
                                if old_w > 1 { record_document(&mut history, &frames, &width, &height, &frame_time_ms, &palette, &animation_name, &selected_frame); let new_w = old_w - 1; resize_frames(&mut frames, old_w, *height.read(), new_w, *height.read()); width.set(new_w); }
                            }, "−" }
                            input { class: "number-input", r#type: "number", min: "1", max: "64", value: "{w}", oninput: move |event| {
                                if let Ok(next) = event.value().parse::<usize>() { let next = next.clamp(1, CELL_LIMIT); let old = *width.read(); if old != next { record_document(&mut history, &frames, &width, &height, &frame_time_ms, &palette, &animation_name, &selected_frame); resize_frames(&mut frames, old, *height.read(), next, *height.read()); width.set(next); } }
                            } }
                            button { class: "stepper-button", onclick: move |_| {
                                let old_w = *width.read();
                                if old_w < CELL_LIMIT { record_document(&mut history, &frames, &width, &height, &frame_time_ms, &palette, &animation_name, &selected_frame); let new_w = old_w + 1; resize_frames(&mut frames, old_w, *height.read(), new_w, *height.read()); width.set(new_w); }
                            }, "+" }
                        }
                        label { class: "field-label field-label-spaced", "Height" }
                        div { class: "stepper",
                            button { class: "stepper-button", onclick: move |_| {
                                let old_h = *height.read();
                                if old_h > 1 { record_document(&mut history, &frames, &width, &height, &frame_time_ms, &palette, &animation_name, &selected_frame); let new_h = old_h - 1; resize_frames(&mut frames, *width.read(), old_h, *width.read(), new_h); height.set(new_h); }
                            }, "−" }
                            input { class: "number-input", r#type: "number", min: "1", max: "64", value: "{h}", oninput: move |event| {
                                if let Ok(next) = event.value().parse::<usize>() { let next = next.clamp(1, CELL_LIMIT); let old = *height.read(); if old != next { record_document(&mut history, &frames, &width, &height, &frame_time_ms, &palette, &animation_name, &selected_frame); resize_frames(&mut frames, *width.read(), old, *width.read(), next); height.set(next); } }
                            } }
                            button { class: "stepper-button", onclick: move |_| {
                                let old_h = *height.read();
                                if old_h < CELL_LIMIT { record_document(&mut history, &frames, &width, &height, &frame_time_ms, &palette, &animation_name, &selected_frame); let new_h = old_h + 1; resize_frames(&mut frames, *width.read(), old_h, *width.read(), new_h); height.set(new_h); }
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
                            span { class: "hint-text", "Scroll to zoom · Shift+drag to pan freely" }
                        }
                    }
                    div {
                        class: "canvas-viewport",
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
                                    history,
                                    selected_frame,
                                    selected_color,
                                    palette,
                                    painting,
                                    right_erasing,
                                    tool,
                                    selection,
                                    selection_anchor,
                                    selection_drag,
                                    clipboard,
                                    hovered_pixel,
                                    height: h,
                                    width_state: width,
                                    height_state: height,
                                    frame_time_ms,
                                    animation_name,
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
                            if frames.read().len() > 1 { record_document(&mut history, &frames, &width, &height, &frame_time_ms, &palette, &animation_name, &selected_frame); let remove = *selected_frame.read(); frames.with_mut(|items| { items.remove(remove); }); selected_frame.set(remove.min(frames.read().len() - 1)); }
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
                                    span { key: "thumb-{frame_index}-{pixel_index}", style: if *color_index == 0 { "background: transparent".to_owned() } else { format!("background:{}", current_palette[*color_index as usize]) } }
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
                                        style: if *color_index == 0 { "background: transparent".to_owned() } else { format!("background:{}", current_palette[*color_index as usize]) }
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
                span { class: "status-shortcuts", "Undo / redo: Ctrl+Z / Ctrl+Y · Erase: right click · Tools: B / M / G" }
                span { class: "status-version", "Autosave · this browser" }
            }
        }
    }
}

#[component]
fn FrameNumber(number: usize) -> Element {
    let label = format!("{number:02}");
    rsx! { span { "{label}" } }
}

fn validate_autosave(draft: &AutosaveProject) -> Result<(), ()> {
    if draft.version != 1
        || !(1..=CELL_LIMIT).contains(&draft.width)
        || !(1..=CELL_LIMIT).contains(&draft.height)
        || !(1..=256).contains(&draft.palette.len())
        || draft.frames.is_empty()
        || draft.frames.iter().any(|frame| {
            frame.pixels.len() != draft.width * draft.height
                || frame
                    .pixels
                    .iter()
                    .any(|index| *index as usize >= draft.palette.len())
        })
        || draft
            .palette
            .iter()
            .any(|color| parse_color_input(color).is_err())
    {
        return Err(());
    }
    Ok(())
}

fn document_snapshot(
    frames: &Signal<Vec<Frame>>,
    width: &Signal<usize>,
    height: &Signal<usize>,
    _frame_time_ms: &Signal<u32>,
    palette: &Signal<Vec<String>>,
    _animation_name: &Signal<String>,
    selected_frame: &Signal<usize>,
) -> DocumentSnapshot {
    DocumentSnapshot {
        frames: frames.read().clone(),
        width: *width.read(),
        height: *height.read(),
        palette: palette.read().clone(),
        selected_frame: *selected_frame.read(),
    }
}

fn record_document(
    history: &mut Signal<History>,
    frames: &Signal<Vec<Frame>>,
    width: &Signal<usize>,
    height: &Signal<usize>,
    frame_time_ms: &Signal<u32>,
    palette: &Signal<Vec<String>>,
    animation_name: &Signal<String>,
    selected_frame: &Signal<usize>,
) {
    let snapshot = document_snapshot(
        frames,
        width,
        height,
        frame_time_ms,
        palette,
        animation_name,
        selected_frame,
    );
    history.with_mut(|history| {
        history.undo.push(snapshot);
        if history.undo.len() > HISTORY_LIMIT {
            history.undo.remove(0);
        }
        history.redo.clear();
    });
}

fn undo_document(
    history: &mut Signal<History>,
    frames: &mut Signal<Vec<Frame>>,
    width: &mut Signal<usize>,
    height: &mut Signal<usize>,
    _frame_time_ms: &mut Signal<u32>,
    palette: &mut Signal<Vec<String>>,
    _animation_name: &mut Signal<String>,
    selected_frame: &mut Signal<usize>,
) -> bool {
    let Some(previous) = history.with_mut(|history| history.undo.pop()) else {
        return false;
    };
    let current = document_snapshot(
        frames,
        width,
        height,
        _frame_time_ms,
        palette,
        _animation_name,
        selected_frame,
    );
    history.with_mut(|history| history.redo.push(current));
    restore_document(
        previous,
        frames,
        width,
        height,
        _frame_time_ms,
        palette,
        _animation_name,
        selected_frame,
    );
    true
}

fn redo_document(
    history: &mut Signal<History>,
    frames: &mut Signal<Vec<Frame>>,
    width: &mut Signal<usize>,
    height: &mut Signal<usize>,
    _frame_time_ms: &mut Signal<u32>,
    palette: &mut Signal<Vec<String>>,
    _animation_name: &mut Signal<String>,
    selected_frame: &mut Signal<usize>,
) -> bool {
    let Some(next) = history.with_mut(|history| history.redo.pop()) else {
        return false;
    };
    let current = document_snapshot(
        frames,
        width,
        height,
        _frame_time_ms,
        palette,
        _animation_name,
        selected_frame,
    );
    history.with_mut(|history| {
        history.undo.push(current);
        if history.undo.len() > HISTORY_LIMIT {
            history.undo.remove(0);
        }
    });
    restore_document(
        next,
        frames,
        width,
        height,
        _frame_time_ms,
        palette,
        _animation_name,
        selected_frame,
    );
    true
}

fn restore_document(
    snapshot: DocumentSnapshot,
    frames: &mut Signal<Vec<Frame>>,
    width: &mut Signal<usize>,
    height: &mut Signal<usize>,
    _frame_time_ms: &mut Signal<u32>,
    palette: &mut Signal<Vec<String>>,
    _animation_name: &mut Signal<String>,
    selected_frame: &mut Signal<usize>,
) {
    frames.set(snapshot.frames);
    width.set(snapshot.width);
    height.set(snapshot.height);
    palette.set(snapshot.palette);
    selected_frame.set(snapshot.selected_frame);
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

async fn load_project_files(
    files: Vec<dioxus::html::FileData>,
    palette: Vec<String>,
) -> Result<LoadedProject, String> {
    let mut manifest_file: Option<(String, String)> = None;
    let mut spritesheets = Vec::new();
    for file in files {
        let name = file.name();
        match name
            .rsplit('.')
            .next()
            .unwrap_or_default()
            .to_ascii_lowercase()
            .as_str()
        {
            "json" => {
                let contents = file
                    .read_string()
                    .await
                    .map_err(|error| format!("could not read {name}: {error:?}"))?;
                manifest_file = Some((name, contents));
            }
            "png" => {
                let contents = file
                    .read_bytes()
                    .await
                    .map_err(|error| format!("could not read {name}: {error:?}"))?;
                spritesheets.push((name, contents.to_vec()));
            }
            _ => {}
        }
    }
    let (_, manifest_text) = manifest_file.ok_or("select the animation JSON file too")?;
    let manifest: ProjectManifest = serde_json::from_str(&manifest_text)
        .map_err(|error| format!("invalid animation JSON: {error}"))?;
    let expected_sheet = manifest
        .spritesheet
        .rsplit(|character| character == '/' || character == '\\')
        .next()
        .unwrap_or(&manifest.spritesheet);
    let (_, sheet_bytes) = spritesheets
        .into_iter()
        .find(|(name, _)| name.eq_ignore_ascii_case(expected_sheet))
        .ok_or_else(|| format!("select the PNG named {expected_sheet}"))?;
    load_project(&manifest, &sheet_bytes, &palette)
}

async fn map_project_directory(promise: js_sys::Promise) -> Result<Vec<MappedProject>, String> {
    use wasm_bindgen_futures::JsFuture;
    let result = JsFuture::from(promise).await.map_err(js_error_message)?;
    let json = result
        .as_string()
        .ok_or("folder scan returned invalid data")?;
    serde_json::from_str(&json).map_err(|error| format!("could not read folder index: {error}"))
}

async fn restore_project_directory(promise: js_sys::Promise) -> Result<DirectoryRestore, String> {
    use wasm_bindgen_futures::JsFuture;
    let result = JsFuture::from(promise).await.map_err(js_error_message)?;
    let value = result
        .as_string()
        .ok_or("saved folder restore returned invalid data")?;
    match value.as_str() {
        "" => Ok(DirectoryRestore::NoSavedFolder),
        "__CNIPIX_PERMISSION_REQUIRED__" => Ok(DirectoryRestore::PermissionRequired),
        json => serde_json::from_str(json)
            .map(DirectoryRestore::Mapped)
            .map_err(|error| format!("could not read saved folder index: {error}")),
    }
}

async fn open_mapped_project(
    id: u32,
    animation_index: usize,
    path: &str,
    palette: Vec<String>,
) -> Result<(LoadedProject, ActiveMapping), String> {
    use wasm_bindgen_futures::JsFuture;
    let result =
        JsFuture::from(cnipixReadProject(id, animation_index as u32).map_err(js_error_message)?)
            .await
            .map_err(js_error_message)?;
    let manifest_text = js_sys::Reflect::get(&result, &"manifest".into())
        .map_err(js_error_message)?
        .as_string()
        .ok_or("could not read the mapped JSON")?;
    let png_value = js_sys::Reflect::get(&result, &"png".into()).map_err(js_error_message)?;
    let png = js_sys::Uint8Array::new(&png_value).to_vec();
    let manifest: ProjectManifest = serde_json::from_str(&manifest_text)
        .map_err(|error| format!("invalid animation JSON: {error}"))?;
    let animation = manifest
        .anims
        .get(animation_index)
        .ok_or("the requested animation does not exist")?;
    let mapping = ActiveMapping {
        id,
        animation_index,
        json_path: path.to_owned(),
        manifest: manifest_text,
        png: png.clone(),
        original_frame_indices: animation.frames.clone(),
        original_width: manifest.width,
        original_height: manifest.height,
    };
    let project = load_project_animation(&manifest, &png, &palette, animation_index)?;
    Ok((project, mapping))
}

async fn reopen_saved_mapping(
    projects: &[MappedProject],
    target: Option<(String, usize)>,
    palette: Vec<String>,
) -> Result<Option<ActiveMapping>, String> {
    let Some((path, animation_index)) = target else {
        return Ok(None);
    };
    let Some(project) = projects.iter().find(|project| project.path == path) else {
        return Ok(None);
    };
    let Some(animation) = project
        .animations
        .iter()
        .find(|animation| animation.index == animation_index)
    else {
        return Ok(None);
    };
    let (_, mapping) =
        open_mapped_project(project.id, animation.index, &project.path, palette).await?;
    Ok(Some(mapping))
}

async fn write_mapped_project(id: u32, manifest: &str, png: &[u8]) -> Result<(), String> {
    use wasm_bindgen_futures::JsFuture;
    let bytes = js_sys::Uint8Array::from(png);
    JsFuture::from(cnipixWriteProject(id, manifest, &bytes).map_err(js_error_message)?)
        .await
        .map_err(js_error_message)?;
    Ok(())
}

fn js_error_message(error: wasm_bindgen::JsValue) -> String {
    error.as_string().unwrap_or_else(|| format!("{error:?}"))
}

fn mapped_frame_indices(manifest: &str, animation_index: usize) -> Vec<usize> {
    serde_json::from_str::<serde_json::Value>(manifest)
        .ok()
        .and_then(|value| {
            value
                .get("anims")?
                .get(animation_index)?
                .get("frames")?
                .as_array()
                .cloned()
        })
        .unwrap_or_default()
        .into_iter()
        .filter_map(|value| value.as_u64().map(|index| index as usize))
        .collect()
}

fn prepare_mapped_save(
    mapping: &ActiveMapping,
    frames: &[Frame],
    width: usize,
    height: usize,
    frame_time_ms: u32,
    animation_name: &str,
    palette: &[String],
) -> Result<(String, Vec<u8>), String> {
    if width != mapping.original_width || height != mapping.original_height {
        return Err(
            "changing the cell width or height for a mapped sprite sheet is not supported yet"
                .to_owned(),
        );
    }
    if frames.is_empty() {
        return Err("an animation must have at least one frame".to_owned());
    }
    let mut manifest: serde_json::Value = serde_json::from_str(&mapping.manifest)
        .map_err(|error| format!("invalid animation JSON: {error}"))?;
    let anim = manifest
        .get_mut("anims")
        .and_then(serde_json::Value::as_array_mut)
        .and_then(|anims| anims.get_mut(mapping.animation_index))
        .ok_or("the mapped animation no longer exists in the JSON")?;
    let mut decoder = png::Decoder::new(Cursor::new(&mapping.png));
    decoder.set_transformations(png::Transformations::EXPAND | png::Transformations::STRIP_16);
    let mut reader = decoder
        .read_info()
        .map_err(|error| format!("could not decode mapped sprite sheet: {error}"))?;
    let mut buffer = vec![0; reader.output_buffer_size()];
    let info = reader
        .next_frame(&mut buffer)
        .map_err(|error| format!("could not read mapped sprite sheet: {error}"))?;
    let sheet_width = info.width as usize;
    let sheet_height = info.height as usize;
    if sheet_width % width != 0 || sheet_height % height != 0 {
        return Err("mapped sprite sheet dimensions no longer match the cell size".to_owned());
    }
    let source = png_to_rgba(&buffer[..info.buffer_size()], info.color_type)?;
    let columns = sheet_width / width;
    let cell_count = (sheet_width / width) * (sheet_height / height);
    if mapping
        .original_frame_indices
        .iter()
        .any(|&index| index >= cell_count)
    {
        return Err("the mapped animation refers to a frame outside its sprite sheet".to_owned());
    }
    let mut frame_indices = Vec::with_capacity(frames.len());
    for frame in 0..frames.len() {
        frame_indices.push(
            if let Some(&index) = mapping.original_frame_indices.get(frame) {
                index
            } else {
                cell_count + frame - mapping.original_frame_indices.len()
            },
        );
    }
    let required_rows = frame_indices
        .iter()
        .map(|index| index / columns + 1)
        .max()
        .unwrap_or(1);
    let out_height = required_rows.max(sheet_height / height) * height;
    let mut rgba = vec![0; sheet_width * out_height * 4];
    rgba[..source.len()].copy_from_slice(&source);
    for (frame, &frame_index) in frames.iter().zip(frame_indices.iter()) {
        let origin_x = (frame_index % columns) * width;
        let origin_y = (frame_index / columns) * height;
        for y in 0..height {
            for x in 0..width {
                let color_index = frame.pixels[y * width + x] as usize;
                let offset = ((origin_y + y) * sheet_width + origin_x + x) * 4;
                if color_index == 0 {
                    rgba[offset..offset + 4].fill(0);
                } else {
                    let hex = u32::from_str_radix(palette[color_index].trim_start_matches('#'), 16)
                        .map_err(|error| format!("invalid palette color: {error}"))?;
                    rgba[offset] = ((hex >> 16) & 0xff) as u8;
                    rgba[offset + 1] = ((hex >> 8) & 0xff) as u8;
                    rgba[offset + 2] = (hex & 0xff) as u8;
                    rgba[offset + 3] = 255;
                }
            }
        }
    }
    anim["name"] = serde_json::Value::String(if animation_name.trim().is_empty() {
        "CNIPIX_ANIMATION".to_owned()
    } else {
        animation_name.trim().to_owned()
    });
    anim["frametime"] = serde_json::Value::from(frame_time_ms as f64 / 1000.0);
    anim["frames"] = serde_json::Value::Array(
        frame_indices
            .iter()
            .map(|index| serde_json::Value::from(*index))
            .collect(),
    );
    let mut png = Vec::new();
    {
        let mut encoder = png::Encoder::new(&mut png, sheet_width as u32, out_height as u32);
        encoder.set_color(png::ColorType::Rgba);
        encoder.set_depth(png::BitDepth::Eight);
        let mut writer = encoder.write_header().map_err(|error| error.to_string())?;
        writer
            .write_image_data(&rgba)
            .map_err(|error| error.to_string())?;
    }
    let json = serde_json::to_string_pretty(&manifest).map_err(|error| error.to_string())?;
    Ok((json, png))
}

fn load_project(
    manifest: &ProjectManifest,
    png_bytes: &[u8],
    palette: &[String],
) -> Result<LoadedProject, String> {
    load_project_animation(manifest, png_bytes, palette, 0)
}

fn load_project_animation(
    manifest: &ProjectManifest,
    png_bytes: &[u8],
    palette: &[String],
    animation_index: usize,
) -> Result<LoadedProject, String> {
    if manifest.width == 0
        || manifest.height == 0
        || manifest.width > CELL_LIMIT
        || manifest.height > CELL_LIMIT
    {
        return Err(format!(
            "cell dimensions must be between 1 and {CELL_LIMIT} pixels"
        ));
    }
    let animation = manifest
        .anims
        .get(animation_index)
        .ok_or("the requested animation does not exist")?;
    if animation.frames.is_empty() {
        return Err("this animation does not contain any frames".to_owned());
    }

    let mut decoder = png::Decoder::new(Cursor::new(png_bytes));
    decoder.set_transformations(png::Transformations::EXPAND | png::Transformations::STRIP_16);
    let mut reader = decoder
        .read_info()
        .map_err(|error| format!("could not decode sprite sheet: {error}"))?;
    let mut buffer = vec![0; reader.output_buffer_size()];
    let info = reader
        .next_frame(&mut buffer)
        .map_err(|error| format!("could not read sprite sheet pixels: {error}"))?;
    let sheet_width = info.width as usize;
    let sheet_height = info.height as usize;
    if sheet_width == 0
        || sheet_height == 0
        || sheet_width
            .checked_mul(sheet_height)
            .is_none_or(|area| area > 16_777_216)
    {
        return Err("sprite sheet dimensions are empty or too large".to_owned());
    }
    if sheet_width % manifest.width != 0 || sheet_height % manifest.height != 0 {
        return Err("sprite sheet dimensions are not a multiple of the JSON cell size".to_owned());
    }
    let pixels = png_to_rgba(&buffer[..info.buffer_size()], info.color_type)?;
    let columns = sheet_width / manifest.width;
    let rows = sheet_height / manifest.height;
    let cell_count = columns * rows;
    let mut project_palette = palette.to_vec();
    if project_palette.is_empty() {
        project_palette.push("#171923".to_owned());
    }
    let mut color_indices = HashMap::new();
    for (index, color) in project_palette.iter().enumerate().skip(1) {
        if let Ok(rgb) = u32::from_str_radix(color.trim_start_matches('#'), 16) {
            color_indices.entry(rgb).or_insert(index as u8);
        }
    }
    let mut frames = Vec::with_capacity(animation.frames.len());
    for &frame_index in &animation.frames {
        if frame_index >= cell_count {
            return Err(format!(
                "animation references frame {frame_index}, but the sprite sheet has only {cell_count} cells"
            ));
        }
        let origin_x = (frame_index % columns) * manifest.width;
        let origin_y = (frame_index / columns) * manifest.height;
        let mut frame_pixels = Vec::with_capacity(manifest.width * manifest.height);
        for y in 0..manifest.height {
            for x in 0..manifest.width {
                let offset = ((origin_y + y) * sheet_width + origin_x + x) * 4;
                let rgba = &pixels[offset..offset + 4];
                if rgba[3] < 128 {
                    frame_pixels.push(0);
                } else {
                    let rgb = ((rgba[0] as u32) << 16) | ((rgba[1] as u32) << 8) | rgba[2] as u32;
                    let index = if let Some(index) = color_indices.get(&rgb) {
                        *index
                    } else {
                        if project_palette.len() >= 256 {
                            return Err("this animation uses more than 255 opaque colors; simplify its palette before importing".to_owned());
                        }
                        let index = project_palette.len() as u8;
                        project_palette.push(format!("#{rgb:06x}"));
                        color_indices.insert(rgb, index);
                        index
                    };
                    frame_pixels.push(index);
                }
            }
        }
        frames.push(Frame {
            pixels: frame_pixels,
        });
    }
    let frame_time_ms = (animation.frametime.clamp(0.01, 2.0) * 1000.0).round() as u32;
    Ok(LoadedProject {
        width: manifest.width,
        height: manifest.height,
        name: animation.name.clone(),
        frame_time_ms,
        frames,
        animation_count: manifest.anims.len(),
        palette: project_palette,
    })
}

fn png_to_rgba(data: &[u8], color_type: png::ColorType) -> Result<Vec<u8>, String> {
    let mut rgba = Vec::new();
    match color_type {
        png::ColorType::Rgba => Ok(data.to_vec()),
        png::ColorType::Rgb => {
            rgba.reserve(data.len() / 3 * 4);
            for pixel in data.chunks_exact(3) {
                rgba.extend_from_slice(&[pixel[0], pixel[1], pixel[2], 255]);
            }
            Ok(rgba)
        }
        png::ColorType::Grayscale => {
            rgba.reserve(data.len() * 4);
            for &gray in data {
                rgba.extend_from_slice(&[gray, gray, gray, 255]);
            }
            Ok(rgba)
        }
        png::ColorType::GrayscaleAlpha => {
            rgba.reserve(data.len() * 2);
            for pixel in data.chunks_exact(2) {
                rgba.extend_from_slice(&[pixel[0], pixel[0], pixel[0], pixel[1]]);
            }
            Ok(rgba)
        }
        png::ColorType::Indexed => Err("could not expand indexed sprite sheet colors".to_owned()),
    }
}

fn nearest_palette_color(red: u8, green: u8, blue: u8, palette: &[String]) -> u8 {
    let mut nearest = (u32::MAX, 1u8);
    for (index, color) in palette.iter().enumerate().skip(1) {
        let rgb = u32::from_str_radix(color.trim_start_matches('#'), 16).unwrap_or(0);
        let dr = red as i32 - ((rgb >> 16) & 0xff) as i32;
        let dg = green as i32 - ((rgb >> 8) & 0xff) as i32;
        let db = blue as i32 - (rgb & 0xff) as i32;
        let distance = (dr * dr + dg * dg + db * db) as u32;
        if distance < nearest.0 {
            nearest = (distance, index as u8);
        }
    }
    nearest.1
}

fn parse_color_input(input: &str) -> Result<String, String> {
    let value = input.trim().to_ascii_lowercase();
    if let Some(hex) = value.strip_prefix('#') {
        if !hex.chars().all(|digit| digit.is_ascii_hexdigit()) {
            return Err("hex colors may contain only digits 0–9 and letters A–F".to_owned());
        }
        let rgb = match hex.len() {
            3 => hex
                .chars()
                .map(|digit| format!("{digit}{digit}"))
                .collect::<String>(),
            4 if hex.ends_with('f') => hex[..3]
                .chars()
                .map(|digit| format!("{digit}{digit}"))
                .collect::<String>(),
            6 => hex.to_owned(),
            8 if hex.ends_with("ff") => hex[..6].to_owned(),
            4 | 8 => return Err("palette colors must be fully opaque".to_owned()),
            _ => return Err("use #RGB, #RRGGBB, or an rgb() color".to_owned()),
        };
        if u32::from_str_radix(&rgb, 16).is_err() {
            return Err("hex colors may contain only digits 0–9 and letters A–F".to_owned());
        }
        return Ok(format!("#{rgb}"));
    }

    let open = value
        .find('(')
        .ok_or("use #RGB, #RRGGBB, rgb(), or opaque rgba()")?;
    let function = value[..open].trim();
    if function != "rgb" && function != "rgba" {
        return Err("use #RGB, #RRGGBB, rgb(), or opaque rgba()".to_owned());
    }
    let body = value[open + 1..]
        .strip_suffix(')')
        .ok_or("RGB color is missing its closing parenthesis")?;
    let channels = body
        .replace(',', " ")
        .replace('/', " ")
        .split_whitespace()
        .map(str::to_owned)
        .collect::<Vec<_>>();
    if channels.len() != 3 && channels.len() != 4 {
        return Err("rgb() needs three channels; rgba() may include an alpha of 1".to_owned());
    }
    if function == "rgba" && channels.len() != 4 {
        return Err("rgba() needs an alpha channel".to_owned());
    }
    if (function == "rgba" || channels.len() == 4)
        && !matches!(channels[3].as_str(), "1" | "1.0" | "1.00" | "100%")
    {
        return Err("palette colors must be fully opaque (alpha 1)".to_owned());
    }
    let mut rgb = String::with_capacity(6);
    for channel in channels.iter().take(3) {
        let number = if let Some(percent) = channel.strip_suffix('%') {
            let percent = percent
                .parse::<f32>()
                .map_err(|_| "RGB percentages must be between 0% and 100%")?;
            if !(0.0..=100.0).contains(&percent) {
                return Err("RGB percentages must be between 0% and 100%".to_owned());
            }
            (percent * 255.0 / 100.0).round() as u8
        } else {
            let number = channel
                .parse::<u16>()
                .map_err(|_| "RGB channels must be integers from 0 to 255")?;
            u8::try_from(number).map_err(|_| "RGB channels must be integers from 0 to 255")?
        };
        write!(rgb, "{number:02x}").expect("writing to a String cannot fail");
    }
    Ok(format!("#{rgb}"))
}

fn parse_palette_file(contents: &str) -> Result<Vec<String>, String> {
    let document: PaletteDocument =
        serde_json::from_str(contents).map_err(|error| format!("invalid palette JSON: {error}"))?;
    let entries = match document {
        PaletteDocument::Named(file) => file.colors,
        PaletteDocument::Colors(colors) => colors,
    };
    if entries.is_empty() || entries.len() > 15 {
        return Err("a palette must contain between 1 and 15 colors".to_owned());
    }
    let mut palette = vec!["#171923".to_owned()];
    for color in entries {
        let hex = color.trim().trim_start_matches('#');
        if hex.len() != 6 || !hex.chars().all(|character| character.is_ascii_hexdigit()) {
            return Err(format!("{color:?} is not a six digit hex color"));
        }
        palette.push(format!("#{hex}").to_ascii_lowercase());
    }
    Ok(palette)
}

fn remap_frames_palette(frames: &mut [Frame], old_palette: &[String], new_palette: &[String]) {
    for frame in frames {
        for color_index in &mut frame.pixels {
            if *color_index == 0 {
                continue;
            }
            let Some(color) = old_palette.get(*color_index as usize) else {
                *color_index = 0;
                continue;
            };
            let Ok(rgb) = u32::from_str_radix(color.trim_start_matches('#'), 16) else {
                *color_index = 0;
                continue;
            };
            *color_index = nearest_palette_color(
                ((rgb >> 16) & 0xff) as u8,
                ((rgb >> 8) & 0xff) as u8,
                (rgb & 0xff) as u8,
                new_palette,
            );
        }
    }
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
    mut history: Signal<History>,
    selected_frame: Signal<usize>,
    selected_color: Signal<usize>,
    palette: Signal<Vec<String>>,
    mut painting: Signal<bool>,
    mut right_erasing: Signal<bool>,
    mut tool: Signal<Tool>,
    mut selection: Signal<Option<Selection>>,
    mut selection_anchor: Signal<Option<(usize, usize)>>,
    mut selection_drag: Signal<Option<SelectionDrag>>,
    mut clipboard: Signal<Option<Clipboard>>,
    mut hovered_pixel: Signal<Option<usize>>,
    width_state: Signal<usize>,
    height_state: Signal<usize>,
    frame_time_ms: Signal<u32>,
    animation_name: Signal<String>,
) -> Element {
    let colors = palette.read();
    let is_current = pixel > 0;
    let show_ghost = onion && !is_current && ghost > 0;
    let background = if is_current {
        colors[pixel as usize].to_owned()
    } else if show_ghost {
        ghost_color(&colors[ghost as usize])
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
                    .is_some_and(|pointer| pointer.shift_key())
                {
                    event.prevent_default();
                    return;
                }
                if event
                    .data()
                    .downcast::<web_sys::PointerEvent>()
                    .is_some_and(|pointer| pointer.button() == 2)
                {
                    event.prevent_default();
                    record_document(&mut history, &frames, &width_state, &height_state, &frame_time_ms, &palette, &animation_name, &selected_frame);
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
                        record_document(&mut history, &frames, &width_state, &height_state, &frame_time_ms, &palette, &animation_name, &selected_frame);
                        painting.set(true);
                        let color = *selected_color.read() as u8;
                        frames.with_mut(|all| all[frame].pixels[index] = color);
                    }
                    Tool::Fill => {
                        let color = *selected_color.read() as u8;
                        if frames.read()[frame].pixels[index] != color {
                            record_document(&mut history, &frames, &width_state, &height_state, &frame_time_ms, &palette, &animation_name, &selected_frame);
                        }
                        flood_fill(&mut frames, frame, width, height, index, color);
                        tool.set(Tool::Pencil);
                    }
                    Tool::Select => {
                        let current_selection = *selection.read();
                        if let Some(region) = current_selection {
                            if x >= region.x && x < region.x + region.width && y >= region.y && y < region.y + region.height {
                                record_document(&mut history, &frames, &width_state, &height_state, &frame_time_ms, &palette, &animation_name, &selected_frame);
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
                            record_document(&mut history, &frames, &width_state, &height_state, &frame_time_ms, &palette, &animation_name, &selected_frame);
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

fn encode_spritesheet(
    frames: &[Frame],
    width: usize,
    height: usize,
    palette: &[String],
) -> Result<Vec<u8>, String> {
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
                    let hex = u32::from_str_radix(palette[color_index].trim_start_matches('#'), 16)
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

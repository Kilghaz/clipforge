//! Renders the UI offscreen into PNGs for the visual pass in
//! `docs/ux/DESIGN.md` §5. No window, no OS screen-capture permission.
//!
//! ```text
//! cargo run -p clipforge-app --bin screenshot -- [scene ...] [--out DIR]
//! ```
//!
//! Scenes: `empty`, `populated`, `export`, `settings`, `narrow` (900 × 560)
//! and `gallery` (every component in every state, at 2× scale). Default is
//! all of them, written to `target/screenshots/<scene>.png`.

// Slint-generated code contains `unsafe`; the workspace-wide `deny` still
// applies to the hand-written code in this file.
#![allow(clippy::print_stdout, clippy::print_stderr)]

/// Generated Slint bindings; lints relaxed only for generated code.
mod ui {
    #![allow(
        missing_debug_implementations,
        unreachable_pub,
        unused,
        clippy::all,
        clippy::pedantic,
        clippy::unwrap_used,
        clippy::expect_used,
        clippy::panic,
        clippy::todo,
        clippy::unimplemented,
        clippy::dbg_macro
    )]
    slint::include_modules!();
}

use std::path::{Path, PathBuf};
use std::rc::Rc;

use anyhow::{Context, Result, bail};
use slint::Rgb8Pixel;
use slint::platform::software_renderer::{MinimalSoftwareWindow, RepaintBufferType};
use slint::platform::{Platform, PlatformError, WindowAdapter, WindowEvent};
use slint::{ComponentHandle, ModelRc, PhysicalSize, VecModel};

use ui::{
    EditorState, GalleryWindow, GridRow, InspectorInfo, LibraryState, MainWindow, MediaCell, Shell,
    TimelineClip,
};

const SCENES: &[&str] = &[
    "empty",
    "populated",
    "export",
    "settings",
    "narrow",
    "gallery",
];

struct Headless {
    window: Rc<MinimalSoftwareWindow>,
}

impl Platform for Headless {
    fn create_window_adapter(&self) -> Result<Rc<dyn WindowAdapter>, PlatformError> {
        Ok(self.window.clone())
    }
}

fn main() -> Result<()> {
    let mut scenes = Vec::new();
    let mut out = PathBuf::from("target/screenshots");
    let mut args = std::env::args().skip(1);
    while let Some(a) = args.next() {
        if a == "--out" {
            out = PathBuf::from(args.next().context("--out needs a directory")?);
        } else if SCENES.contains(&a.as_str()) {
            scenes.push(a);
        } else {
            bail!("unknown scene {a:?}; known: {}", SCENES.join(", "));
        }
    }
    if scenes.is_empty() {
        scenes = SCENES.iter().map(|s| (*s).to_owned()).collect();
    }
    std::fs::create_dir_all(&out)?;

    let window = MinimalSoftwareWindow::new(RepaintBufferType::NewBuffer);
    slint::platform::set_platform(Box::new(Headless {
        window: window.clone(),
    }))
    .map_err(|e| anyhow::anyhow!("{e}"))?;

    let fixtures = fixtures_dir()?;
    for scene in scenes {
        let path = out.join(format!("{scene}.png"));
        if scene == "gallery" {
            // 2× so 1 px details (borders, centring) are visible.
            let (w, h, scale) = (1480, 1760, 2.0);
            window.dispatch_event(WindowEvent::ScaleFactorChanged {
                scale_factor: scale,
            });
            window.set_size(PhysicalSize::new(w * 2, h * 2));
            let gallery = GalleryWindow::new()?;
            gallery.set_sample(load(&fixtures, "photo_landscape.jpg")?);
            gallery.show()?;
            render(&window, w * 2, h * 2, &path)?;
            gallery.hide()?;
            window.dispatch_event(WindowEvent::ScaleFactorChanged { scale_factor: 1.0 });
            continue;
        }
        let (w, h) = if scene == "narrow" {
            (900, 560)
        } else {
            (1400, 860)
        };
        window.set_size(PhysicalSize::new(w, h));
        let app = MainWindow::new()?;
        populate(&app, &scene, &fixtures)?;
        app.show()?;
        render(&window, w, h, &path)?;
        app.hide()?;
    }
    Ok(())
}

/// Draws a few frames (layout first, then bindings that depend on layout
/// such as grid width and strip geometry) and writes the PNG.
fn render(window: &Rc<MinimalSoftwareWindow>, w: u32, h: u32, path: &Path) -> Result<()> {
    let mut pixels = vec![Rgb8Pixel::default(); (w * h) as usize];
    for _ in 0..3 {
        slint::platform::update_timers_and_animations();
        window.draw_if_needed(|renderer| {
            renderer.render(&mut pixels, w as usize);
        });
    }
    save_png(path, w, h, &pixels)?;
    println!("{}", path.display());
    Ok(())
}

fn fixtures_dir() -> Result<PathBuf> {
    let here = Path::new(env!("CARGO_MANIFEST_DIR"));
    let dir = here.join("../../fixtures");
    if !dir.is_dir() {
        bail!("fixtures directory not found at {}", dir.display());
    }
    Ok(dir)
}

fn load(fixtures: &Path, name: &str) -> Result<slint::Image> {
    slint::Image::load_from_path(&fixtures.join(name))
        .map_err(|e| anyhow::anyhow!("cannot load {name}: {e:?}"))
}

fn populate(app: &MainWindow, scene: &str, fixtures: &Path) -> Result<()> {
    app.set_app_version("0.1.0".into());
    app.set_language_options(ModelRc::new(VecModel::from(vec![
        slint::SharedString::from("System language"),
        "English".into(),
        "Deutsch".into(),
    ])));
    app.global::<Shell>().set_macos(false);
    let editor = app.global::<EditorState>();
    editor.set_project_title("Summer holiday".into());
    editor.set_time_text("00:00:00".into());
    editor.set_total_text("00:00:00".into());

    if scene == "empty" {
        return Ok(());
    }

    let landscape = load(fixtures, "photo_landscape.jpg")?;
    let portrait = load(fixtures, "photo_portrait.png")?;
    let library = app.global::<LibraryState>();
    let mut rows = Vec::new();
    let mut index = 0;
    for r in 0..4 {
        let mut cells = Vec::new();
        for c in 0..3 {
            let kind = match (r + c) % 5 {
                0..=2 => 0,
                3 => 1,
                _ => 2,
            };
            let has_image = kind != 2 && (r + c) % 7 != 6;
            cells.push(MediaCell {
                index,
                id: format!("id-{index}").into(),
                title: match kind {
                    0 => format!("IMG_{:04}.jpg", 4021 + index),
                    1 => format!("Clip {}.mov", index),
                    _ => "Soundtrack.m4a".to_owned(),
                }
                .into(),
                subtitle: match kind {
                    0 => "12 Jul 2026".into(),
                    1 => "0:14 · 4K".into(),
                    _ => "3:42".into(),
                },
                image: if c % 2 == 0 {
                    landscape.clone()
                } else {
                    portrait.clone()
                },
                has_image,
                kind,
                cloud: index == 5,
                failed: index == 10,
                pending: index == 11,
                selected: index == 1 || index == 4,
            });
            index += 1;
        }
        rows.push(GridRow {
            cells: ModelRc::new(VecModel::from(cells)),
        });
    }
    library.set_rows(ModelRc::new(VecModel::from(rows)));
    library.set_total_count(index);
    library.set_selected_count(2);
    library.set_status_kind(3);
    library.set_import_added(12);
    library.set_import_existing(3);
    library.set_inspector(InspectorInfo {
        visible: true,
        title: "IMG_4022.jpg".into(),
        kind: 0,
        dimensions: "6000 × 4000".into(),
        duration: "".into(),
        captured: "12 Jul 2026, 14:03".into(),
        codec: "".into(),
        size: "8.4 MB".into(),
        path: "/Users/you/Pictures/Summer/IMG_4022.jpg".into(),
        status_kind: 1,
        error: "".into(),
        hdr: true,
    });
    library.set_inspector_image(landscape.clone());
    library.set_inspector_has_image(true);

    let mut clips = Vec::new();
    let mut x = 8.0;
    for i in 0..6 {
        let is_video = i == 2 || i == 4;
        let width = if is_video { 240.0 } else { 160.0 };
        clips.push(TimelineClip {
            index: i,
            id: format!("clip-{i}").into(),
            title: if is_video {
                format!("Clip {i}.mov").into()
            } else {
                format!("IMG_{:04}.jpg", 4021 + i).into()
            },
            thumb: if i % 2 == 0 {
                landscape.clone()
            } else {
                portrait.clone()
            },
            has_thumb: true,
            x: x as f32,
            width: width as f32,
            overlap: if i > 0 { 40.0 } else { 0.0 },
            selected: i == 1,
            duration_text: if is_video {
                "0:06".into()
            } else {
                "4.0 s".into()
            },
            transition: if i == 0 { 0 } else { 1 },
            is_video,
            muted: i == 4,
        });
        x += width - 40.0 + 4.0;
    }
    editor.set_clips(ModelRc::new(VecModel::from(clips)));
    editor.set_clip_count(6);
    editor.set_selected_count(1);
    editor.set_strip_width(x as f32);
    editor.set_playhead_x(300.0);
    editor.set_time_text("00:00:07".into());
    editor.set_total_text("00:00:26".into());
    editor.set_preview(landscape);
    editor.set_can_undo(true);
    editor.set_dirty(true);

    match scene {
        "export" => {
            editor.set_export_open(true);
            editor.set_export_status(1);
            editor.set_export_progress(0.42);
        }
        "settings" => app.global::<Shell>().set_settings_open(true),
        _ => {}
    }
    Ok(())
}

fn save_png(path: &Path, w: u32, h: u32, pixels: &[Rgb8Pixel]) -> Result<()> {
    let mut bytes = Vec::with_capacity(pixels.len() * 3);
    for p in pixels {
        bytes.extend_from_slice(&[p.r, p.g, p.b]);
    }
    let img: image::RgbImage =
        image::ImageBuffer::from_raw(w, h, bytes).context("buffer size mismatch")?;
    img.save(path)
        .with_context(|| format!("writing {}", path.display()))
}

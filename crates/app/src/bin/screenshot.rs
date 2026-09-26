//! Renders the UI offscreen into PNGs for the visual pass in
//! `docs/ux/DESIGN.md` §5. No window, no OS screen-capture permission.
//!
//! ```text
//! cargo run -p clipforge-app --bin screenshot -- [scene ...] [--out DIR]
//! ```
//!
//! Scenes: `empty`, `populated`, the export window (`export` with Advanced
//! open, `export-running`, `export-done`, `export-small` scrolling),
//! `export-background` (toolbar progress), `settings`, `narrow` (900 × 560)
//! and `gallery` (every component in every state, at 2× scale). Default is
//! all of them, written to `target/screenshots/<scene>.png`.

// Slint-generated code contains `unsafe`; the workspace-wide `deny` still
// applies to the hand-written code in this file.
#![allow(clippy::print_stdout, clippy::print_stderr)]

/// Generated Slint bindings; lints relaxed only for generated code.
mod ui {
    #![allow(
        // Generated with element debug info (debug builds) it contains
        // `todo!()` in paths the app never takes.
        clippy::todo,
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
    EditorState, ExportState, ExportWindow, GalleryWindow, GridRow, InspectorInfo, LibraryState,
    MainWindow, MediaCell, PreviewText, Shell, SongBlockView, SongItem, Strings, TextBlockView,
    TimelineClip,
};

const SCENES: &[&str] = &[
    "empty",
    "populated",
    "export",
    "export-running",
    "export-done",
    "export-background",
    "settings",
    "narrow",
    "export-small",
    "export-hevc",
    "music",
    "title",
    "text",
    "text-edit",
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
            let (w, h, scale) = (1480, 2400, 2.0);
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
        if EXPORT_SCENES.contains(&scene.as_str()) {
            // The export window on its own, sized to its content as the
            // app does (320..760 px); `export-small` shows the scrolling.
            let export = ExportWindow::new()?;
            populate_export(&export, &scene);
            export.show()?;
            let w = 520;
            window.set_size(PhysicalSize::new(w, 760));
            render(&window, w, 760, &path)?;
            #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
            let content = export.get_content_height().clamp(320.0, 760.0) as u32;
            let h = if scene == "export-small" {
                420
            } else {
                content
            };
            window.set_size(PhysicalSize::new(w, h));
            render(&window, w, h, &path)?;
            export.hide()?;
            continue;
        }
        let (w, h) = if scene.starts_with("narrow") {
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
const EXPORT_SCENES: &[&str] = &[
    "export",
    "export-running",
    "export-done",
    "export-small",
    "export-hevc",
];

fn populate_export(export: &ExportWindow, scene: &str) {
    export.global::<Shell>().set_macos(false);
    let s = export.global::<ExportState>();
    s.set_export_size_text("153 MB".into());
    s.set_export_codec_name("HEVC".into());
    s.set_export_video_mbps(8.0);
    s.set_export_auto_video_mbps(8.0);
    s.set_export_hdr_availability(0);
    s.set_export_hdr(true);
    match scene {
        "export" | "export-small" => s.set_export_advanced_open(true),
        // Windows without the HEVC extension: the Store hint.
        "export-hevc" => s.set_export_hevc_hint(true),
        "export-running" => {
            s.set_export_status(1);
            s.set_export_progress(0.42);
            s.set_export_minutes_left(3);
        }
        _ => {
            s.set_export_status(2);
            s.set_export_output_name("Summer in Italy.mp4".into());
        }
    }
}

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
                focused: false,
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
        // The show opens with a colour card.
        let is_title = i == 0;
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
            // The title scene selects the opening card; the others a photo.
            selected: if scene == "title" { i == 0 } else { i == 1 },
            duration_text: if is_video {
                "0:06".into()
            } else {
                "4.0 s".into()
            },
            // Cut, dissolve, fade, slide left, zoom, wipe left: overlays of both kinds.
            transition: [0, 1, 2, 4, 12, 8][usize::try_from(i).unwrap_or(0)],
            is_video,
            muted: i == 4,
            moving: i == 1 || i == 3,
            focused: false,
            is_title,
            title_bg: slint::Color::from_rgb_u8(22, 44, 84),
        });
        x += width - 40.0 + 4.0;
    }
    editor.set_clips(ModelRc::new(VecModel::from(clips)));
    // Two songs, the playlist loops once; fade-out at the end.
    let songs = [
        ("Sunny road.mp3", 360.0, false),
        ("Evening.m4a", 250.0, false),
        ("Sunny road.mp3", x as f32 - 618.0, true),
    ];
    let mut sx = 0.0f32;
    let blocks: Vec<SongBlockView> = songs
        .iter()
        .map(|(t, w, repeat)| {
            let b = SongBlockView {
                x: sx,
                width: *w,
                title: (*t).into(),
                repeat: *repeat,
            };
            sx += w;
            b
        })
        .collect();
    editor.set_song_blocks(ModelRc::new(VecModel::from(blocks)));
    editor.set_music_end_x(sx);
    editor.set_music_fade_x(sx - 120.0);
    editor.set_songs(ModelRc::new(VecModel::from(vec![
        SongItem {
            title: "Sunny road.mp3".into(),
            duration_text: "0:09".into(),
        },
        SongItem {
            title: "Evening.m4a".into(),
            duration_text: "0:06".into(),
        },
    ])));
    editor.set_music_length_text("0:15".into());
    editor.set_show_length_text("0:26".into());
    editor.set_can_fit_music(true);

    editor.set_clip_count(6);
    editor.set_selected_count(1);
    // The selected clip (index 1) is a photo with a dissolve and a zoom-in.
    editor.set_transition_index(1);
    editor.set_motion_index(1);
    // Feedback of the last bulk action, as after "Shuffle transitions".
    let strings = app.global::<Strings>();
    strings.set_count(6);
    editor.set_status_text(strings.get_shuffled_transitions());
    editor.set_strip_width(x as f32);
    editor.set_playhead_x(300.0);
    editor.set_time_text("00:00:07".into());
    editor.set_total_text("00:00:26".into());
    // The preview is rendered by the real compositor, so the text on it
    // and the overlay's boxes can be checked against each other.
    let editing = scene == "text-edit";
    let (preview, boxes) = render_preview(fixtures, editing)?;
    editor.set_preview(preview);
    editor.set_frame_aspect(16.0 / 9.0);
    let text_selected = scene == "text" || editing;
    editor.set_preview_texts(ModelRc::new(VecModel::from(
        boxes
            .iter()
            .enumerate()
            .map(|(i, b)| PreviewText {
                index: i32::try_from(i).unwrap_or(0),
                x: b.0,
                y: b.1,
                width: b.2,
                height: b.3,
                selected: text_selected && i == 0,
            })
            .collect::<Vec<_>>(),
    )));
    editor.set_text_blocks(ModelRc::new(VecModel::from(vec![
        TextBlockView {
            index: 0,
            x: 8.0,
            width: 300.0,
            row: 0,
            title: "Summer in Italy".into(),
            selected: text_selected,
            focused: false,
        },
        TextBlockView {
            index: 1,
            x: 330.0,
            width: 360.0,
            row: 0,
            title: "Rome, the Colosseum".into(),
            selected: false,
            focused: false,
        },
    ])));
    editor.set_text_rows(1);
    editor.set_can_undo(true);
    editor.set_dirty(true);

    match scene {
        // Export running in the background: the toolbar status button.
        "export-background" => {
            editor.set_export_status(1);
            editor.set_export_progress(0.42);
            editor.set_export_minutes_left(3);
        }
        "settings" => app.global::<Shell>().set_settings_open(true),
        "text" | "text-edit" => {
            editor.set_selected_count(0);
            editor.set_text_selected_count(1);
            editor.set_text_content("Summer in Italy".into());
            editor.set_text_font("Playfair Display".into());
            editor.set_text_points(97);
            editor.set_text_bold(true);
            editor.set_text_color_index(0);
            editor.set_text_shadow(true);
            editor.set_text_duration(4.0);
            editor.set_text_enter_index(10);
            editor.set_text_enter_seconds(0.8);
            editor.set_text_exit_index(1);
            editor.set_text_exit_seconds(0.5);
            editor.set_guide_x(if scene == "text" { 0.5 } else { -1.0 });
            if editing {
                editor.set_editing_index(0);
                editor.set_editing_text("Summer in Italy".into());
                editor.set_editing_family("Playfair Display".into());
                editor.set_editing_size(0.09);
                editor.set_editing_color(slint::Color::from_rgb_u8(255, 255, 255));
                editor.set_editing_bold(true);
            }
        }
        "music" => {
            editor.set_music_selected(true);
            editor.set_selected_count(0);
            editor.set_music_fade_out(3.0);
        }
        "title" => {
            // The opening colour card is selected: background, duration.
            editor.set_only_title_target(true);
            editor.set_has_title_target(true);
            editor.set_has_photo_target(false);
            editor.set_title_background_index(2);
            editor.set_transition_index(0);
        }
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

/// A text box as fractions of the picture: x, y, width, height.
type BoxFrac = (f32, f32, f32, f32);

/// Renders the preview frame of a small project (the landscape fixture with
/// two texts) and returns it with the texts' boxes as picture fractions.
/// `hide_first` leaves the first text out, as while it is edited in place.
fn render_preview(fixtures: &Path, hide_first: bool) -> Result<(slint::Image, Vec<BoxFrac>)> {
    use clipforge_core::{Clip, Command, MediaId, MediaRef, Project, RefKind, TextItem, Ticks};
    use clipforge_render::source::MapProvider;
    use clipforge_render::{Compositor, RenderQuality, SourceImage};
    let img = image::open(fixtures.join("photo_landscape.jpg"))?.to_rgba8();
    let (w, h) = img.dimensions();
    let id = MediaId::new();
    let mut provider = MapProvider::default();
    provider.images.insert(
        id,
        SourceImage {
            width: w,
            height: h,
            rgba: std::sync::Arc::new(img.into_raw()),
        },
    );
    let mut project = Project::new();
    let media = MediaRef {
        id,
        kind: RefKind::Photo,
        path: "/fixture".into(),
        fingerprint_hash: 1,
        size: 1,
        pixel_size: Some((w, h)),
        duration: None,
        captured_at_ms: None,
        hdr: false,
        name: "photo_landscape.jpg".into(),
    };
    Command::InsertClips {
        entries: vec![(0, Clip::photo(id, Ticks::from_seconds(8)))],
        media: vec![media],
    }
    .apply(&mut project)
    .map_err(|e| anyhow::anyhow!("{e}"))?;
    let mut title = TextItem::new("Summer in Italy", Ticks::ZERO, Ticks::from_seconds(8));
    title.y = 4_200;
    title.style.font = "Playfair Display".into();
    title.style.points = 97;
    title.style.bold = true;
    let mut label = TextItem::new("Rome, the Colosseum", Ticks::ZERO, Ticks::from_seconds(8));
    label.y = 8_700;
    label.width = 5_000;
    label.style.font = "Montserrat".into();
    label.style.points = 45;
    label.style.background = Some([0, 0, 0, 170]);
    label.style.shadow = false;
    let texts = vec![title, label];
    let measure = clipforge_render::text::TextRenderer::new();
    let frame_size = RenderQuality::Preview.frame_size(project.settings.aspect);
    #[allow(clippy::cast_possible_truncation)]
    let boxes = texts
        .iter()
        .map(|t| {
            let b = measure.hit_box(t, frame_size);
            let (fw, fh) = (f64::from(frame_size.0), f64::from(frame_size.1));
            (
                (b.x / fw) as f32,
                (b.y / fh) as f32,
                (b.width / fw) as f32,
                (b.height / fh) as f32,
            )
        })
        .collect();
    project.texts = texts;
    if hide_first {
        project.texts.remove(0);
    }
    let frame = Compositor::new().render(
        &project,
        Ticks::from_seconds(2),
        RenderQuality::Preview,
        &provider,
    );
    let buffer = slint::SharedPixelBuffer::<slint::Rgba8Pixel>::clone_from_slice(
        &frame.rgba,
        frame.width,
        frame.height,
    );
    Ok((slint::Image::from_rgba8(buffer), boxes))
}

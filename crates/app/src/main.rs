//! ClipForge desktop application entry point.
//!
//! This crate is deliberately thin: it owns the Slint window, the persisted
//! [`settings::Settings`] and the glue between UI callbacks and the core
//! crates. No domain logic lives here.

// Slint-generated code contains `unsafe`; the workspace-wide `deny` still
// applies to the hand-written code in this crate.
#![cfg_attr(test, allow(clippy::unwrap_used, clippy::expect_used, clippy::panic))]

mod drop;
mod editor_ui;
mod editor_view;
mod export_view;
mod format;
mod language;
mod library_ui;
mod library_view;
mod player;
mod preview_worker;
mod settings;
mod text_view;

use anyhow::{Context, Result};
use std::sync::Arc;

use clipforge_jobs::Scheduler;
use clipforge_library::Library;
use clipforge_media::Backends;
use clipforge_platform::AppDirs;
use slint::{ComponentHandle, ModelRc, VecModel};
use tracing::{info, warn};

/// Generated Slint bindings. Lints are relaxed here only because the code is
/// generated; hand-written code keeps the workspace lints.
mod ui {
    #![allow(
        // Generated with element debug info (debug builds) it contains
        // `todo!()` in paths the app never takes.
        clippy::todo,
        missing_debug_implementations,
        unreachable_pub,
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
use ui::{MainWindow, Strings};

fn main() -> Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(tracing_subscriber::EnvFilter::from_default_env())
        .init();

    let dirs = AppDirs::standard().context("cannot determine home directory")?;
    dirs.ensure_exist()?;
    let store = settings::SettingsStore::new(dirs.settings_file());
    let mut current = store.load();
    info!(?current, "settings loaded");

    let window = MainWindow::new()?;
    // Must run after the first component exists.
    language::apply(current.language);

    window.set_app_version(env!("CARGO_PKG_VERSION").into());
    window
        .global::<ui::Shell>()
        .set_macos(cfg!(target_os = "macos"));
    let system_label = window.global::<Strings>().get_system_language();
    window.set_language_options(ModelRc::new(VecModel::from(language::picker_options(
        &system_label,
    ))));
    window.set_language_index(language::picker_index(current.language));

    {
        let store = store.clone();
        let weak = window.as_weak();
        window.on_language_selected(move |index| {
            let pref = language::picker_preference(index);
            language::apply(pref);
            current.language = pref;
            if let Err(e) = store.save(&current) {
                warn!(error = %e, "could not save settings");
            }
            if let Some(w) = weak.upgrade() {
                // Rebuild the picker so the "System language" entry is translated too.
                let label = w.global::<Strings>().get_system_language();
                w.set_language_options(ModelRc::new(VecModel::from(language::picker_options(
                    &label,
                ))));
            }
        });
    }
    window.on_quit(|| {
        let _ = slint::quit_event_loop();
    });
    let workers =
        std::thread::available_parallelism().map_or(2, |n| n.get().saturating_sub(1).max(2));
    // Reading the installed fonts takes a moment; do it while the window
    // comes up so the first text does not wait.
    let _ = std::thread::Builder::new()
        .name("clipforge-fonts".into())
        .spawn(clipforge_render::text::preload_fonts);
    let scheduler = Arc::new(Scheduler::new(workers));
    let backends = Backends::discover();
    if backends.has_ffmpeg() {
        info!("ffmpeg found; video and HEIC support enabled");
    } else {
        warn!("ffmpeg not found; videos and HEIC files cannot be read");
    }
    let library = Arc::new(
        Library::open(&dirs, backends.clone(), Arc::clone(&scheduler))
            .context("opening media library")?,
    );
    let library_controller = library_ui::LibraryController::new(&window, Arc::clone(&library));
    drop::install(&window, &library_controller);
    let editor_controller =
        editor_ui::EditorController::new(&window, library, scheduler, backends, dirs.clone());
    library_controller.connect_editor(
        editor_controller.add_media_handle(),
        editor_controller.preview_thumb_handle(),
        editor_controller.drop_hover_handle(),
        editor_controller.drop_insert_handle(),
    );
    if let Ok(paths) = std::env::var("CLIPFORGE_IMPORT") {
        library_controller.import(paths.split(':').map(std::path::PathBuf::from).collect());
    }

    window.run()?;
    editor_controller.flush_autosave();
    drop(library_controller);
    drop(editor_controller);
    Ok(())
}

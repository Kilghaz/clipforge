//! ClipForge desktop application entry point.
//!
//! This crate is deliberately thin: it owns the Slint window, the persisted
//! [`settings::Settings`] and the glue between UI callbacks and the core
//! crates. No domain logic lives here.

// Slint-generated code contains `unsafe`; the workspace-wide `deny` still
// applies to the hand-written code in this crate.
#![cfg_attr(test, allow(clippy::unwrap_used, clippy::expect_used, clippy::panic))]

mod language;
mod settings;

use anyhow::{Context, Result};
use clipforge_platform::AppDirs;
use slint::{ComponentHandle, ModelRc, VecModel};
use tracing::{info, warn};

/// Generated Slint bindings. Lints are relaxed here only because the code is
/// generated; hand-written code keeps the workspace lints.
mod ui {
    #![allow(
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
    window.on_new_project(|| info!("new project: arrives with milestone 2"));
    window.on_open_project(|| info!("open project: arrives with milestone 2"));

    window.run()?;
    Ok(())
}

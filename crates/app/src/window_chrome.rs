//! Window frame details the OS draws: the title bar follows the app's dark
//! theme (ADR-0009) instead of the system theme, so a light Windows or
//! macOS setting does not put a white title bar over the dark app.

use i_slint_backend_winit::WinitWindowAccessor;

/// Call after the window is shown (the OS window exists from then on).
pub(crate) fn dark_title_bar(window: &slint::Window) {
    let applied = window.with_winit_window(|w| w.set_theme(Some(winit::window::Theme::Dark)));
    if applied.is_none() {
        tracing::debug!("no winit window; title bar keeps the system theme");
    }
}

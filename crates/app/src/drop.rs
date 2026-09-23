//! OS drag-and-drop of files onto the window.
//!
//! Slint does not surface external file drops, but its winit backend lets
//! us observe the raw winit events. winit delivers one `DroppedFile` per
//! file; we batch them for a few milliseconds so a drop of 500 files becomes
//! one import.

use std::cell::RefCell;
use std::path::PathBuf;
use std::rc::Rc;
use std::time::Duration;

use i_slint_backend_winit::{EventResult, WinitWindowAccessor};
use slint::{ComponentHandle, Timer, TimerMode};
use winit::event::WindowEvent;

use crate::library_ui::LibraryController;
use crate::ui::{LibraryState, MainWindow};

/// How long to wait for further `DroppedFile` events before importing.
const BATCH_DELAY: Duration = Duration::from_millis(60);

pub(crate) fn install(window: &MainWindow, controller: &LibraryController) {
    let pending: Rc<RefCell<Vec<PathBuf>>> = Rc::new(RefCell::new(Vec::new()));
    let flush_timer = Rc::new(Timer::default());
    let weak = window.as_weak();
    let import = controller.import_handle();

    window.window().on_winit_window_event(move |_, event| {
        match event {
            WindowEvent::HoveredFile(_) => set_hover(&weak, true),
            WindowEvent::HoveredFileCancelled => set_hover(&weak, false),
            WindowEvent::DroppedFile(path) => {
                set_hover(&weak, false);
                pending.borrow_mut().push(path.clone());
                let pending = Rc::clone(&pending);
                let import = import.clone();
                flush_timer.start(TimerMode::SingleShot, BATCH_DELAY, move || {
                    let paths: Vec<PathBuf> = pending.borrow_mut().drain(..).collect();
                    import(paths);
                });
            }
            _ => {}
        }
        EventResult::Propagate
    });
}

fn set_hover(weak: &slint::Weak<MainWindow>, on: bool) {
    if let Some(w) = weak.upgrade() {
        w.global::<LibraryState>().set_drop_hover(on);
    }
}

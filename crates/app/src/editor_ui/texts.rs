//! Editor actions for the text track and on-preview text editing
//! (docs/ux/decisions/m5b-text-track.md). Part of `editor_ui`.

use std::sync::Arc;
use std::time::Instant;

use clipforge_core::text::{FRAME_UNITS, Font, TextAlign, TextId, TextItem, TextMotion};
use clipforge_core::timeline::total_duration;
use clipforge_core::{Command, Ticks};
use slint::{Color, ComponentHandle, SharedString};
use tracing::warn;

use super::Inner;
use crate::editor_view::{self, layout};
use crate::text_view::{self, LaneGesture, TextGesture};
use crate::ui::{EditorState, PreviewText, Strings, TextBlockView};

/// Undo group for typing a text (inspector field or inline editor).
const TEXT_TYPING_GROUP: u64 = 2;
/// Arrow-key nudge in frame units (0.5 %), Shift: 5 %.
const NUDGE: i32 = 50;
const NUDGE_LARGE: i32 = 500;

/// The eight text colours offered as swatches; must match
/// `Theme.text-colors` (checked by a test).
pub(crate) const TEXT_COLORS: [[u8; 3]; 8] = [
    [255, 255, 255],
    [20, 20, 22],
    [255, 214, 10],
    [255, 146, 48],
    [235, 64, 52],
    [255, 105, 180],
    [64, 156, 255],
    [52, 199, 89],
];
/// Opacity of the box behind a text.
const BOX_ALPHA: u8 = 170;

/// A preview or lane drag in progress: the items as they were and as they
/// are shown now.
#[derive(Debug)]
pub(super) struct TextDrag {
    kind: DragKind,
    start: (f32, f32),
    originals: Vec<(usize, TextItem)>,
    current: Vec<(usize, TextItem)>,
}

#[derive(Copy, Clone, Debug)]
enum DragKind {
    Preview(TextGesture),
    Lane(LaneGesture),
}

impl Inner {
    // ----- selection --------------------------------------------------------

    pub(super) fn selected_text_indices(&self) -> Vec<usize> {
        let mut v: Vec<usize> = self
            .text_selection
            .iter()
            .filter_map(|id| self.project.text_index(*id))
            .collect();
        v.sort_unstable();
        v
    }

    /// Selecting texts clears clips and music, and the other way round.
    pub(super) fn leave_texts(&mut self) {
        self.end_inline_edit();
        if !self.text_selection.is_empty() {
            self.text_selection.clear();
            self.history.seal();
        }
    }

    fn select_texts_only(&mut self) {
        self.leave_music();
        self.selection.clear();
    }

    fn select_text(&mut self, index: usize, shift: bool, toggle: bool) {
        let Some(id) = self.project.texts.get(index).map(|t| t.id) else {
            return;
        };
        self.select_texts_only();
        if shift || toggle {
            if let Some(pos) = self.text_selection.iter().position(|s| *s == id) {
                if toggle {
                    self.text_selection.remove(pos);
                }
            } else {
                self.text_selection.push(id);
            }
        } else if !self.text_selection.contains(&id) {
            self.text_selection = vec![id];
        }
    }

    // ----- adding and removing ---------------------------------------------

    /// Ctrl/Cmd+T, the toolbar button and the Edit menu: a text at the
    /// playhead, selected, with the inline editor open.
    pub(super) fn add_text(&mut self) {
        let placeholder = self
            .state()
            .map(|w| w.global::<Strings>().get_new_text().to_string())
            .unwrap_or_default();
        let item = text_view::new_text(
            &placeholder,
            self.playhead,
            total_duration(&self.project.clips),
        );
        self.insert_text(item, true);
    }

    /// Double-click on empty text lane space.
    pub(super) fn text_lane_double_clicked(&mut self, x: f32) {
        let boxes = layout(&self.project.clips, self.pps);
        let t = editor_view::time_at_x(&self.project.clips, &boxes, x);
        let placeholder = self
            .state()
            .map(|w| w.global::<Strings>().get_new_text().to_string())
            .unwrap_or_default();
        let item = TextItem::new(placeholder, t, text_view::NEW_TEXT_DURATION);
        self.insert_text(item, true);
    }

    pub(super) fn insert_text(&mut self, item: TextItem, edit: bool) {
        let id = item.id;
        let start = item.start;
        self.apply(Command::InsertTexts {
            entries: vec![(self.project.texts.len(), item)],
        });
        self.select_texts_only();
        self.text_selection = vec![id];
        // Show the new text: move the playhead into it.
        if !self
            .project
            .texts
            .iter()
            .any(|t| t.id == id && t.visible_at(self.playhead))
        {
            self.playhead = start;
        }
        if edit {
            self.editing_text = Some(id);
        }
        self.preview_dirty = true;
        self.sync_all();
    }

    /// Delete with texts selected.
    pub(super) fn remove_selected_texts(&mut self) -> bool {
        let indices = self.selected_text_indices();
        if indices.is_empty() {
            return false;
        }
        self.end_inline_edit();
        self.text_selection.clear();
        self.apply(Command::RemoveTexts { indices });
        self.preview_dirty = true;
        true
    }

    // ----- lane gestures ----------------------------------------------------

    pub(super) fn text_lane_pressed(
        &mut self,
        index: i32,
        shift: bool,
        toggle: bool,
        code: i32,
        x: f32,
    ) {
        let Ok(index) = usize::try_from(index) else {
            return;
        };
        self.end_inline_edit();
        self.select_text(index, shift, toggle);
        let originals: Vec<(usize, TextItem)> = self
            .selected_text_indices()
            .into_iter()
            .map(|i| (i, self.project.texts[i].clone()))
            .collect();
        self.text_drag = Some(TextDrag {
            kind: DragKind::Lane(LaneGesture::from_code(code)),
            start: (x, 0.0),
            current: originals.clone(),
            originals,
        });
        if let Some(t) = self.project.texts.get(index)
            && !t.visible_at(self.playhead)
        {
            self.playhead = t.start;
            self.preview_dirty = true;
        }
        self.sync_all();
    }

    pub(super) fn text_lane_dragged(&mut self, x: f32) {
        let Some(drag) = self.text_drag.as_mut() else {
            return;
        };
        let DragKind::Lane(gesture) = drag.kind else {
            return;
        };
        let boxes = layout(&self.project.clips, self.pps);
        let clips = &self.project.clips;
        let dt = editor_view::time_at_x(clips, &boxes, x)
            - editor_view::time_at_x(clips, &boxes, drag.start.0);
        drag.current = drag
            .originals
            .iter()
            .map(|(i, t)| (*i, text_view::lane_drag(t, gesture, dt)))
            .collect();
        self.preview_dirty = true;
        self.refresh_texts();
    }

    // ----- preview gestures -------------------------------------------------

    pub(super) fn preview_text_pressed(&mut self, index: i32, code: i32, nx: f32, ny: f32) {
        let Ok(index) = usize::try_from(index) else {
            return;
        };
        if self.editing_text.is_some() {
            self.end_inline_edit();
        }
        // Handles act on the selection; a press on a text selects it.
        if code == 0 {
            self.select_text(index, false, false);
        }
        let originals: Vec<(usize, TextItem)> = self
            .selected_text_indices()
            .into_iter()
            .map(|i| (i, self.project.texts[i].clone()))
            .collect();
        self.text_drag = Some(TextDrag {
            kind: DragKind::Preview(TextGesture::from_code(code)),
            start: (nx, ny),
            current: originals.clone(),
            originals,
        });
        self.sync_all();
    }

    pub(super) fn preview_text_dragged(&mut self, nx: f32, ny: f32) {
        let aspect = self.frame_aspect();
        let Some(drag) = self.text_drag.as_mut() else {
            return;
        };
        let DragKind::Preview(gesture) = drag.kind else {
            return;
        };
        drag.current = drag
            .originals
            .iter()
            .map(|(i, t)| {
                let moved = text_view::apply_gesture(t, gesture, drag.start, (nx, ny), aspect).0;
                (*i, moved)
            })
            .collect();
        // Moving snaps the first dragged text (centre to the frame centre or
        // another text, edges to the safe margin); the others keep formation.
        let mut guides = text_view::Guides::default();
        if gesture == TextGesture::Move
            && let Some((_, primary)) = drag.current.first()
        {
            let frame =
                clipforge_render::RenderQuality::Preview.frame_size(self.project.settings.aspect);
            let b = self.text_measure.hit_box(primary, frame);
            #[allow(clippy::cast_possible_truncation)]
            let half = (
                (b.width / 2.0 / f64::from(frame.0) * f64::from(FRAME_UNITS)) as i32,
                (b.height / 2.0 / f64::from(frame.1) * f64::from(FRAME_UNITS)) as i32,
            );
            let dragged: Vec<usize> = drag.current.iter().map(|(i, _)| *i).collect();
            let others: Vec<(i32, i32)> = self
                .project
                .texts
                .iter()
                .enumerate()
                .filter(|(i, t)| !dragged.contains(i) && t.visible_at(self.playhead))
                .map(|(_, t)| (t.x, t.y))
                .collect();
            let (dx, dy, g) = text_view::snap(primary, half, &others);
            for (_, t) in &mut drag.current {
                t.x += dx;
                t.y += dy;
            }
            guides = g;
        }
        self.show_guides(guides);
        self.preview_dirty = true;
        self.refresh_texts();
    }

    /// Release on the lane or the preview: one undo step for the gesture.
    pub(super) fn text_released(&mut self) {
        let Some(drag) = self.text_drag.take() else {
            return;
        };
        self.show_guides(text_view::Guides::default());
        let changed: Vec<(usize, TextItem)> = drag
            .current
            .into_iter()
            .zip(&drag.originals)
            .filter(|((_, now), (_, before))| now != before)
            .map(|(c, _)| c)
            .collect();
        if changed.is_empty() {
            self.refresh_texts();
            return;
        }
        self.apply(Command::SetTexts { entries: changed });
        self.preview_dirty = true;
    }

    #[allow(clippy::cast_precision_loss)]
    fn show_guides(&self, g: text_view::Guides) {
        if let Some(w) = self.state() {
            let s = w.global::<EditorState>();
            let frac = |v: Option<i32>| v.map_or(-1.0, |v| v as f32 / FRAME_UNITS as f32);
            s.set_guide_x(frac(g.x));
            s.set_guide_y(frac(g.y));
        }
    }

    // ----- keyboard on the text lane ---------------------------------------

    /// Tab reached the text lane: focus the first selected text, else the
    /// first one in time.
    pub(super) fn text_lane_focus_entered(&mut self) {
        let texts = &self.project.texts;
        let from_selection = self.selected_text_indices().first().copied();
        let focus = from_selection.or_else(|| text_view::text_nav(texts, None, true));
        self.text_focus = focus.map(|i| texts[i].id);
        self.sync_texts();
    }

    /// ←/→ on the focused lane: the previous / next text; the playhead
    /// follows so it shows on the preview.
    pub(super) fn text_lane_navigate(&mut self, forward: bool) {
        let focus = self.text_focus.and_then(|id| self.project.text_index(id));
        let Some(next) = text_view::text_nav(&self.project.texts, focus, forward) else {
            return;
        };
        let t = &self.project.texts[next];
        self.text_focus = Some(t.id);
        if !t.visible_at(self.playhead) {
            self.playhead = t.start;
            self.preview_dirty = true;
        }
        self.sync_all();
    }

    /// Enter / Space on the focused lane: select the focused text; Enter on
    /// a text that is already the only selection edits it in place.
    pub(super) fn text_lane_activate(&mut self) {
        let Some(index) = self.text_focus.and_then(|id| self.project.text_index(id)) else {
            return;
        };
        if self.selected_text_indices() == [index] {
            self.begin_inline_edit(i32::try_from(index).unwrap_or(-1));
            return;
        }
        self.select_text(index, false, false);
        if let Some(t) = self.project.texts.get(index)
            && !t.visible_at(self.playhead)
        {
            self.playhead = t.start;
            self.preview_dirty = true;
        }
        self.sync_all();
    }

    /// Escape during a drag: back to where it started.
    pub(super) fn cancel_text_drag(&mut self) -> bool {
        if self.text_drag.take().is_none() {
            return false;
        }
        self.show_guides(text_view::Guides::default());

        self.preview_dirty = true;
        self.refresh_texts();
        true
    }

    /// A press on empty preview space.
    pub(super) fn preview_background_pressed(&mut self) {
        if self.editing_text.is_some() || !self.text_selection.is_empty() {
            self.leave_texts();
            self.preview_dirty = true;
            self.sync_all();
        }
    }

    pub(super) fn text_nudge(&mut self, dx: i32, dy: i32, large: bool) {
        let step = if large { NUDGE_LARGE } else { NUDGE };
        let entries: Vec<(usize, TextItem)> = self
            .selected_text_indices()
            .into_iter()
            .map(|i| {
                (
                    i,
                    text_view::nudge(&self.project.texts[i], dx * step, dy * step),
                )
            })
            .collect();
        if entries.is_empty() {
            return;
        }
        self.apply(Command::SetTexts { entries });
        self.preview_dirty = true;
    }

    // ----- inline editing ---------------------------------------------------

    pub(super) fn begin_inline_edit(&mut self, index: i32) {
        let Some(id) = usize::try_from(index)
            .ok()
            .and_then(|i| self.project.texts.get(i))
            .map(|t| t.id)
        else {
            return;
        };
        self.select_texts_only();
        self.text_selection = vec![id];
        self.editing_text = Some(id);
        self.preview_dirty = true;
        self.sync_all();
    }

    /// Enter with a single text selected.
    pub(super) fn edit_selected_text(&mut self) {
        if let [i] = self.selected_text_indices()[..] {
            self.begin_inline_edit(i32::try_from(i).unwrap_or(-1));
        }
    }

    pub(super) fn end_inline_edit(&mut self) {
        if self.editing_text.take().is_some() {
            self.history.seal();
            self.preview_dirty = true;
            if let Some(w) = self.state() {
                w.global::<EditorState>().set_editing_index(-1);
            }
            self.refresh_texts();
        }
    }

    /// Typing in the inline editor or the inspector field: all selected
    /// texts get the new content; one undo step per typing session.
    pub(super) fn text_typed(&mut self, content: &str) {
        let entries: Vec<(usize, TextItem)> = self
            .selected_text_indices()
            .into_iter()
            .map(|i| {
                (
                    i,
                    TextItem {
                        text: content.to_owned(),
                        ..self.project.texts[i].clone()
                    },
                )
            })
            .collect();
        if entries.is_empty() {
            return;
        }
        match self.history.apply_merging(
            &mut self.project,
            Command::SetTexts { entries },
            TEXT_TYPING_GROUP,
        ) {
            Ok(()) => {
                self.dirty_since.get_or_insert_with(Instant::now);
                self.preview_dirty = true;
                self.sync_all();
            }
            Err(e) => warn!(error = %e, "text rejected"),
        }
    }

    pub(super) fn text_typing_done(&mut self) {
        self.history.seal();
    }

    // ----- inspector --------------------------------------------------------

    /// Applies `f` to every selected text as one undo step.
    fn edit_texts(&mut self, f: impl Fn(&mut TextItem)) {
        let entries: Vec<(usize, TextItem)> = self
            .selected_text_indices()
            .into_iter()
            .map(|i| {
                let mut t = self.project.texts[i].clone();
                f(&mut t);
                (i, t)
            })
            .filter(|(i, t)| *t != self.project.texts[*i])
            .collect();
        if entries.is_empty() {
            return;
        }
        self.apply(Command::SetTexts { entries });
        self.preview_dirty = true;
    }

    pub(super) fn text_font(&mut self, index: i32) {
        let font = Font::from_index(usize::try_from(index).unwrap_or(0));
        self.edit_texts(|t| t.style.font = font);
    }

    pub(super) fn text_size(&mut self, percent: f32) {
        #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
        let size = (percent * 100.0).round().clamp(
            f32::from(clipforge_core::TextStyle::MIN_SIZE),
            f32::from(clipforge_core::TextStyle::MAX_SIZE),
        ) as u16;
        self.edit_texts(|t| t.style.size = size);
    }

    pub(super) fn text_bold(&mut self, on: bool) {
        self.edit_texts(|t| t.style.bold = on);
    }

    pub(super) fn text_italic(&mut self, on: bool) {
        self.edit_texts(|t| t.style.italic = on);
    }

    pub(super) fn text_align(&mut self, index: i32) {
        let align = TextAlign::from_index(usize::try_from(index).unwrap_or(1));
        self.edit_texts(|t| t.style.align = align);
    }

    pub(super) fn text_color(&mut self, index: i32) {
        let Some([r, g, b]) = usize::try_from(index)
            .ok()
            .and_then(|i| TEXT_COLORS.get(i).copied())
        else {
            return;
        };
        self.edit_texts(|t| t.style.color = [r, g, b, 255]);
    }

    pub(super) fn text_box(&mut self, on: bool) {
        self.edit_texts(|t| {
            t.style.background = on.then_some([0, 0, 0, BOX_ALPHA]);
            // A box replaces the shadow.
            if on {
                t.style.shadow = false;
            }
        });
    }

    pub(super) fn text_box_color(&mut self, index: i32) {
        let Some([r, g, b]) = usize::try_from(index)
            .ok()
            .and_then(|i| TEXT_COLORS.get(i).copied())
        else {
            return;
        };
        self.edit_texts(|t| t.style.background = Some([r, g, b, BOX_ALPHA]));
    }

    pub(super) fn text_shadow(&mut self, on: bool) {
        self.edit_texts(|t| t.style.shadow = on);
    }

    pub(super) fn text_duration(&mut self, secs: f32) {
        let d = Ticks::from_seconds_f64(f64::from(secs)).max(TextItem::MIN_DURATION);
        self.edit_texts(|t| t.duration = d);
    }

    pub(super) fn text_enter(&mut self, index: i32, secs: f32) {
        let kind = TextMotion::from_index(usize::try_from(index).unwrap_or(0));
        let duration = Ticks::from_seconds_f64(f64::from(secs.max(0.0)));
        self.edit_texts(|t| {
            t.enter.kind = kind;
            t.enter.duration = duration;
        });
    }

    pub(super) fn text_exit(&mut self, index: i32, secs: f32) {
        let kind = TextMotion::from_index(usize::try_from(index).unwrap_or(0));
        let duration = Ticks::from_seconds_f64(f64::from(secs.max(0.0)));
        self.edit_texts(|t| {
            t.exit.kind = kind;
            t.exit.duration = duration;
        });
    }

    // ----- display ----------------------------------------------------------

    /// Texts as shown: the project's, with a drag in progress applied.
    pub(super) fn display_texts(&self) -> Vec<TextItem> {
        let mut texts = self.project.texts.clone();
        if let Some(drag) = &self.text_drag {
            for (i, t) in &drag.current {
                if let Some(slot) = texts.get_mut(*i) {
                    *slot = t.clone();
                }
            }
        }
        texts
    }

    fn frame_aspect(&self) -> f32 {
        #[allow(clippy::cast_possible_truncation)]
        let a = self.project.settings.aspect.ratio() as f32;
        a
    }

    /// The project the preview renders: drag overrides applied, the text
    /// being edited in place hidden (the inline editor shows it).
    pub(super) fn preview_project(&self) -> clipforge_core::Project {
        let mut p = self.project.clone();
        p.texts = self.display_texts();
        if let Some(id) = self.editing_text {
            p.texts.retain(|t| t.id != id);
        }
        p
    }

    /// Lane, overlay and snapshot after a live change (no command).
    pub(super) fn refresh_texts(&mut self) {
        self.snapshot = Arc::new(self.preview_project());
        self.sync_texts();
    }

    /// Text lane, preview overlay and Text inspector.
    pub(super) fn sync_texts(&self) {
        let Some(w) = self.state() else { return };
        let s = w.global::<EditorState>();
        let texts = self.display_texts();
        let clips = self.display_clips();
        let boxes = self.display_boxes();
        let (blocks, rows) = text_view::text_blocks(&texts, &clips, &boxes);
        let selected = |i: usize| {
            texts
                .get(i)
                .is_some_and(|t| self.text_selection.contains(&t.id))
        };
        let lane: Vec<TextBlockView> = blocks
            .iter()
            .map(|b| TextBlockView {
                index: i32::try_from(b.index).unwrap_or(-1),
                x: b.x,
                width: b.width,
                row: i32::try_from(b.row).unwrap_or(0),
                title: SharedString::from(b.title.as_str()),
                selected: selected(b.index),
                focused: texts
                    .get(b.index)
                    .is_some_and(|t| self.text_focus == Some(t.id)),
            })
            .collect();
        super::update_rows(&self.text_blocks_model, lane);
        s.set_text_rows(i32::try_from(rows).unwrap_or(1));
        s.set_frame_aspect(self.frame_aspect());

        // Boxes of the texts visible at the playhead, as picture fractions.
        let frame =
            clipforge_render::RenderQuality::Preview.frame_size(self.project.settings.aspect);
        let (fw, fh) = (f64::from(frame.0), f64::from(frame.1));
        let visible: Vec<PreviewText> = texts
            .iter()
            .enumerate()
            .filter(|(_, t)| t.visible_at(self.playhead))
            .map(|(i, t)| {
                let b = self.text_measure.hit_box(t, frame);
                #[allow(clippy::cast_possible_truncation)]
                PreviewText {
                    index: i32::try_from(i).unwrap_or(-1),
                    x: (b.x / fw) as f32,
                    y: (b.y / fh) as f32,
                    width: (b.width / fw) as f32,
                    height: (b.height / fh) as f32,
                    selected: self.text_selection.contains(&t.id),
                }
            })
            .collect();
        super::update_rows(&self.preview_texts_model, visible);

        // Inline editor.
        let editing = self
            .editing_text
            .and_then(|id| texts.iter().position(|t| t.id == id));
        s.set_editing_index(editing.map_or(-1, |i| i32::try_from(i).unwrap_or(-1)));
        if let Some(t) = editing.and_then(|i| texts.get(i)) {
            let [r, g, b, a] = t.style.color;
            s.set_editing_text(SharedString::from(t.text.as_str()));
            s.set_editing_family(self.text_measure.family_name(t.style.font).into());
            #[allow(clippy::cast_precision_loss)]
            s.set_editing_size(f32::from(t.style.size) / FRAME_UNITS as f32);
            s.set_editing_color(Color::from_argb_u8(a, r, g, b));
            s.set_editing_bold(t.style.bold);
            s.set_editing_italic(t.style.italic);
            s.set_editing_align(i32::try_from(t.style.align.index()).unwrap_or(1));
            // The inline editor imitates the shadow and the box.
            s.set_editing_shadow(t.style.shadow);
            s.set_editing_box(t.style.background.is_some());
            if let Some([r, g, b, a]) = t.style.background {
                s.set_editing_box_color(Color::from_argb_u8(a, r, g, b));
            }
        }

        // Inspector: the first selected text's values.
        let sel = self.selected_text_indices();
        s.set_text_selected_count(i32::try_from(sel.len()).unwrap_or(0));
        if let Some(first) = sel.first().and_then(|&i| texts.get(i)) {
            let mixed = sel.iter().any(|&i| texts[i].text != first.text);
            let content = if mixed { "" } else { first.text.as_str() };
            if s.get_text_content() != content {
                s.set_text_content(content.into());
            }
            s.set_text_mixed(mixed);
            let st = &first.style;
            s.set_text_font_index(i32::try_from(st.font.index()).unwrap_or(0));
            s.set_text_size_percent(f32::from(st.size) / 100.0);
            s.set_text_bold(st.bold);
            s.set_text_italic(st.italic);
            s.set_text_align_index(i32::try_from(st.align.index()).unwrap_or(1));
            let colour_index = |rgb: [u8; 3]| {
                TEXT_COLORS
                    .iter()
                    .position(|c| *c == rgb)
                    .map_or(-1, |i| i32::try_from(i).unwrap_or(-1))
            };
            s.set_text_color_index(colour_index([st.color[0], st.color[1], st.color[2]]));
            s.set_text_box(st.background.is_some());
            s.set_text_box_color_index(
                st.background
                    .map_or(1, |b| colour_index([b[0], b[1], b[2]])),
            );
            s.set_text_shadow(st.shadow);
            #[allow(clippy::cast_possible_truncation)]
            {
                s.set_text_duration(first.duration.as_seconds_f64() as f32);
                s.set_text_enter_index(i32::try_from(first.enter.kind.index()).unwrap_or(1));
                s.set_text_enter_seconds(first.enter.duration.as_seconds_f64() as f32);
                s.set_text_exit_index(i32::try_from(first.exit.kind.index()).unwrap_or(1));
                s.set_text_exit_seconds(first.exit.duration.as_seconds_f64() as f32);
            }
        }
    }

    /// Ids of all texts (after undo some may be gone).
    pub(super) fn retain_text_selection(&mut self) {
        let ids: Vec<TextId> = self.project.texts.iter().map(|t| t.id).collect();
        self.text_selection.retain(|id| ids.contains(id));
        if self.editing_text.is_some_and(|id| !ids.contains(&id)) {
            self.editing_text = None;
        }
        if self.text_focus.is_some_and(|id| !ids.contains(&id)) {
            self.text_focus = None;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn text_colours_match_the_theme() {
        let theme = include_str!("../../ui/theme.slint");
        let key = "out property <[color]> text-colors: [";
        let at = theme.find(key).unwrap_or_else(|| panic!("{key} missing")) + key.len();
        let list = &theme[at..at + theme[at..].find(']').unwrap()];
        let parsed: Vec<[u8; 3]> = list
            .split(',')
            .map(|c| {
                let hex = c.trim().trim_start_matches('#');
                [0, 2, 4].map(|i| u8::from_str_radix(&hex[i..i + 2], 16).unwrap())
            })
            .collect();
        assert_eq!(parsed, TEXT_COLORS.to_vec());
    }
}

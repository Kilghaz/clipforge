//! Undo / redo stacks.

use crate::command::{Command, CommandError, CommandLabel};
use crate::project::Project;

/// Applies commands to a project and remembers how to undo them.
#[derive(Debug, Default)]
pub struct History {
    undo: Vec<(CommandLabel, Command)>,
    /// Merge group of the top undo entry (see [`History::apply_merging`]).
    merge: Option<u64>,
    redo: Vec<(CommandLabel, Command)>,
    /// Number of applied commands since the last `mark_saved`.
    dirty: i64,
}

impl History {
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Applies `command`; on success clears the redo stack.
    pub fn apply(&mut self, project: &mut Project, command: Command) -> Result<(), CommandError> {
        let label = command.label();
        let inverse = command.apply(project)?;
        self.undo.push((label, inverse));
        self.redo.clear();
        self.dirty += 1;
        self.merge = None;
        Ok(())
    }

    /// Like [`History::apply`], but consecutive commands with the same
    /// `group` form one undo step (typing into one text field). The first
    /// command's inverse is kept; later inverses are dropped. Any other
    /// command, an undo, a save or [`History::seal`] ends the group.
    pub fn apply_merging(
        &mut self,
        project: &mut Project,
        command: Command,
        group: u64,
    ) -> Result<(), CommandError> {
        if self.merge == Some(group) && self.redo.is_empty() && !self.undo.is_empty() {
            command.apply(project)?;
            return Ok(());
        }
        self.apply(project, command)?;
        self.merge = Some(group);
        Ok(())
    }

    /// Ends the current merge group; the next edit starts a new undo step.
    pub fn seal(&mut self) {
        self.merge = None;
    }

    /// Undoes the last command. Returns its label, or `None` if nothing to undo.
    pub fn undo(&mut self, project: &mut Project) -> Option<CommandLabel> {
        self.merge = None;
        let (label, inverse) = self.undo.pop()?;
        match inverse.apply(project) {
            Ok(redo) => {
                self.redo.push((label, redo));
                self.dirty -= 1;
                Some(label)
            }
            Err(_) => None,
        }
    }

    pub fn redo(&mut self, project: &mut Project) -> Option<CommandLabel> {
        self.merge = None;
        let (label, cmd) = self.redo.pop()?;
        match cmd.apply(project) {
            Ok(inverse) => {
                self.undo.push((label, inverse));
                self.dirty += 1;
                Some(label)
            }
            Err(_) => None,
        }
    }

    #[must_use]
    pub fn can_undo(&self) -> bool {
        !self.undo.is_empty()
    }

    #[must_use]
    pub fn can_redo(&self) -> bool {
        !self.redo.is_empty()
    }

    #[must_use]
    pub fn undo_label(&self) -> Option<CommandLabel> {
        self.undo.last().map(|(l, _)| *l)
    }

    #[must_use]
    pub fn redo_label(&self) -> Option<CommandLabel> {
        self.redo.last().map(|(l, _)| *l)
    }

    /// True if the project differs from the last saved state.
    #[must_use]
    pub fn is_dirty(&self) -> bool {
        self.dirty != 0
    }

    pub fn mark_saved(&mut self) {
        self.dirty = 0;
        // An edit merged after a save must count as a change.
        self.merge = None;
    }

    /// Forgets everything (new/open project).
    pub fn clear(&mut self) {
        self.undo.clear();
        self.redo.clear();
        self.dirty = 0;
        self.merge = None;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ids::MediaId;
    use crate::project::{Clip, Fit, MediaRef, RefKind};
    use crate::time::Ticks;

    fn seeded() -> Project {
        let mut p = Project::new();
        let m = MediaRef {
            id: MediaId::new(),
            kind: RefKind::Photo,
            path: "/a".into(),
            fingerprint_hash: 1,
            size: 1,
            pixel_size: None,
            duration: None,
            captured_at_ms: None,
            name: "a".into(),
        };
        let clips = (0..3)
            .map(|i| (i, Clip::photo(m.id, Ticks::from_seconds(2))))
            .collect();
        Command::InsertClips {
            entries: clips,
            media: vec![m],
        }
        .apply(&mut p)
        .unwrap();
        p
    }

    #[test]
    fn undo_redo_walk_back_and_forth() {
        let mut p = seeded();
        let mut h = History::new();
        let s0 = p.clone();
        h.apply(
            &mut p,
            Command::SetFit {
                indices: vec![0],
                fit: Fit::Cover,
            },
        )
        .unwrap();
        let s1 = p.clone();
        h.apply(&mut p, Command::RemoveClips { indices: vec![2] })
            .unwrap();
        let s2 = p.clone();
        assert!(h.is_dirty());
        assert_eq!(h.undo_label(), Some(CommandLabel::Remove));

        assert_eq!(h.undo(&mut p), Some(CommandLabel::Remove));
        assert_eq!(p, s1);
        assert_eq!(h.undo(&mut p), Some(CommandLabel::Fit));
        assert_eq!(p, s0);
        assert!(!h.is_dirty());
        assert_eq!(h.undo(&mut p), None);

        assert_eq!(h.redo(&mut p), Some(CommandLabel::Fit));
        assert_eq!(h.redo(&mut p), Some(CommandLabel::Remove));
        assert_eq!(p, s2);
        assert_eq!(h.redo(&mut p), None);
    }

    #[test]
    fn new_command_clears_redo_and_failed_command_changes_nothing() {
        let mut p = seeded();
        let mut h = History::new();
        h.apply(
            &mut p,
            Command::SetFit {
                indices: vec![0],
                fit: Fit::Cover,
            },
        )
        .unwrap();
        h.undo(&mut p);
        assert!(h.can_redo());
        h.apply(
            &mut p,
            Command::SetFit {
                indices: vec![1],
                fit: Fit::Cover,
            },
        )
        .unwrap();
        assert!(!h.can_redo());
        let before = p.clone();
        assert!(
            h.apply(&mut p, Command::RemoveClips { indices: vec![99] })
                .is_err()
        );
        assert_eq!(p, before);
        assert_eq!(h.undo_label(), Some(CommandLabel::Fit));
    }

    #[test]
    fn saved_marker_tracks_dirtiness_through_undo() {
        let mut p = seeded();
        let mut h = History::new();
        h.apply(
            &mut p,
            Command::SetFit {
                indices: vec![0],
                fit: Fit::Cover,
            },
        )
        .unwrap();
        h.mark_saved();
        assert!(!h.is_dirty());
        h.undo(&mut p);
        assert!(h.is_dirty());
        h.redo(&mut p);
        assert!(!h.is_dirty());
        h.clear();
        assert!(!h.can_undo() && !h.can_redo());
    }

    #[test]
    fn merged_text_edits_undo_in_one_step() {
        use crate::text::TextItem;
        let mut p = Project::new();
        let mut h = History::new();
        let item = TextItem::new("", Ticks::ZERO, Ticks::SECOND);
        h.apply(
            &mut p,
            Command::InsertTexts {
                entries: vec![(0, item.clone())],
            },
        )
        .unwrap();
        let before = p.clone();
        let typed = |t: &str| Command::SetTexts {
            entries: vec![(
                0,
                TextItem {
                    text: t.to_owned(),
                    ..item.clone()
                },
            )],
        };
        for t in ["R", "Ro", "Rom", "Rome"] {
            h.apply_merging(&mut p, typed(t), 7).unwrap();
        }
        assert_eq!(p.texts[0].text, "Rome");
        h.undo(&mut p);
        assert_eq!(p, before, "one undo removes the whole typing session");
        h.redo(&mut p);
        assert_eq!(p.texts[0].text, "Rome");
        // A sealed group, another group or a save starts a new step.
        h.seal();
        h.apply_merging(&mut p, typed("Rome!"), 7).unwrap();
        h.mark_saved();
        h.apply_merging(&mut p, typed("Rome!!"), 7).unwrap();
        assert!(h.is_dirty(), "edit after save counts");
        h.undo(&mut p);
        assert_eq!(p.texts[0].text, "Rome!");
        h.undo(&mut p);
        assert_eq!(p.texts[0].text, "Rome");
    }
}

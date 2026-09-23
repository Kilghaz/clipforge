//! Undo / redo stacks.

use crate::command::{Command, CommandError, CommandLabel};
use crate::project::Project;

/// Applies commands to a project and remembers how to undo them.
#[derive(Debug, Default)]
pub struct History {
    undo: Vec<(CommandLabel, Command)>,
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
        Ok(())
    }

    /// Undoes the last command. Returns its label, or `None` if nothing to undo.
    pub fn undo(&mut self, project: &mut Project) -> Option<CommandLabel> {
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
    }

    /// Forgets everything (new/open project).
    pub fn clear(&mut self) {
        self.undo.clear();
        self.redo.clear();
        self.dirty = 0;
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
}

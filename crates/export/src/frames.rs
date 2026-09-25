//! Producing the frames of an export.

use clipforge_core::timeline::frame_at;
use clipforge_core::{Project, Ticks};
use clipforge_render::{Compositor, Frame, RenderQuality, SourceProvider};

/// What the exporter gets for frame `i`.
#[derive(Debug)]
pub enum FrameRef {
    /// A newly rendered frame.
    New(Frame),
    /// Pixel-identical to the previous frame (a still with no transition);
    /// the exporter re-sends the cached buffer without converting again.
    SameAsPrevious,
}

/// Anything that yields frames in order.
pub trait FrameSource {
    /// Total number of frames.
    fn len(&self) -> u64;

    fn is_empty(&self) -> bool {
        self.len() == 0
    }

    /// Frame `index` in `0..len()`.
    fn frame(&mut self, index: u64) -> FrameRef;
}

/// Frames of a project timeline at full quality.
pub struct TimelineFrames<'a> {
    project: &'a Project,
    compositor: &'a Compositor,
    sources: &'a dyn SourceProvider,
    quality: RenderQuality,
    len: u64,
    last_key: Option<(usize, Ticks)>,
}

impl std::fmt::Debug for TimelineFrames<'_> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("TimelineFrames")
            .field("len", &self.len)
            .finish()
    }
}

impl<'a> TimelineFrames<'a> {
    #[must_use]
    pub fn new(
        project: &'a Project,
        compositor: &'a Compositor,
        sources: &'a dyn SourceProvider,
        quality: RenderQuality,
    ) -> Self {
        let total = clipforge_core::timeline::total_duration(&project.clips);
        let fd = project.settings.frame_rate.frame_duration();
        #[allow(clippy::cast_sign_loss)]
        let len = ((total.flicks() + fd.flicks() - 1) / fd.flicks()).max(0) as u64;
        TimelineFrames {
            project,
            compositor,
            sources,
            quality,
            len,
            last_key: None,
        }
    }

    fn time_of(&self, index: u64) -> Ticks {
        #[allow(clippy::cast_possible_wrap)]
        Ticks::from_frames(index as i64, self.project.settings.frame_rate)
    }
}

impl FrameSource for TimelineFrames<'_> {
    fn len(&self) -> u64 {
        self.len
    }

    fn frame(&mut self, index: u64) -> FrameRef {
        let t = self.time_of(index);
        // A still photo outside any transition shows the same picture for
        // every frame: key it by clip index so repeats are free. Moving
        // photos (Ken Burns) and the opening transition change every frame.
        let key = frame_at(&self.project.clips, t).and_then(|at| {
            let (idx, local) = at.current;
            let clip = &self.project.clips[idx];
            let in_opening =
                idx == 0 && local < clipforge_core::timeline::opening_overlap(&self.project.clips);
            let still = clip.is_photo() && clip.motion == clipforge_core::Motion::None;
            (still && at.outgoing.is_none() && !in_opening).then_some((idx, Ticks::ZERO))
        });
        if key.is_some() && key == self.last_key {
            return FrameRef::SameAsPrevious;
        }
        self.last_key = key;
        FrameRef::New(
            self.compositor
                .render(self.project, t, self.quality, self.sources),
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use clipforge_core::project::{MediaRef, RefKind, Transition, TransitionKind};
    use clipforge_core::{Clip, Command, MediaId, Resolution};
    use clipforge_render::source::{MapProvider, SourceImage};

    fn project(secs: &[i64], dissolve: bool) -> (Project, MapProvider) {
        let mut p = Project::new();
        let mut provider = MapProvider::default();
        let mut entries = Vec::new();
        let mut media = Vec::new();
        for (i, s) in secs.iter().enumerate() {
            let id = MediaId::new();
            provider
                .images
                .insert(id, SourceImage::solid(16, 9, [i as u8 * 40, 0, 0]));
            media.push(MediaRef {
                id,
                kind: RefKind::Photo,
                path: "/p".into(),
                fingerprint_hash: 1,
                size: 1,
                pixel_size: None,
                duration: None,
                captured_at_ms: None,
                name: "p".into(),
            });
            let mut clip = Clip::photo(id, Ticks::from_seconds(*s));
            if dissolve && i > 0 {
                clip.transition_in = Transition {
                    kind: TransitionKind::CrossDissolve,
                    duration: Ticks::SECOND,
                };
            }
            entries.push((i, clip));
        }
        Command::InsertClips { entries, media }
            .apply(&mut p)
            .unwrap();
        (p, provider)
    }

    #[test]
    fn frame_count_rounds_up_and_repeats_stills() {
        let (p, provider) = project(&[2, 1], false);
        let c = Compositor::new();
        let mut frames =
            TimelineFrames::new(&p, &c, &provider, RenderQuality::Full(Resolution::FullHd));
        assert_eq!(frames.len(), 90);
        let mut new = 0;
        for i in 0..frames.len() {
            if matches!(frames.frame(i), FrameRef::New(_)) {
                new += 1;
            }
        }
        assert_eq!(new, 2, "one render per still");
    }

    #[test]
    fn transitions_render_every_frame_in_the_overlap() {
        let (p, provider) = project(&[2, 2], true);
        let c = Compositor::new();
        let mut frames = TimelineFrames::new(&p, &c, &provider, RenderQuality::Preview);
        // total = 3 s -> 90 frames; overlap 1.0..2.0 s = 30 frames, plus 2 stills.
        assert_eq!(frames.len(), 90);
        let new = (0..frames.len())
            .filter(|i| matches!(frames.frame(*i), FrameRef::New(_)))
            .count();
        assert_eq!(new, 32);
    }

    #[test]
    fn moving_photos_render_every_frame() {
        let (mut p, provider) = project(&[2], false);
        Command::SetMotion {
            indices: vec![0],
            motion: clipforge_core::Motion::ZoomIn,
        }
        .apply(&mut p)
        .unwrap();
        let c = Compositor::new();
        let mut frames = TimelineFrames::new(&p, &c, &provider, RenderQuality::Preview);
        let new = (0..frames.len())
            .filter(|i| matches!(frames.frame(*i), FrameRef::New(_)))
            .count();
        assert_eq!(new, 60, "2 s at 30 fps, all distinct");
    }

    #[test]
    fn opening_transition_frames_are_rendered_individually() {
        let (mut p, provider) = project(&[2], false);
        let t = Transition {
            kind: TransitionKind::SlideLeft,
            duration: Ticks::SECOND,
        };
        Command::SetTransition {
            indices: vec![0],
            transition: t,
        }
        .apply(&mut p)
        .unwrap();
        let c = Compositor::new();
        let mut frames = TimelineFrames::new(&p, &c, &provider, RenderQuality::Preview);
        let new = (0..frames.len())
            .filter(|i| matches!(frames.frame(*i), FrameRef::New(_)))
            .count();
        assert_eq!(new, 31, "30 opening frames plus one still for the rest");
    }

    #[test]
    fn empty_project_has_no_frames() {
        let p = Project::new();
        let c = Compositor::new();
        let provider = MapProvider::default();
        let frames = TimelineFrames::new(&p, &c, &provider, RenderQuality::Preview);
        assert!(frames.is_empty());
    }
}

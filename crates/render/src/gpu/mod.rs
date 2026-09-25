//! GPU compositor on wgpu (Metal on macOS, DirectX 12 or Vulkan on Windows).
//!
//! Mirrors [`crate::Compositor`] exactly: the same layout maths
//! ([`crate::layout::place`], [`Motion::camera`]) and the same transition
//! timing ([`crate::transition::ease`]), so both paths produce the same
//! picture and can be compared in tests. The work is done as alpha-blended
//! textured quads (`quad.wgsl`):
//!
//! 1. each visible clip is drawn into its own frame-sized texture (fit,
//!    cover, rotation and Ken Burns are one destination rectangle);
//! 2. a transition combines the two clip textures (alpha, offset, scissor
//!    or scale) into the output texture;
//! 3. the output is read back as an RGBA [`Frame`] for the preview image
//!    and the encoder.
//!
//! Stills are uploaded once with a mipmap chain so strong downscaling stays
//! sharp; video frames reuse one texture per video. Validation errors are
//! logged, never panics.

use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use bytemuck::{Pod, Zeroable};
use clipforge_core::project::{Clip, ClipSource, Motion, Quarter, TransitionKind};
use clipforge_core::timeline::opening_overlap;
use clipforge_core::{Fit, MediaId, Project, Ticks};

use crate::draw::RectF;
use crate::frame::Frame;
use crate::layout::place;
use crate::quality::RenderQuality;
use crate::source::{SourceImage, SourceProvider};
use crate::text::{TextImage, TextRenderer, text_draw};
use crate::transition::{ease, opening_colour};
use crate::{FrameRenderer, PLACEHOLDER_RGB};

const FORMAT: wgpu::TextureFormat = wgpu::TextureFormat::Rgba8Unorm;
/// Uniform slots per frame (each 256 bytes, the dynamic offset alignment).
const SLOTS: u64 = 32;
const SLOT_SIZE: u64 = 256;
/// Upper bound for cached still textures (bytes, including mip chains).
const STILL_BUDGET: u64 = 768 * 1024 * 1024;

#[repr(C)]
#[derive(Copy, Clone, Pod, Zeroable)]
struct QuadUniform {
    dst: [f32; 4],
    uv: [f32; 4],
    colour: [f32; 4],
    params: [f32; 4],
}

/// A texture the shader can sample, with its bind group.
struct Sampled {
    _texture: wgpu::Texture,
    bind_group: wgpu::BindGroup,
    size: (u32, u32),
    bytes: u64,
}

/// A frame-sized render target that can also be sampled.
struct Target {
    texture: wgpu::Texture,
    view: wgpu::TextureView,
    bind_group: wgpu::BindGroup,
}

struct CachedStill {
    sampled: Sampled,
    last_used: u64,
}

struct CachedVideo {
    sampled: Sampled,
    /// The uploaded frame, to skip re-uploading repeats. Holding the `Arc`
    /// (not just its address) matters: once a frame is freed the allocator
    /// may hand its address to the next one, and a stale picture would stick.
    frame: Arc<Vec<u8>>,
}

/// Text blocks uploaded recently; the `Arc` identifies the rendered image
/// (the text renderer hands out the same `Arc` for the same text).
const TEXT_TEXTURES: usize = 16;

struct State {
    text_textures: Vec<(Arc<TextImage>, Sampled)>,
    stills: HashMap<(MediaId, u32, u32), CachedStill>,
    videos: HashMap<MediaId, CachedVideo>,
    still_bytes: u64,
    clock: u64,
    targets: Option<((u32, u32), [Target; 3])>,
    readback: Option<(u64, wgpu::Buffer)>,
}

/// wgpu-based implementation of [`FrameRenderer`].
pub struct GpuCompositor {
    device: wgpu::Device,
    queue: wgpu::Queue,
    adapter_name: String,
    pipeline: wgpu::RenderPipeline,
    layout: wgpu::BindGroupLayout,
    sampler: wgpu::Sampler,
    uniforms: wgpu::Buffer,
    /// 1 × 1 white texture for solid draws (set right after construction).
    white: Option<Sampled>,
    text: TextRenderer,
    state: Mutex<State>,
}

impl std::fmt::Debug for GpuCompositor {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("GpuCompositor")
            .field("adapter", &self.adapter_name)
            .finish()
    }
}

/// A scissor rectangle in frame pixels: x, y, width, height.
type Scissor = (u32, u32, u32, u32);

/// What a transition blends from: an outgoing clip (index, local time) or,
/// with `None`, the opening colour of the first clip's transition.
type BlendFrom = Option<(usize, Ticks)>;

/// One draw in a pass.
#[derive(Copy, Clone)]
enum Paint<'a> {
    /// Sample a texture into a destination rectangle (frame pixels).
    Texture {
        bind_group: &'a wgpu::BindGroup,
        dst: RectF,
        rotate: Quarter,
        alpha: f32,
        scissor: Option<Scissor>,
    },
    /// A solid colour over the whole target.
    Solid { rgb: [u8; 3], alpha: f32 },
}

impl GpuCompositor {
    /// Opens the best adapter. `None` if the machine has no usable GPU (or
    /// only a software adapter is requested and missing).
    #[must_use]
    pub fn new() -> Option<GpuCompositor> {
        let instance =
            wgpu::Instance::new(wgpu::InstanceDescriptor::new_without_display_handle_from_env());
        let adapter = pollster::block_on(instance.request_adapter(&wgpu::RequestAdapterOptions {
            power_preference: wgpu::PowerPreference::HighPerformance,
            ..Default::default()
        }))
        .ok()?;
        let info = adapter.get_info();
        let (device, queue) = pollster::block_on(adapter.request_device(&wgpu::DeviceDescriptor {
            label: Some("clipforge compositor"),
            required_limits: wgpu::Limits::downlevel_defaults().using_resolution(adapter.limits()),
            ..Default::default()
        }))
        .ok()?;
        device.on_uncaptured_error(Arc::new(|e: wgpu::Error| {
            tracing::error!(error = %e, "GPU validation error");
        }));
        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("quad"),
            source: wgpu::ShaderSource::Wgsl(include_str!("quad.wgsl").into()),
        });
        let layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("quad"),
            entries: &[
                wgpu::BindGroupLayoutEntry {
                    binding: 0,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Texture {
                        sample_type: wgpu::TextureSampleType::Float { filterable: true },
                        view_dimension: wgpu::TextureViewDimension::D2,
                        multisampled: false,
                    },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 1,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 2,
                    visibility: wgpu::ShaderStages::VERTEX_FRAGMENT,
                    ty: wgpu::BindingType::Buffer {
                        ty: wgpu::BufferBindingType::Uniform,
                        has_dynamic_offset: true,
                        min_binding_size: wgpu::BufferSize::new(
                            std::mem::size_of::<QuadUniform>() as u64
                        ),
                    },
                    count: None,
                },
            ],
        });
        let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("quad"),
            bind_group_layouts: &[Some(&layout)],
            immediate_size: 0,
        });
        let pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("quad"),
            layout: Some(&pipeline_layout),
            vertex: wgpu::VertexState {
                module: &shader,
                entry_point: Some("vs_main"),
                compilation_options: wgpu::PipelineCompilationOptions::default(),
                buffers: &[],
            },
            primitive: wgpu::PrimitiveState {
                topology: wgpu::PrimitiveTopology::TriangleStrip,
                ..Default::default()
            },
            depth_stencil: None,
            multisample: wgpu::MultisampleState::default(),
            fragment: Some(wgpu::FragmentState {
                module: &shader,
                entry_point: Some("fs_main"),
                compilation_options: wgpu::PipelineCompilationOptions::default(),
                targets: &[Some(wgpu::ColorTargetState {
                    format: FORMAT,
                    blend: Some(wgpu::BlendState::ALPHA_BLENDING),
                    write_mask: wgpu::ColorWrites::ALL,
                })],
            }),
            multiview_mask: None,
            cache: None,
        });
        let sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            label: Some("linear"),
            address_mode_u: wgpu::AddressMode::ClampToEdge,
            address_mode_v: wgpu::AddressMode::ClampToEdge,
            address_mode_w: wgpu::AddressMode::ClampToEdge,
            mag_filter: wgpu::FilterMode::Linear,
            min_filter: wgpu::FilterMode::Linear,
            mipmap_filter: wgpu::MipmapFilterMode::Linear,
            ..Default::default()
        });
        let uniforms = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("quad uniforms"),
            size: SLOTS * SLOT_SIZE,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        let mut gpu = GpuCompositor {
            white: None,
            text: TextRenderer::new(),
            device,
            queue,
            adapter_name: format!("{} ({:?})", info.name, info.backend),
            pipeline,
            layout,
            sampler,
            uniforms,
            state: Mutex::new(State {
                text_textures: Vec::new(),
                stills: HashMap::new(),
                videos: HashMap::new(),
                still_bytes: 0,
                clock: 0,
                targets: None,
                readback: None,
            }),
        };
        gpu.white = Some(gpu.upload(
            &SourceImage {
                width: 1,
                height: 1,
                rgba: Arc::new(vec![255; 4]),
            },
            false,
        ));
        tracing::info!(adapter = %gpu.adapter_name, "GPU compositor ready");
        Some(gpu)
    }

    /// Name and backend of the adapter in use, for logs and the UI.
    #[must_use]
    pub fn adapter_name(&self) -> &str {
        &self.adapter_name
    }

    fn bind_group(&self, view: &wgpu::TextureView) -> wgpu::BindGroup {
        self.device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: None,
            layout: &self.layout,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: wgpu::BindingResource::TextureView(view),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: wgpu::BindingResource::Sampler(&self.sampler),
                },
                wgpu::BindGroupEntry {
                    binding: 2,
                    resource: wgpu::BindingResource::Buffer(wgpu::BufferBinding {
                        buffer: &self.uniforms,
                        offset: 0,
                        size: wgpu::BufferSize::new(std::mem::size_of::<QuadUniform>() as u64),
                    }),
                },
            ],
        })
    }

    /// Uploads an image; with `mips`, also builds the mipmap chain.
    fn upload(&self, img: &SourceImage, mips: bool) -> Sampled {
        let (w, h) = (img.width.max(1), img.height.max(1));
        let levels = if mips {
            32 - w.max(h).leading_zeros()
        } else {
            1
        };
        let texture = self.device.create_texture(&wgpu::TextureDescriptor {
            label: Some("source"),
            size: wgpu::Extent3d {
                width: w,
                height: h,
                depth_or_array_layers: 1,
            },
            mip_level_count: levels,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: FORMAT,
            usage: wgpu::TextureUsages::TEXTURE_BINDING
                | wgpu::TextureUsages::COPY_DST
                | wgpu::TextureUsages::RENDER_ATTACHMENT,
            view_formats: &[],
        });
        self.write_level0(&texture, img);
        if levels > 1 {
            self.build_mips(&texture, levels);
        }
        let view = texture.create_view(&wgpu::TextureViewDescriptor::default());
        let bind_group = self.bind_group(&view);
        let bytes = u64::from(w) * u64::from(h) * 4 * if levels > 1 { 4 } else { 3 } / 3;
        Sampled {
            _texture: texture,
            bind_group,
            size: (w, h),
            bytes,
        }
    }

    fn write_level0(&self, texture: &wgpu::Texture, img: &SourceImage) {
        let expected = img.width as usize * img.height as usize * 4;
        if img.rgba.len() < expected {
            return;
        }
        self.queue.write_texture(
            wgpu::TexelCopyTextureInfo {
                texture,
                mip_level: 0,
                origin: wgpu::Origin3d::ZERO,
                aspect: wgpu::TextureAspect::All,
            },
            &img.rgba[..expected],
            wgpu::TexelCopyBufferLayout {
                offset: 0,
                bytes_per_row: Some(img.width * 4),
                rows_per_image: Some(img.height),
            },
            wgpu::Extent3d {
                width: img.width,
                height: img.height,
                depth_or_array_layers: 1,
            },
        );
    }

    /// Box-filters each level from the previous one with the quad pipeline.
    fn build_mips(&self, texture: &wgpu::Texture, levels: u32) {
        let mut encoder = self
            .device
            .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                label: Some("mips"),
            });
        let full = RectF {
            x: 0.0,
            y: 0.0,
            width: 1.0,
            height: 1.0,
        };
        let mut uniforms = Vec::new();
        for level in 1..levels {
            let src = texture.create_view(&wgpu::TextureViewDescriptor {
                base_mip_level: level - 1,
                mip_level_count: Some(1),
                ..Default::default()
            });
            let dst = texture.create_view(&wgpu::TextureViewDescriptor {
                base_mip_level: level,
                mip_level_count: Some(1),
                ..Default::default()
            });
            let bg = self.bind_group(&src);
            // Uniform slots are reused per level via separate submits below.
            uniforms.push((bg, dst));
        }
        for (i, (bg, dst)) in uniforms.iter().enumerate() {
            let slot = (i as u64) % SLOTS;
            self.queue.write_buffer(
                &self.uniforms,
                slot * SLOT_SIZE,
                bytemuck::bytes_of(&quad(full, 1, 1, Quarter::None, 1.0, None)),
            );
            {
                let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                    label: Some("mip"),
                    color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                        view: dst,
                        depth_slice: None,
                        resolve_target: None,
                        ops: wgpu::Operations {
                            load: wgpu::LoadOp::Clear(wgpu::Color::BLACK),
                            store: wgpu::StoreOp::Store,
                        },
                    })],
                    depth_stencil_attachment: None,
                    timestamp_writes: None,
                    occlusion_query_set: None,
                    multiview_mask: None,
                });
                pass.set_pipeline(&self.pipeline);
                #[allow(clippy::cast_possible_truncation)]
                pass.set_bind_group(0, bg, &[(slot * SLOT_SIZE) as u32]);
                pass.draw(0..4, 0..1);
            }
            // Each level needs its own uniform contents before the next write.
            if (i as u64 + 1).is_multiple_of(SLOTS) {
                self.queue.submit(Some(
                    std::mem::replace(
                        &mut encoder,
                        self.device
                            .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                                label: Some("mips"),
                            }),
                    )
                    .finish(),
                ));
            }
        }
        self.queue.submit(Some(encoder.finish()));
    }

    fn make_targets(&self, w: u32, h: u32) -> [Target; 3] {
        let make = |label| {
            let texture = self.device.create_texture(&wgpu::TextureDescriptor {
                label: Some(label),
                size: wgpu::Extent3d {
                    width: w,
                    height: h,
                    depth_or_array_layers: 1,
                },
                mip_level_count: 1,
                sample_count: 1,
                dimension: wgpu::TextureDimension::D2,
                format: FORMAT,
                usage: wgpu::TextureUsages::RENDER_ATTACHMENT
                    | wgpu::TextureUsages::TEXTURE_BINDING
                    | wgpu::TextureUsages::COPY_SRC,
                view_formats: &[],
            });
            let view = texture.create_view(&wgpu::TextureViewDescriptor::default());
            let bind_group = self.bind_group(&view);
            Target {
                texture,
                view,
                bind_group,
            }
        };
        [make("clip a"), make("clip b"), make("output")]
    }

    /// Records one pass: clear to `clear` (or keep the target with `None`),
    /// then the paints in order.
    fn pass(
        &self,
        encoder: &mut wgpu::CommandEncoder,
        target: &wgpu::TextureView,
        size: (u32, u32),
        clear: Option<[u8; 3]>,
        paints: &[Paint<'_>],
        slot: &mut u64,
    ) {
        let colour = |c: u8| f64::from(c) / 255.0;
        let load = match clear {
            Some(clear) => wgpu::LoadOp::Clear(wgpu::Color {
                r: colour(clear[0]),
                g: colour(clear[1]),
                b: colour(clear[2]),
                a: 1.0,
            }),
            None => wgpu::LoadOp::Load,
        };
        let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some("compose"),
            color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                view: target,
                depth_slice: None,
                resolve_target: None,
                ops: wgpu::Operations {
                    load,
                    store: wgpu::StoreOp::Store,
                },
            })],
            depth_stencil_attachment: None,
            timestamp_writes: None,
            occlusion_query_set: None,
            multiview_mask: None,
        });
        pass.set_pipeline(&self.pipeline);
        for paint in paints {
            if *slot >= SLOTS {
                tracing::warn!("too many draws in one frame; skipping");
                return;
            }
            let (bind_group, uniform, scissor) = match *paint {
                Paint::Texture {
                    bind_group,
                    dst,
                    rotate,
                    alpha,
                    scissor,
                } => (
                    bind_group,
                    quad(dst, size.0, size.1, rotate, alpha, None),
                    scissor,
                ),
                Paint::Solid { rgb, alpha } => {
                    let Some(white) = self.white.as_ref() else {
                        continue;
                    };
                    let full = RectF {
                        x: 0.0,
                        y: 0.0,
                        width: f64::from(size.0),
                        height: f64::from(size.1),
                    };
                    (
                        &white.bind_group,
                        quad(full, size.0, size.1, Quarter::None, alpha, Some(rgb)),
                        None,
                    )
                }
            };
            match scissor {
                Some((_, _, 0, _) | (_, _, _, 0)) => continue,
                Some((x, y, w, h)) => pass.set_scissor_rect(x, y, w, h),
                None => pass.set_scissor_rect(0, 0, size.0, size.1),
            }
            self.queue.write_buffer(
                &self.uniforms,
                *slot * SLOT_SIZE,
                bytemuck::bytes_of(&uniform),
            );
            #[allow(clippy::cast_possible_truncation)]
            pass.set_bind_group(0, bind_group, &[(*slot * SLOT_SIZE) as u32]);
            pass.draw(0..4, 0..1);
            *slot += 1;
        }
    }

    /// Texture for a clip's source at the given time, uploading if needed.
    /// Returns the bind group (by cache key) and the source size.
    fn source<'s>(
        &self,
        state: &'s mut State,
        clip: &Clip,
        local: Ticks,
        want_edge: u32,
        sources: &dyn SourceProvider,
    ) -> Option<&'s Sampled> {
        match clip.source {
            ClipSource::Photo { .. } => {
                #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
                let edge = if clip.motion != Motion::None {
                    (f64::from(want_edge) * Motion::SCALE).ceil() as u32
                } else {
                    want_edge
                };
                let img = sources.still(clip.media, edge)?;
                let key = (clip.media, img.width, img.height);
                state.clock += 1;
                let clock = state.clock;
                if !state.stills.contains_key(&key) {
                    let sampled = self.upload(&img, true);
                    state.still_bytes += sampled.bytes;
                    state.stills.insert(
                        key,
                        CachedStill {
                            sampled,
                            last_used: clock,
                        },
                    );
                    evict_stills(state, key);
                }
                let entry = state.stills.get_mut(&key)?;
                entry.last_used = clock;
                Some(&entry.sampled)
            }
            ClipSource::Title { .. } => None,
            ClipSource::Video { in_point, .. } => {
                let img = sources.video_frame(clip.media, in_point + local, want_edge)?;
                let reuse = state
                    .videos
                    .get(&clip.media)
                    .is_some_and(|v| v.sampled.size == (img.width, img.height));
                if reuse {
                    if let Some(v) = state.videos.get_mut(&clip.media)
                        && !Arc::ptr_eq(&v.frame, &img.rgba)
                    {
                        self.write_level0(&v.sampled._texture, &img);
                        v.frame = Arc::clone(&img.rgba);
                    }
                } else {
                    let sampled = self.upload(&img, false);
                    state.videos.insert(
                        clip.media,
                        CachedVideo {
                            sampled,
                            frame: Arc::clone(&img.rgba),
                        },
                    );
                }
                state.videos.get(&clip.media).map(|v| &v.sampled)
            }
        }
    }

    /// The text track over the finished frame in target 2.
    fn draw_texts(
        &self,
        state: &mut State,
        project: &Project,
        t: Ticks,
        (w, h): (u32, u32),
        encoder: &mut wgpu::CommandEncoder,
        slot: &mut u64,
    ) {
        let mut draws = Vec::new();
        for item in project.texts.iter().filter(|i| i.visible_at(t)) {
            let Some(img) = self.text.image(item, (w, h)) else {
                continue;
            };
            let Some(d) = text_draw(item, &img, t, (w, h)) else {
                continue;
            };
            let bg = self.text_texture(state, &img);
            draws.push((bg, d));
        }
        if draws.is_empty() {
            return;
        }
        let Some((_, targets)) = state.targets.as_ref() else {
            return;
        };
        let paints: Vec<Paint<'_>> = draws
            .iter()
            .map(|(bg, d)| Paint::Texture {
                bind_group: bg,
                dst: d.dst,
                rotate: Quarter::None,
                alpha: d.alpha,
                scissor: d.clip.map(|c| scissor_of(c, w, h)),
            })
            .collect();
        self.pass(encoder, &targets[2].view, (w, h), None, &paints, slot);
    }

    /// Bind group of an uploaded text block, uploading it on first use.
    fn text_texture(&self, state: &mut State, img: &Arc<TextImage>) -> wgpu::BindGroup {
        if let Some(pos) = state
            .text_textures
            .iter()
            .position(|(i, _)| Arc::ptr_eq(i, img))
        {
            // Most recently used last.
            let entry = state.text_textures.remove(pos);
            let bg = entry.1.bind_group.clone();
            state.text_textures.push(entry);
            return bg;
        }
        let sampled = self.upload(
            &SourceImage {
                width: img.width,
                height: img.height,
                rgba: Arc::new(img.rgba.clone()),
            },
            false,
        );
        let bg = sampled.bind_group.clone();
        if state.text_textures.len() >= TEXT_TEXTURES {
            state.text_textures.remove(0);
        }
        state.text_textures.push((Arc::clone(img), sampled));
        bg
    }

    fn read_back(
        &self,
        state: &mut State,
        texture: &wgpu::Texture,
        w: u32,
        h: u32,
    ) -> Option<Frame> {
        let row = w * 4;
        let padded =
            row.div_ceil(wgpu::COPY_BYTES_PER_ROW_ALIGNMENT) * wgpu::COPY_BYTES_PER_ROW_ALIGNMENT;
        let size = u64::from(padded) * u64::from(h);
        if state.readback.as_ref().is_none_or(|(s, _)| *s != size) {
            let buffer = self.device.create_buffer(&wgpu::BufferDescriptor {
                label: Some("readback"),
                size,
                usage: wgpu::BufferUsages::MAP_READ | wgpu::BufferUsages::COPY_DST,
                mapped_at_creation: false,
            });
            state.readback = Some((size, buffer));
        }
        let (_, buffer) = state.readback.as_ref()?;
        let mut encoder = self
            .device
            .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                label: Some("readback"),
            });
        encoder.copy_texture_to_buffer(
            wgpu::TexelCopyTextureInfo {
                texture,
                mip_level: 0,
                origin: wgpu::Origin3d::ZERO,
                aspect: wgpu::TextureAspect::All,
            },
            wgpu::TexelCopyBufferInfo {
                buffer,
                layout: wgpu::TexelCopyBufferLayout {
                    offset: 0,
                    bytes_per_row: Some(padded),
                    rows_per_image: Some(h),
                },
            },
            wgpu::Extent3d {
                width: w,
                height: h,
                depth_or_array_layers: 1,
            },
        );
        self.queue.submit(Some(encoder.finish()));
        let (tx, rx) = std::sync::mpsc::channel();
        buffer.map_async(wgpu::MapMode::Read, .., move |r| {
            let _ = tx.send(r);
        });
        if let Err(e) = self.device.poll(wgpu::PollType::wait_indefinitely()) {
            tracing::error!(error = %e, "GPU poll failed");
            return None;
        }
        rx.recv().ok()?.ok()?;
        let mut rgba = Vec::with_capacity((row * h) as usize);
        {
            let view = buffer.get_mapped_range(..).ok()?;
            for y in 0..h as usize {
                let start = y * padded as usize;
                rgba.extend_from_slice(&view[start..start + row as usize]);
            }
        }
        buffer.unmap();
        for px in rgba.as_chunks_mut::<4>().0 {
            px[3] = 255;
        }
        Some(Frame {
            width: w,
            height: h,
            rgba,
        })
    }
}

fn evict_stills(state: &mut State, keep: (MediaId, u32, u32)) {
    while state.still_bytes > STILL_BUDGET && state.stills.len() > 1 {
        let Some(oldest) = state
            .stills
            .iter()
            .filter(|(k, _)| **k != keep)
            .min_by_key(|(_, v)| v.last_used)
            .map(|(k, _)| *k)
        else {
            break;
        };
        if let Some(e) = state.stills.remove(&oldest) {
            state.still_bytes = state.still_bytes.saturating_sub(e.sampled.bytes);
        }
    }
}

/// Uniform for a quad covering `dst` (frame pixels) in a `w × h` target.
fn quad(
    dst: RectF,
    w: u32,
    h: u32,
    rotate: Quarter,
    alpha: f32,
    solid: Option<[u8; 3]>,
) -> QuadUniform {
    #[allow(clippy::cast_possible_truncation)]
    let nx = |x: f64| (x / f64::from(w) * 2.0 - 1.0) as f32;
    #[allow(clippy::cast_possible_truncation)]
    let ny = |y: f64| (1.0 - y / f64::from(h) * 2.0) as f32;
    let turns = match rotate {
        Quarter::None => 0.0,
        Quarter::Cw90 => 1.0,
        Quarter::Cw180 => 2.0,
        Quarter::Cw270 => 3.0,
    };
    let c = solid.unwrap_or([0, 0, 0]);
    QuadUniform {
        dst: [
            nx(dst.x),
            ny(dst.y),
            nx(dst.x + dst.width),
            ny(dst.y + dst.height),
        ],
        uv: [0.0, 0.0, 1.0, 1.0],
        colour: [
            f32::from(c[0]) / 255.0,
            f32::from(c[1]) / 255.0,
            f32::from(c[2]) / 255.0,
            1.0,
        ],
        params: [
            turns,
            alpha.clamp(0.0, 1.0),
            if solid.is_some() { 1.0 } else { 0.0 },
            0.0,
        ],
    }
}

/// Destination rectangle of a clip's picture (same maths as the CPU
/// `compose`): fit or cover, then the Ken Burns camera.
fn clip_rect(
    src: (u32, u32),
    rotate: Quarter,
    w: u32,
    h: u32,
    fit: Fit,
    camera: (f64, f64, f64),
) -> Option<RectF> {
    let (sw, sh) = if rotate.swaps_dimensions() {
        (src.1, src.0)
    } else {
        src
    };
    let r = place(sw, sh, w, h, fit);
    if r.width == 0 || r.height == 0 {
        return None;
    }
    let (zoom, cx, cy) = camera;
    #[allow(clippy::cast_precision_loss)]
    let base = RectF {
        x: r.x as f64,
        y: r.y as f64,
        width: f64::from(r.width),
        height: f64::from(r.height),
    };
    Some(base.zoomed(
        zoom,
        -cx * zoom * base.width,
        -cy * zoom * base.height,
        w,
        h,
    ))
}

impl FrameRenderer for GpuCompositor {
    fn render(
        &self,
        project: &Project,
        t: Ticks,
        quality: RenderQuality,
        sources: &dyn SourceProvider,
    ) -> Frame {
        let (w, h) = quality.frame_size(project.settings.aspect);
        let at = clipforge_core::timeline::frame_at(&project.clips, t);
        if at.is_none() && !project.texts.iter().any(|i| i.visible_at(t)) {
            return Frame::black(w, h);
        }
        let Ok(mut guard) = self.state.lock() else {
            return Frame::black(w, h);
        };
        let state = &mut *guard;
        if state.targets.as_ref().is_none_or(|(s, _)| *s != (w, h)) {
            state.targets = Some(((w, h), self.make_targets(w, h)));
        }
        let want_edge = quality
            .source_edge(clipforge_core::Aspect::Landscape16x9)
            .max(w.max(h));
        let mut encoder = self
            .device
            .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                label: Some("frame"),
            });
        let mut slot = 0u64;

        // Renders a clip into target `idx`; returns false if nothing to draw.
        let draw_clip = |state: &mut State,
                         clip: &Clip,
                         local: Ticks,
                         idx: usize,
                         encoder: &mut wgpu::CommandEncoder,
                         slot: &mut u64| {
            let camera = if clip.is_photo() && clip.motion != Motion::None {
                let d = clip.duration();
                #[allow(clippy::cast_precision_loss)]
                let p = if d > Ticks::ZERO {
                    local.flicks() as f64 / d.flicks() as f64
                } else {
                    0.0
                };
                clip.motion.camera(p)
            } else {
                (1.0, 0.0, 0.0)
            };
            let source = self
                .source(state, clip, local, want_edge, sources)
                .map(|s| (s.size, s.bind_group.clone()));
            let Some((_, targets)) = state.targets.as_ref() else {
                return;
            };
            let view = &targets[idx].view;
            let picture = source.and_then(|(size, bg)| {
                clip_rect(size, clip.rotate, w, h, clip.fit, camera).map(|r| (r, bg))
            });
            let clear = match clip.source {
                ClipSource::Title { background, .. } => crate::title_rgb(background),
                _ if picture.is_some() => [0, 0, 0],
                _ => PLACEHOLDER_RGB,
            };
            let mut paints = Vec::with_capacity(2);
            if let Some((dst, bg)) = &picture {
                paints.push(Paint::Texture {
                    bind_group: bg,
                    dst: *dst,
                    rotate: clip.rotate,
                    alpha: 1.0,
                    scissor: None,
                });
            }
            self.pass(encoder, view, (w, h), Some(clear), &paints, slot);
        };

        if let Some(at) = at {
            let (cur_idx, cur_local) = at.current;
            let current = &project.clips[cur_idx];
            let kind = current.transition_in.kind;
            let opening = if cur_idx == 0 {
                opening_overlap(&project.clips)
            } else {
                Ticks::ZERO
            };
            // (from, progress, kind to draw); `from` None = the opening colour
            // of the clip's own transition (a fade from a colour is a dissolve).
            let blend: Option<(BlendFrom, f32, TransitionKind)> = if let Some(out) = at.outgoing {
                Some((Some(out), at.progress, kind))
            } else if opening > Ticks::ZERO && cur_local < opening {
                #[allow(clippy::cast_precision_loss)]
                let p = cur_local.flicks() as f32 / opening.flicks() as f32;
                Some((
                    None,
                    p,
                    if kind.is_fade() {
                        TransitionKind::CrossDissolve
                    } else {
                        kind
                    },
                ))
            } else {
                None
            };
            let opening_rgb = opening_colour(kind);

            match blend {
                None => draw_clip(state, current, cur_local, 2, &mut encoder, &mut slot),
                Some((from, p, draw_kind)) => {
                    // `to` into A, `from` into B (or a solid colour).
                    draw_clip(state, current, cur_local, 0, &mut encoder, &mut slot);
                    let from_rgb = match from {
                        Some((i, local)) => {
                            draw_clip(state, &project.clips[i], local, 1, &mut encoder, &mut slot);
                            None
                        }
                        None => Some(opening_rgb),
                    };
                    let Some((_, targets)) = state.targets.as_ref() else {
                        return Frame::black(w, h);
                    };
                    let full = RectF {
                        x: 0.0,
                        y: 0.0,
                        width: f64::from(w),
                        height: f64::from(h),
                    };
                    let to = |dst: RectF, alpha: f32, scissor: Option<Scissor>| Paint::Texture {
                        bind_group: &targets[0].bind_group,
                        dst,
                        rotate: Quarter::None,
                        alpha,
                        scissor,
                    };
                    let from_paint = |dst: RectF| match from_rgb {
                        Some(rgb) => Paint::Solid { rgb, alpha: 1.0 },
                        None => Paint::Texture {
                            bind_group: &targets[1].bind_group,
                            dst,
                            rotate: Quarter::None,
                            alpha: 1.0,
                            scissor: None,
                        },
                    };
                    let paints = transition_paints(draw_kind, p, w, h, full, &to, &from_paint);
                    self.pass(
                        &mut encoder,
                        &targets[2].view,
                        (w, h),
                        Some([0, 0, 0]),
                        &paints,
                        &mut slot,
                    );
                }
            }
        } else if let Some((_, targets)) = state.targets.as_ref() {
            // Past the clips: black under the text track.
            self.pass(
                &mut encoder,
                &targets[2].view,
                (w, h),
                Some([0, 0, 0]),
                &[],
                &mut slot,
            );
        }
        self.draw_texts(state, project, t, (w, h), &mut encoder, &mut slot);
        self.queue.submit(Some(encoder.finish()));
        let Some((_, targets)) = state.targets.take() else {
            return Frame::black(w, h);
        };
        let frame = self.read_back(state, &targets[2].texture, w, h);
        state.targets = Some(((w, h), targets));
        frame.unwrap_or_else(|| Frame::black(w, h))
    }

    fn name(&self) -> &str {
        "gpu"
    }
}

/// A frame-pixel rectangle as a scissor, clamped to the frame. Pixel
/// centres decide, like the CPU path.
#[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
fn scissor_of(c: RectF, w: u32, h: u32) -> Scissor {
    let x0 = (c.x - 0.5).ceil().clamp(0.0, f64::from(w)) as u32;
    let y0 = (c.y - 0.5).ceil().clamp(0.0, f64::from(h)) as u32;
    let x1 = (c.x + c.width - 0.5).ceil().clamp(0.0, f64::from(w)) as u32;
    let y1 = (c.y + c.height - 0.5).ceil().clamp(0.0, f64::from(h)) as u32;
    (x0, y0, x1.saturating_sub(x0), y1.saturating_sub(y0))
}

/// The draws of a transition pass (same timing as `transition::apply`).
fn transition_paints<'a>(
    kind: TransitionKind,
    progress: f32,
    w: u32,
    h: u32,
    full: RectF,
    to: &dyn Fn(RectF, f32, Option<Scissor>) -> Paint<'a>,
    from: &dyn Fn(RectF) -> Paint<'a>,
) -> Vec<Paint<'a>> {
    let p = progress.clamp(0.0, 1.0);
    let shifted = |dx: i64, dy: i64| {
        #[allow(clippy::cast_precision_loss)]
        RectF {
            x: dx as f64,
            y: dy as f64,
            width: full.width,
            height: full.height,
        }
    };
    match kind {
        TransitionKind::Cut => vec![to(full, 1.0, None)],
        TransitionKind::CrossDissolve => vec![from(full), to(full, p, None)],
        TransitionKind::FadeThroughBlack | TransitionKind::FadeThroughWhite => {
            let rgb = if kind == TransitionKind::FadeThroughWhite {
                [255, 255, 255]
            } else {
                [0, 0, 0]
            };
            if p < 0.5 {
                vec![
                    from(full),
                    Paint::Solid {
                        rgb,
                        alpha: p * 2.0,
                    },
                ]
            } else {
                vec![
                    Paint::Solid { rgb, alpha: 1.0 },
                    to(full, (p - 0.5) * 2.0, None),
                ]
            }
        }
        TransitionKind::SlideLeft
        | TransitionKind::SlideRight
        | TransitionKind::SlideUp
        | TransitionKind::SlideDown => {
            let e = f64::from(ease(p));
            let (fw, fh) = (i64::from(w), i64::from(h));
            #[allow(clippy::cast_possible_truncation, clippy::cast_precision_loss)]
            let shift = |len: i64| (len as f64 * e).round() as i64;
            let (fx, fy, tx, ty) = match kind {
                TransitionKind::SlideLeft => (-shift(fw), 0, fw - shift(fw), 0),
                TransitionKind::SlideRight => (shift(fw), 0, shift(fw) - fw, 0),
                TransitionKind::SlideUp => (0, -shift(fh), 0, fh - shift(fh)),
                _ => (0, shift(fh), 0, shift(fh) - fh),
            };
            vec![
                Paint::Solid {
                    rgb: [0, 0, 0],
                    alpha: 1.0,
                },
                from(shifted(fx, fy)),
                to(shifted(tx, ty), 1.0, None),
            ]
        }
        TransitionKind::WipeLeft
        | TransitionKind::WipeRight
        | TransitionKind::WipeUp
        | TransitionKind::WipeDown => {
            let e = ease(p);
            #[allow(
                clippy::cast_possible_truncation,
                clippy::cast_sign_loss,
                clippy::cast_precision_loss
            )]
            let edge = |len: u32| ((len as f32) * e).round() as u32;
            let scissor = match kind {
                TransitionKind::WipeLeft => (w - edge(w), 0, edge(w), h),
                TransitionKind::WipeRight => (0, 0, edge(w), h),
                TransitionKind::WipeUp => (0, h - edge(h), w, edge(h)),
                _ => (0, 0, w, edge(h)),
            };
            vec![from(full), to(full, 1.0, Some(scissor))]
        }
        TransitionKind::Zoom => {
            let e = ease(p);
            let scale = 0.6 + 0.4 * f64::from(e);
            vec![from(full), to(full.zoomed(scale, 0.0, 0.0, w, h), e, None)]
        }
    }
}

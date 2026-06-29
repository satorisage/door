//! The GPU sky — the animated atmosphere rendered as a single fullscreen WGSL
//! fragment shader (Iced's custom `shader` widget over its `wgpu` backend). This is
//! the *per-pixel* tier: a true radial glow, fbm-noise nebula, a sparse twinkling
//! starfield, and a drifting comet, all computed per pixel so soft falloffs are
//! genuinely smooth — none of the Mach-banding that flat-fill `canvas` stacking
//! (concentric circles, 2–3 stop gradients) produces on a light background.
//!
//! It mirrors `sky.rs` (the canvas port) in look and timing, but renders on the GPU.
//! Shared by `door-greeter` (live background) and `door-settings` (preview), so both
//! show identical atmosphere. Day/night is a uniform flag; colors flow from the
//! `Theme`, so re-theming re-tints the sky with no shader change.

use crate::Theme;
use iced::wgpu;
use iced::widget::shader::{self, Primitive};
use iced::{mouse, Color, Rectangle};

/// The shader program. Carries everything the fragment shader needs as plain data;
/// `from_theme` derives it all from a `Theme` so call sites stay one line.
#[derive(Debug, Clone, Copy)]
pub struct SkyShader {
    uniforms: Uniforms,
}

/// Uniform block — must match `struct U` in the WGSL below (std140 alignment: a
/// `vec2 + 2×f32` fills the first 16 bytes, then nine 16-byte `vec4`s).
#[repr(C)]
#[derive(Debug, Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
struct Uniforms {
    res: [f32; 2],
    time: f32,
    fade: f32,
    bg_top: [f32; 4],
    bg_bot: [f32; 4],
    glow: [f32; 4],
    comet: [f32; 4],
    star: [f32; 4],
    /// x = day flag (0/1), y = comet intensity, z = star intensity, w = glow strength.
    params: [f32; 4],
    /// x = star threshold, y = twinkle-speed mult, z = comet period (s), w = cloud amount.
    params2: [f32; 4],
    /// x = cloud speed, y = glow falloff, z = nebula amount, w = comet tail decay.
    params3: [f32; 4],
    /// x = sun_x (0–1), y = sun_y (0–1), z = sun halo radius, w = sun intensity.
    params4: [f32; 4],
}

fn srgb8(r: u8, g: u8, b: u8) -> Color {
    Color {
        r: r as f32 / 255.0,
        g: g as f32 / 255.0,
        b: b as f32 / 255.0,
        a: 1.0,
    }
}

/// Mix two colors in sRGB space (good enough for tinting the gradient stops).
fn mix(a: Color, b: Color, t: f32) -> Color {
    Color {
        r: a.r + (b.r - a.r) * t,
        g: a.g + (b.g - a.g) * t,
        b: a.b + (b.b - a.b) * t,
        a: 1.0,
    }
}

impl SkyShader {
    /// Build the atmosphere from a theme. `anim` is the continuously-advancing clock
    /// (seconds); `fade` (0–1) ramps the whole thing in on launch.
    pub fn from_theme(t: &Theme, anim: f32, fade: f32) -> Self {
        let day = t.is_day;
        let bg = t.background.iced();
        // The glow / sun-haze tint — themeable per variant (sky-blue by day, indigo by
        // night by default), so cranking `sky_glow` recolors the whole atmosphere.
        let glow_tint = t.glow_color.iced();
        // Vertical gradient: the top lifts toward the glow tint, the bottom is the
        // solid background — a smooth per-pixel sky, not three flat bands.
        let bg_top = mix(bg, glow_tint, if day { 0.40 } else { 0.20 });
        let bg_bot = bg;
        // Stars: cooler/brighter at night, a faint blue sparkle by day.
        let star = if day {
            srgb8(0x3a, 0x5f, 0xb0)
        } else {
            t.foreground.iced()
        };
        let comet = t.comet_color.iced();

        let uniforms = Uniforms {
            res: [1.0, 1.0], // set per-frame from bounds in `draw`
            time: anim,
            fade,
            bg_top: bg_top.into_linear(),
            bg_bot: bg_bot.into_linear(),
            glow: glow_tint.into_linear(),
            comet: comet.into_linear(),
            star: star.into_linear(),
            params: [
                if day { 1.0 } else { 0.0 },
                if t.comet_enabled { 1.0 } else { 0.0 },
                if day { 0.45 } else { 0.95 },
                t.sky_glow,
            ],
            params2: [
                0.97 - 0.14 * t.star_density, // density 0–1 → twinkle threshold
                t.star_twinkle,
                t.comet_interval.max(3.0), // guard: period must exceed the 2.5s pause
                t.cloud_amount,
            ],
            params3: [
                t.cloud_speed,
                t.glow_falloff,
                t.nebula_amount,
                t.comet_tail_decay,
            ],
            params4: [t.sun_x, t.sun_y, t.sun_size, t.sun_intensity],
        };
        Self { uniforms }
    }
}

impl<Message> shader::Program<Message> for SkyShader {
    type State = ();
    type Primitive = SkyPrimitive;

    fn draw(&self, _state: &(), _cursor: mouse::Cursor, bounds: Rectangle) -> SkyPrimitive {
        let mut u = self.uniforms;
        u.res = [bounds.width.max(1.0), bounds.height.max(1.0)];
        SkyPrimitive { u }
    }
}

/// One frame's worth of GPU work — just the uniforms; all the art is in the shader.
#[derive(Debug)]
pub struct SkyPrimitive {
    u: Uniforms,
}

impl Primitive for SkyPrimitive {
    type Pipeline = SkyPipeline;

    fn prepare(
        &self,
        pipeline: &mut SkyPipeline,
        _device: &wgpu::Device,
        queue: &wgpu::Queue,
        _bounds: &Rectangle,
        _viewport: &shader::Viewport,
    ) {
        queue.write_buffer(&pipeline.uniforms, 0, bytemuck::bytes_of(&self.u));
    }

    fn render(
        &self,
        pipeline: &SkyPipeline,
        encoder: &mut wgpu::CommandEncoder,
        target: &wgpu::TextureView,
        clip_bounds: &Rectangle<u32>,
    ) {
        let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some("door sky"),
            color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                view: target,
                depth_slice: None,
                resolve_target: None,
                // Load (don't clear) — the sky is the bottom layer; it paints opaque.
                ops: wgpu::Operations {
                    load: wgpu::LoadOp::Load,
                    store: wgpu::StoreOp::Store,
                },
            })],
            depth_stencil_attachment: None,
            timestamp_writes: None,
            occlusion_query_set: None,
        });
        pass.set_scissor_rect(
            clip_bounds.x,
            clip_bounds.y,
            clip_bounds.width,
            clip_bounds.height,
        );
        pass.set_viewport(
            clip_bounds.x as f32,
            clip_bounds.y as f32,
            clip_bounds.width as f32,
            clip_bounds.height as f32,
            0.0,
            1.0,
        );
        pass.set_pipeline(&pipeline.pipeline);
        pass.set_bind_group(0, Some(&pipeline.bind_group), &[]);
        pass.draw(0..3, 0..1);
    }
}

/// The shared GPU pipeline for every `SkyPrimitive` (built once, lazily).
#[derive(Debug)]
pub struct SkyPipeline {
    pipeline: wgpu::RenderPipeline,
    uniforms: wgpu::Buffer,
    bind_group: wgpu::BindGroup,
}

impl shader::Pipeline for SkyPipeline {
    fn new(device: &wgpu::Device, _queue: &wgpu::Queue, format: wgpu::TextureFormat) -> Self {
        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("door sky shader"),
            source: wgpu::ShaderSource::Wgsl(WGSL.into()),
        });
        let uniforms = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("door sky uniforms"),
            size: std::mem::size_of::<Uniforms>() as u64,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        let bgl = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("door sky bgl"),
            entries: &[wgpu::BindGroupLayoutEntry {
                binding: 0,
                visibility: wgpu::ShaderStages::FRAGMENT,
                ty: wgpu::BindingType::Buffer {
                    ty: wgpu::BufferBindingType::Uniform,
                    has_dynamic_offset: false,
                    min_binding_size: None,
                },
                count: None,
            }],
        });
        let bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("door sky bind group"),
            layout: &bgl,
            entries: &[wgpu::BindGroupEntry {
                binding: 0,
                resource: uniforms.as_entire_binding(),
            }],
        });
        let layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("door sky layout"),
            bind_group_layouts: &[&bgl],
            push_constant_ranges: &[],
        });
        let pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("door sky pipeline"),
            layout: Some(&layout),
            vertex: wgpu::VertexState {
                module: &shader,
                entry_point: Some("vs_main"),
                compilation_options: wgpu::PipelineCompilationOptions::default(),
                buffers: &[],
            },
            primitive: wgpu::PrimitiveState::default(),
            depth_stencil: None,
            multisample: wgpu::MultisampleState::default(),
            fragment: Some(wgpu::FragmentState {
                module: &shader,
                entry_point: Some("fs_main"),
                compilation_options: wgpu::PipelineCompilationOptions::default(),
                targets: &[Some(wgpu::ColorTargetState {
                    format,
                    blend: Some(wgpu::BlendState::ALPHA_BLENDING),
                    write_mask: wgpu::ColorWrites::ALL,
                })],
            }),
            multiview: None,
            cache: None,
        });
        Self {
            pipeline,
            uniforms,
            bind_group,
        }
    }
}

// ───────────────────────────── the comet spinner ─────────────────────────────
//
// The card's default emblem: a comet orbiting a faint ring, rendered per pixel so
// the head's bloom and the trail's taper are genuinely smooth (the flat-fill canvas
// version beads and bands). White-hot head + coma halo + a 4-point star glint +
// shed sparkles, all tied to the `spinner_*` theme controls. Premultiplied-additive
// blending (its own pipeline) so highlights pile toward white on the dark night
// card, while the day card (glow 0) stays a crisp pure-comet streak.

/// GPU comet spinner — drop-in replacement for `sky::Spinner`, same theme inputs.
#[derive(Debug, Clone, Copy)]
pub struct SpinnerShader {
    uniforms: SpinUniforms,
}

/// Uniform block — must match `struct U` in `SPIN_WGSL` (64 bytes, std140).
#[repr(C)]
#[derive(Debug, Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
struct SpinUniforms {
    res: [f32; 2],
    time: f32,
    fade: f32,
    comet: [f32; 4],
    track: [f32; 4],
    /// x = glow, y = trail length, z = speed (rad/s), w = pulse-speed mult.
    params: [f32; 4],
    /// x = orbit-ring intensity, y/z/w = unused.
    params2: [f32; 4],
}

impl SpinnerShader {
    /// Build the spinner from a theme. `anim` is the shared clock (seconds); `fade`
    /// (0–1) ramps it in on launch.
    pub fn from_theme(t: &Theme, anim: f32, fade: f32) -> Self {
        let uniforms = SpinUniforms {
            res: [1.0, 1.0],
            time: anim,
            fade,
            comet: t.spinner_comet.iced().into_linear(),
            track: t.spinner_track.iced().into_linear(),
            params: [
                t.spinner_glow,
                t.spinner_trail,
                t.spinner_speed,
                t.spinner_pulse,
            ],
            params2: [t.spinner_ring, 0.0, 0.0, 0.0],
        };
        Self { uniforms }
    }
}

impl<Message> shader::Program<Message> for SpinnerShader {
    type State = ();
    type Primitive = SpinPrimitive;

    fn draw(&self, _state: &(), _cursor: mouse::Cursor, bounds: Rectangle) -> SpinPrimitive {
        let mut u = self.uniforms;
        u.res = [bounds.width.max(1.0), bounds.height.max(1.0)];
        SpinPrimitive { u }
    }
}

#[derive(Debug)]
pub struct SpinPrimitive {
    u: SpinUniforms,
}

impl Primitive for SpinPrimitive {
    type Pipeline = SpinPipeline;

    fn prepare(
        &self,
        pipeline: &mut SpinPipeline,
        _device: &wgpu::Device,
        queue: &wgpu::Queue,
        _bounds: &Rectangle,
        _viewport: &shader::Viewport,
    ) {
        queue.write_buffer(&pipeline.uniforms, 0, bytemuck::bytes_of(&self.u));
    }

    fn render(
        &self,
        pipeline: &SpinPipeline,
        encoder: &mut wgpu::CommandEncoder,
        target: &wgpu::TextureView,
        clip_bounds: &Rectangle<u32>,
    ) {
        let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some("door spinner"),
            color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                view: target,
                depth_slice: None,
                resolve_target: None,
                // Load — the emblem composites over the already-drawn card.
                ops: wgpu::Operations {
                    load: wgpu::LoadOp::Load,
                    store: wgpu::StoreOp::Store,
                },
            })],
            depth_stencil_attachment: None,
            timestamp_writes: None,
            occlusion_query_set: None,
        });
        pass.set_scissor_rect(
            clip_bounds.x,
            clip_bounds.y,
            clip_bounds.width,
            clip_bounds.height,
        );
        pass.set_viewport(
            clip_bounds.x as f32,
            clip_bounds.y as f32,
            clip_bounds.width as f32,
            clip_bounds.height as f32,
            0.0,
            1.0,
        );
        pass.set_pipeline(&pipeline.pipeline);
        pass.set_bind_group(0, Some(&pipeline.bind_group), &[]);
        pass.draw(0..3, 0..1);
    }
}

#[derive(Debug)]
pub struct SpinPipeline {
    pipeline: wgpu::RenderPipeline,
    uniforms: wgpu::Buffer,
    bind_group: wgpu::BindGroup,
}

impl shader::Pipeline for SpinPipeline {
    fn new(device: &wgpu::Device, _queue: &wgpu::Queue, format: wgpu::TextureFormat) -> Self {
        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("door spinner shader"),
            source: wgpu::ShaderSource::Wgsl(SPIN_WGSL.into()),
        });
        let uniforms = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("door spinner uniforms"),
            size: std::mem::size_of::<SpinUniforms>() as u64,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        let bgl = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("door spinner bgl"),
            entries: &[wgpu::BindGroupLayoutEntry {
                binding: 0,
                visibility: wgpu::ShaderStages::FRAGMENT,
                ty: wgpu::BindingType::Buffer {
                    ty: wgpu::BufferBindingType::Uniform,
                    has_dynamic_offset: false,
                    min_binding_size: None,
                },
                count: None,
            }],
        });
        let bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("door spinner bind group"),
            layout: &bgl,
            entries: &[wgpu::BindGroupEntry {
                binding: 0,
                resource: uniforms.as_entire_binding(),
            }],
        });
        let layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("door spinner layout"),
            bind_group_layouts: &[&bgl],
            push_constant_ranges: &[],
        });
        // Premultiplied-additive over: src already carries its coverage, highlights
        // accumulate beyond 1.0 toward white (the glow), and it composites cleanly
        // over the transparent regions of the emblem onto the card behind.
        let blend = wgpu::BlendState {
            color: wgpu::BlendComponent {
                src_factor: wgpu::BlendFactor::One,
                dst_factor: wgpu::BlendFactor::OneMinusSrcAlpha,
                operation: wgpu::BlendOperation::Add,
            },
            alpha: wgpu::BlendComponent {
                src_factor: wgpu::BlendFactor::One,
                dst_factor: wgpu::BlendFactor::OneMinusSrcAlpha,
                operation: wgpu::BlendOperation::Add,
            },
        };
        let pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("door spinner pipeline"),
            layout: Some(&layout),
            vertex: wgpu::VertexState {
                module: &shader,
                entry_point: Some("vs_main"),
                compilation_options: wgpu::PipelineCompilationOptions::default(),
                buffers: &[],
            },
            primitive: wgpu::PrimitiveState::default(),
            depth_stencil: None,
            multisample: wgpu::MultisampleState::default(),
            fragment: Some(wgpu::FragmentState {
                module: &shader,
                entry_point: Some("fs_main"),
                compilation_options: wgpu::PipelineCompilationOptions::default(),
                targets: &[Some(wgpu::ColorTargetState {
                    format,
                    blend: Some(blend),
                    write_mask: wgpu::ColorWrites::ALL,
                })],
            }),
            multiview: None,
            cache: None,
        });
        Self {
            pipeline,
            uniforms,
            bind_group,
        }
    }
}

/// The comet spinner, per pixel. A fullscreen triangle feeds a fragment stage that
/// accumulates, premultiplied: faint orbit ring → luminous arc trail → coma halo →
/// white-hot nucleus → 4-point star glint → shed sparkles. `hot = clamp(glow)` ties
/// the whiteness/bloom to the glow control so day (glow 0) stays a crisp comet.
const SPIN_WGSL: &str = r#"
struct U {
  res: vec2<f32>,
  time: f32,
  fade: f32,
  comet: vec4<f32>,
  track: vec4<f32>,
  params: vec4<f32>,
  params2: vec4<f32>,
};
@group(0) @binding(0) var<uniform> u: U;

struct VsOut {
  @builtin(position) pos: vec4<f32>,
  @location(0) uv: vec2<f32>,
};

@vertex
fn vs_main(@builtin(vertex_index) vi: u32) -> VsOut {
  var tri = array<vec2<f32>, 3>(vec2(-1.0, -1.0), vec2(3.0, -1.0), vec2(-1.0, 3.0));
  let xy = tri[vi];
  var out: VsOut;
  out.pos = vec4(xy, 0.0, 1.0);
  out.uv = vec2((xy.x + 1.0) * 0.5, (1.0 - xy.y) * 0.5);
  return out;
}

const TAU: f32 = 6.2831853;

@fragment
fn fs_main(in: VsOut) -> @location(0) vec4<f32> {
  let aspect = u.res.x / max(u.res.y, 1.0);
  var p = in.uv - vec2(0.5, 0.5);
  p.x = p.x * aspect;                 // circular orbit regardless of bounds

  let glow = u.params.x;
  let trailLen = clamp(u.params.y, 0.15, 1.0);
  let speed = u.params.z;
  let hot = clamp(glow, 0.0, 1.0);
  let white = vec3(1.0, 1.0, 1.0);
  let cometc = u.comet.rgb;
  let pulse = 0.86 + 0.14 * sin(u.time * 3.0 * u.params.w);

  let R = 0.30;                       // orbit radius — pulled in so the glow has margin
  let r = length(p);
  let ang = atan2(p.y, p.x);
  let headA = u.time * speed;
  let head = vec2(cos(headA), sin(headA)) * R;
  let lp = p - head;
  let nd = dot(lp, lp);

  var acc = vec3(0.0);                // premultiplied color
  var cov = 0.0;                      // coverage / alpha

  // Faint orbit ring the comet rides.
  let ringI = exp(-(r - R) * (r - R) * 1900.0) * u.params2.x;
  acc += u.track.rgb * ringI;
  cov += ringI;

  // Comet tail — a smooth exponential fade by angular distance behind the head (no
  // hard cutoff, so no seam and no donut). `trailLen` sets the reach: a short crisp
  // stub up to a tail fading over ~half the ring. Pinched radially to the orbit;
  // whitened only on the hot near-head segment when `glow` is up.
  var d = headA - ang;
  d = d - floor(d / TAU) * TAU;       // angle behind the head, [0, TAU)
  let decay = mix(6.5, 1.4, (trailLen - 0.15) / 0.85);
  let radial = exp(-(r - R) * (r - R) * 1200.0);
  let trailI = radial * exp(-d * decay) * 0.95;
  let kw = clamp(1.0 - d / 1.2, 0.0, 1.0);
  let trailCol = mix(cometc, white, hot * kw * kw * 0.85);
  acc += trailCol * trailI;
  cov += trailI;

  // Coma halo (night only — broad soft bloom; banded on the light day card).
  if (glow > 0.0) {
    let halo = (exp(-nd * 150.0) * 0.5 + exp(-nd * 60.0) * 0.30 + exp(-nd * 30.0) * 0.16)
               * glow * pulse;
    acc += mix(cometc, white, hot * 0.5) * halo;
    cov += halo;
  }

  // Bright nucleus + a tiny white center pip.
  let core = exp(-nd * 700.0) * pulse;
  acc += mix(cometc, white, hot * 0.85) * core;
  cov += core;
  let pip = exp(-nd * 2400.0) * (0.35 + 0.55 * hot) * pulse;
  acc += white * pip;
  cov += pip;

  // 4-point star glint over the head — spikes lengthen with `glow`.
  let longf = mix(40.0, 28.0, hot);
  let horiz = exp(-lp.y * lp.y * 5200.0) * exp(-abs(lp.x) * longf);
  let vert  = exp(-lp.x * lp.x * 5200.0) * exp(-abs(lp.y) * longf);
  let glint = (horiz + vert) * (0.22 + 0.4 * hot) * pulse;
  acc += mix(cometc, white, 0.4 + 0.5 * hot) * glint;
  cov += glint;

  // Fade out *radially* (a circle, never a square) so the orbiting glow reads as a
  // free-floating spinner, not something happening inside a box. The orbit (0.30) and
  // its glow sit inside 0.42, so this only softens the faint outer tail.
  let edge = 1.0 - smoothstep(0.42, 0.49, r);
  acc = acc * edge;
  cov = cov * edge;

  acc = acc * u.fade;
  cov = clamp(cov, 0.0, 1.0) * u.fade;
  return vec4(acc, cov);
}
"#;

/// The atmosphere, per pixel. A fullscreen triangle (vertex stage) feeds a fragment
/// stage that composites: smooth vertical gradient → true radial glow → fbm nebula →
/// sparse twinkling stars → drifting comet → ordered dither (kills 8-bit banding).
const WGSL: &str = r#"
struct U {
  res: vec2<f32>,
  time: f32,
  fade: f32,
  bg_top: vec4<f32>,
  bg_bot: vec4<f32>,
  glow: vec4<f32>,
  comet: vec4<f32>,
  star: vec4<f32>,
  params: vec4<f32>,
  params2: vec4<f32>,
  params3: vec4<f32>,
  params4: vec4<f32>,
};
@group(0) @binding(0) var<uniform> u: U;

struct VsOut {
  @builtin(position) pos: vec4<f32>,
  @location(0) uv: vec2<f32>,
};

@vertex
fn vs_main(@builtin(vertex_index) vi: u32) -> VsOut {
  var tri = array<vec2<f32>, 3>(vec2(-1.0, -1.0), vec2(3.0, -1.0), vec2(-1.0, 3.0));
  let xy = tri[vi];
  var out: VsOut;
  out.pos = vec4(xy, 0.0, 1.0);
  out.uv = vec2((xy.x + 1.0) * 0.5, (1.0 - xy.y) * 0.5);
  return out;
}

fn hash21(p: vec2<f32>) -> f32 {
  var q = fract(p * vec2(123.34, 345.45));
  q += dot(q, q + 34.345);
  return fract(q.x * q.y);
}

fn noise2(p: vec2<f32>) -> f32 {
  let i = floor(p);
  let f = fract(p);
  let a = hash21(i);
  let b = hash21(i + vec2(1.0, 0.0));
  let c = hash21(i + vec2(0.0, 1.0));
  let d = hash21(i + vec2(1.0, 1.0));
  let w = f * f * (3.0 - 2.0 * f);
  return mix(mix(a, b, w.x), mix(c, d, w.x), w.y);
}

fn fbm(p0: vec2<f32>) -> f32 {
  var p = p0;
  var v = 0.0;
  var amp = 0.5;
  for (var i = 0; i < 5; i = i + 1) {
    v = v + amp * noise2(p);
    p = p * 2.0;
    amp = amp * 0.5;
  }
  return v;
}

fn ease(x: f32) -> f32 { return 0.5 * (1.0 - cos(3.14159265 * x)); }

// One drifting, sun-lit cloud layer. A cheap single-octave domain warp feeds an fbm
// density; a second fbm sampled a step toward the sun gives soft self-shadowing, so
// the clouds read as volumetric (bright sun-side, cool underside) rather than flat.
fn cloud_layer(col: vec3<f32>, p: vec2<f32>, sundir: vec2<f32>, cscale: f32, wscale: f32,
               spd: f32, alpha: f32, lo: f32, hi: f32, lit: vec3<f32>, sh: vec3<f32>) -> vec3<f32> {
  let drift = vec2(u.time * spd, u.time * spd * 0.25);
  let warp = noise2(p * wscale + drift);
  let n  = fbm(p * cscale + vec2<f32>(warp * 1.6) + drift);
  let ns = fbm((p + sundir * 0.04) * cscale + vec2<f32>(warp * 1.6) + drift);
  let density = smoothstep(lo, hi, n);
  let light = clamp((n - ns) * 8.0 + 0.5, 0.0, 1.0);
  return mix(col, mix(sh, lit, light), density * alpha);
}

// The daytime atmosphere: a zenith→horizon haze gradient, a bloomed sun, two parallax
// layers of sun-lit drifting cloud, and a soft horizon haze. Colors flow from the
// theme (zenith lifts the background toward the sky-blue glow tint), so re-theming
// re-tints the whole sky. This is the per-pixel "3D" tier for the light variant.
fn day_sky(uv: vec2<f32>, p: vec2<f32>, aspect: f32) -> vec3<f32> {
  let bg = u.bg_bot.rgb;
  let zenith = mix(bg, u.glow.rgb, 0.60);
  let horizon = mix(bg, vec3(1.0, 1.0, 1.0), 0.30);
  let height = 1.0 - uv.y;
  var col = mix(horizon, zenith, smoothstep(0.0, 1.0, height));

  // Sun — position (params4.xy, 0–1 UV), halo radius (params4.z) and intensity
  // (params4.w) are theme knobs; the warm tint + near-white core stay fixed for now.
  // `size` is a radius (bigger = wider): the gaussian is 1/size² so size 0.25
  // reproduces the prior falloff of 16, and the halo stays contained, not a wash.
  var sp = vec2(u.params4.x, u.params4.y) - vec2(0.5, 0.5);
  sp.x = sp.x * aspect;
  let sd = length(p - sp);
  let ssz = max(u.params4.z, 0.02);
  col = col + vec3(1.0, 0.91, 0.69) * exp(-sd * sd / (ssz * ssz)) * u.params4.w;
  col = mix(col, vec3(1.0, 0.99, 0.96), smoothstep(0.045, 0.028, sd) * 0.85);
  let sundir = normalize(sp);

  // Two parallax cloud layers, far/soft then near/detailed.
  let lit = vec3(1.0, 1.0, 1.0);
  let sh = vec3(0.706, 0.761, 0.859);
  let cspd = u.params3.x;       // cloud speed mult
  let camt = u.params2.w;       // cloud amount mult
  col = cloud_layer(col, p, sundir, 3.0, 1.5, 0.013 * cspd, 0.65 * camt, 0.40, 0.74, lit, sh);
  col = cloud_layer(col, p, sundir, 5.4, 2.4, 0.027 * cspd, 0.50 * camt, 0.46, 0.82, lit, sh);

  // Soft haze thickening toward the horizon.
  col = mix(col, horizon, smoothstep(0.35, 0.0, height) * 0.25);
  return col;
}

@fragment
fn fs_main(in: VsOut) -> @location(0) vec4<f32> {
  let uv = in.uv;
  let aspect = u.res.x / max(u.res.y, 1.0);
  var p = uv - vec2(0.5, 0.5);
  p.x = p.x * aspect;
  let day = u.params.x;

  var col: vec3<f32>;
  if (day > 0.5) {
    col = day_sky(uv, p, aspect);
  } else {
    // Night: smooth vertical gradient (per pixel — no banding).
    col = mix(u.bg_top.rgb, u.bg_bot.rgb, smoothstep(0.0, 1.0, uv.y));

    // True radial glow / sun-haze.
    let gc = vec2(0.0, -0.06);
    let gd = length(p - gc);
    col = col + u.glow.rgb * exp(-gd * gd * u.params3.y) * u.params.w;

    // fbm nebula for depth, concentrated near the glow.
    let neb = fbm(p * 2.2 + vec2(u.time * 0.015, u.time * -0.010));
    col = col + u.glow.rgb * neb * u.params3.z * smoothstep(0.95, 0.0, gd);

    // Twinkling stars across three scaled layers — denser, each with its own pulse
    // rate and a sharp (cubed) twinkle so they sparkle rather than throb, and the
    // brightest carry a faint cross-glint.
    var stars = 0.0;
    var spark = 0.0;
    for (var l = 0; l < 3; l = l + 1) {
      let scale = 8.0 * pow(1.9, f32(l));
      let gp = uv * vec2(aspect, 1.0) * scale;
      let cell = floor(gp);
      let thr = u.params2.x;
      let h = hash21(cell + f32(l) * 17.0);
      if (h > thr) {
        let h2 = hash21(cell + f32(l) * 17.0 + 5.0);
        let cp = fract(gp) - vec2(0.5, 0.5);
        var tw = 0.5 + 0.5 * sin(u.time * (0.8 + h2 * 3.6) * u.params2.y + h * 40.0);
        tw = tw * tw * tw;
        let bright = (h - thr) / max(1.0 - thr, 0.01);
        stars = stars + exp(-dot(cp, cp) * 70.0) * tw * bright;
        let cross = exp(-abs(cp.x) * 55.0) * exp(-cp.y * cp.y * 900.0)
                  + exp(-abs(cp.y) * 55.0) * exp(-cp.x * cp.x * 900.0);
        spark = spark + cross * tw * bright * 0.5;
      }
    }
    col = col + u.star.rgb * stars * u.params.z;
    col = col + vec3(1.0, 1.0, 1.0) * spark * 0.5 * u.params.z;
  }

  // Drifting comet: upper-right -> lower-left on an InOutSine ease, then pause.
  let period = u.params2.z;
  let cu = fract(u.time / period);
  let sf = (period - 2.5) / period;   // a fixed ~2.5s pause after each sweep
  if (cu < sf) {
    let pp = ease(cu / sf);
    var head = vec2(mix(1.08, -0.22, pp), mix(0.10, 0.65, pp));
    head = head - vec2(0.5, 0.5);
    head.x = head.x * aspect;
    let dir = normalize(vec2(0.92, -0.39)); // tail recedes up-right
    let rel = p - head;
    let along = dot(rel, dir);
    let perp = length(rel - along * dir);
    // Teardrop tail: narrow at the head, fanning out and fading along the heading —
    // not a constant-width straight beam.
    let t = max(along, 0.0);
    let width = 0.010 + t * 0.10;
    let tail = exp(-(perp * perp) / (width * width)) * exp(-t * u.params3.w) * step(0.0, along);
    let nuc = exp(-dot(rel, rel) * 3000.0);   // white-hot nucleus
    let coma = exp(-dot(rel, rel) * 300.0);   // soft round coma
    let ccol = mix(u.comet.rgb, vec3(1.0, 1.0, 1.0), nuc * 0.7);
    col = col + ccol * (nuc * 1.5 + coma * 0.45 + tail * 0.85) * u.params.y;
  }

  // Ordered dither — one sub-LSB of noise so 8-bit targets never band.
  col = col + vec3((hash21(in.pos.xy) - 0.5) / 255.0);

  col = col * u.fade;
  return vec4(col, 1.0);
}
"#;

#[cfg(test)]
mod wgsl_tests {
    use super::{SPIN_WGSL, WGSL};

    /// Parse + validate both shaders through naga (the same front-end wgpu uses), so a
    /// WGSL typo is a failed test, not a runtime pipeline panic on the login screen.
    fn check(label: &str, src: &str) {
        let module = naga::front::wgsl::parse_str(src)
            .unwrap_or_else(|e| panic!("{label} WGSL failed to parse: {e}"));
        let mut validator = naga::valid::Validator::new(
            naga::valid::ValidationFlags::all(),
            naga::valid::Capabilities::all(),
        );
        validator
            .validate(&module)
            .unwrap_or_else(|e| panic!("{label} WGSL failed to validate: {e}"));
    }

    #[test]
    fn sky_shader_is_valid_wgsl() {
        check("sky", WGSL);
    }

    #[test]
    fn spinner_shader_is_valid_wgsl() {
        check("spinner", SPIN_WGSL);
    }
}

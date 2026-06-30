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
    /// Daytime sun halo tint (rgb; a unused).
    sun_col: [f32; 4],
    /// x = night glow center x (0–1), y = glow center y (0–1), z = day haze,
    /// w = sky mode (0 = auto/day-night, 1 = aurora).
    params5: [f32; 4],
    /// x = star layers, y = star size mult, z = nebula drift speed, w unused.
    params6: [f32; 4],
    /// x = comet tilt (rad), y = comet pause (s), z = comet width mult, w unused.
    params7: [f32; 4],
    /// Daytime cloud sun-lit color (rgb).
    cloud_lit: [f32; 4],
    /// Daytime cloud shadowed color (rgb).
    cloud_shadow: [f32; 4],
    /// x = cursor dx (−0.5..0.5 of bounds), y = cursor dy, z = parallax strength, w unused.
    params8: [f32; 4],
    /// x = film-grain strength, y = vignette strength, z = card corner radius (px,
    /// for the frost mask), w unused.
    params9: [f32; 4],
    /// Frosted-card backdrop rect in screen-normalized coords: x, y, w, h. Set per
    /// frame in the frost primitive's prepare(); unused by the full-screen sky pass.
    frost: [f32; 4],
    /// Scene-param pool (M8) — the active `sky_mode` reinterprets these slots as its
    /// own controls (scenes are mutually exclusive). Synthwave mapping:
    ///   scene_a = [grid speed, grid density, grid perspective, grid glow]
    ///   scene_b = [sun size, sun stripes, sun bloom, horizon]
    ///   scene_c1/c2/c3 = grid / sky-top / sky-bottom colours (rgb; a unused)
    scene_a: [f32; 4],
    scene_b: [f32; 4],
    scene_c1: [f32; 4],
    scene_c2: [f32; 4],
    scene_c3: [f32; 4],
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
        // Pack the active scene's authoring dials into the shared scene-param pool
        // (M8) — keyed by the resolved sky_mode (load_at resolves Seasonal first).
        // Scene colours stay sRGB-nominal (raw `.iced()`, not linearized) to match the
        // shaders' original literals. Modes with no per-scene controls pack zeros.
        let nom = |c: crate::Color| -> [f32; 4] {
            let i = c.iced();
            [i.r, i.g, i.b, 0.0]
        };
        let z = [0.0f32; 4];
        let (scene_a, scene_b, scene_c1, scene_c2, scene_c3) = match t.sky_mode {
            crate::SkyMode::Synthwave => (
                [
                    t.synthwave_grid_speed,
                    t.synthwave_grid_density,
                    t.synthwave_grid_perspective,
                    t.synthwave_grid_glow,
                ],
                [
                    t.synthwave_sun_size,
                    t.synthwave_sun_stripes,
                    t.synthwave_sun_bloom,
                    t.synthwave_horizon,
                ],
                nom(t.synthwave_grid_color),
                nom(t.synthwave_sky_top),
                nom(t.synthwave_sky_bottom),
            ),
            crate::SkyMode::Storm => (
                [
                    t.storm_lightning_rate,
                    t.storm_strike_chance,
                    t.storm_cloud_density,
                    0.0,
                ],
                z,
                nom(t.storm_bolt_color),
                nom(t.storm_flash_color),
                z,
            ),
            crate::SkyMode::Rain => (
                [
                    t.rain_fall_speed,
                    t.rain_density,
                    t.rain_slant,
                    t.rain_intensity,
                ],
                z,
                nom(t.rain_color),
                z,
                z,
            ),
            crate::SkyMode::Snow => (
                [
                    t.snow_fall_speed,
                    t.snow_density,
                    t.snow_sway,
                    t.snow_flake_size,
                ],
                z,
                nom(t.snow_color),
                z,
                z,
            ),
            crate::SkyMode::Fire => (
                [t.fire_rise_speed, t.fire_flame_height, 0.0, 0.0],
                z,
                nom(t.fire_flame_color),
                nom(t.fire_tip_color),
                z,
            ),
            _ => (z, z, z, z, z),
        };
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
            sun_col: t.sun_color.iced().into_linear(),
            params5: [t.glow_x, t.glow_y, t.day_haze, t.sky_mode.shader_id()],
            params6: [t.star_layers, t.star_size, t.nebula_speed, 0.0],
            params7: [t.comet_tilt, t.comet_pause, t.comet_width, 0.0],
            cloud_lit: t.cloud_lit.iced().into_linear(),
            cloud_shadow: t.cloud_shadow.iced().into_linear(),
            params8: [0.0, 0.0, t.cursor_parallax, t.glow_pulse], // xy set per-frame from the cursor; w = glow pulse
            params9: [t.grain, t.vignette, t.corner_radius, 0.0],
            frost: [0.0, 0.0, 1.0, 1.0], // set per frame in the frost primitive
            // Scene-param pool, packed above per the active sky_mode.
            scene_a,
            scene_b,
            scene_c1,
            scene_c2,
            scene_c3,
        };
        Self { uniforms }
    }
}

impl<Message> shader::Program<Message> for SkyShader {
    type State = ();
    type Primitive = SkyPrimitive;

    fn draw(&self, _state: &(), cursor: mouse::Cursor, bounds: Rectangle) -> SkyPrimitive {
        let mut u = self.uniforms;
        u.res = [bounds.width.max(1.0), bounds.height.max(1.0)];
        // Feed the cursor (normalized to −0.5..0.5 of the bounds) for starfield parallax.
        if let Some(pos) = cursor.position_in(bounds) {
            u.params8[0] = pos.x / bounds.width.max(1.0) - 0.5;
            u.params8[1] = pos.y / bounds.height.max(1.0) - 0.5;
        }
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

// ───────────────────────────── frosted card backdrop ─────────────────────────
//
// A blurred sample of the *same* procedural scene, drawn behind the (translucent) card
// and masked to its rounded rect — true backdrop blur. It reuses the sky uniforms +
// WGSL (the `fs_frost` entry), so the blur is the exact scene that sits behind the card.

/// The frosted backdrop program — same scene as `SkyShader`, blurred in `fs_frost`.
#[derive(Debug, Clone, Copy)]
pub struct FrostShader {
    uniforms: Uniforms,
}

impl FrostShader {
    /// Build from a theme (identical uniforms to the sky, so the blur matches it).
    pub fn from_theme(t: &Theme, anim: f32, fade: f32) -> Self {
        Self {
            uniforms: SkyShader::from_theme(t, anim, fade).uniforms,
        }
    }
}

impl<Message> shader::Program<Message> for FrostShader {
    type State = ();
    type Primitive = FrostPrimitive;

    fn draw(&self, _state: &(), _cursor: mouse::Cursor, _bounds: Rectangle) -> FrostPrimitive {
        // res + the card rect are filled in prepare() (it has the viewport + bounds).
        FrostPrimitive { u: self.uniforms }
    }
}

#[derive(Debug)]
pub struct FrostPrimitive {
    u: Uniforms,
}

impl Primitive for FrostPrimitive {
    type Pipeline = FrostPipeline;

    fn prepare(
        &self,
        pipeline: &mut FrostPipeline,
        _device: &wgpu::Device,
        queue: &wgpu::Queue,
        bounds: &Rectangle,
        viewport: &shader::Viewport,
    ) {
        let mut u = self.u;
        let vp = viewport.logical_size();
        u.res = [vp.width.max(1.0), vp.height.max(1.0)];
        // The card rect, normalized to the full surface, so fs_frost maps its local uv
        // to the global screen uv and the blur lines up with the sky behind the card.
        u.frost = [
            bounds.x / u.res[0],
            bounds.y / u.res[1],
            bounds.width / u.res[0],
            bounds.height / u.res[1],
        ];
        queue.write_buffer(&pipeline.uniforms, 0, bytemuck::bytes_of(&u));
    }

    fn render(
        &self,
        pipeline: &FrostPipeline,
        encoder: &mut wgpu::CommandEncoder,
        target: &wgpu::TextureView,
        clip_bounds: &Rectangle<u32>,
    ) {
        let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some("door frost"),
            color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                view: target,
                depth_slice: None,
                resolve_target: None,
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

/// The frost pipeline — the sky WGSL with the `fs_frost` fragment entry, alpha-blended.
#[derive(Debug)]
pub struct FrostPipeline {
    pipeline: wgpu::RenderPipeline,
    uniforms: wgpu::Buffer,
    bind_group: wgpu::BindGroup,
}

impl shader::Pipeline for FrostPipeline {
    fn new(device: &wgpu::Device, _queue: &wgpu::Queue, format: wgpu::TextureFormat) -> Self {
        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("door frost shader"),
            source: wgpu::ShaderSource::Wgsl(WGSL.into()),
        });
        let uniforms = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("door frost uniforms"),
            size: std::mem::size_of::<Uniforms>() as u64,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        let bgl = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("door frost bgl"),
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
            label: Some("door frost bind group"),
            layout: &bgl,
            entries: &[wgpu::BindGroupEntry {
                binding: 0,
                resource: uniforms.as_entire_binding(),
            }],
        });
        let layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("door frost layout"),
            bind_group_layouts: &[&bgl],
            push_constant_ranges: &[],
        });
        let pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("door frost pipeline"),
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
                entry_point: Some("fs_frost"),
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
    /// x = orbit-ring intensity, y = comet orbit radius, z/w unused.
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
            params2: [
                t.spinner_ring,
                t.spinner_orbit,
                t.spinner_style.shader_id(),
                0.0,
            ],
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

// The signature comet emblem (style 0).
fn comet_spin(in: VsOut) -> vec4<f32> {
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

  let R = max(u.params2.y, 0.05);     // orbit radius (themeable; pulled in for glow margin)
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

// Ring (style 1): a thin orbit ring with a bright arc trailing the rotating head.
fn ring_spin(in: VsOut) -> vec4<f32> {
  let aspect = u.res.x / max(u.res.y, 1.0);
  var p = in.uv - vec2(0.5, 0.5);
  p.x = p.x * aspect;
  let r = length(p);
  let R = max(u.params2.y, 0.05);
  let speed = u.params.z;
  let ang = atan2(p.y, p.x);
  let head = u.time * speed;
  var d = head - ang;
  d = d - floor(d / TAU) * TAU;
  let band = exp(-(r - R) * (r - R) * 2400.0);
  let arc = band * exp(-d * 1.6);
  let track = band * u.params2.x * 0.5;
  var acc = u.comet.rgb * arc + u.track.rgb * track;
  var cov = arc + track;
  let edge = 1.0 - smoothstep(0.42, 0.49, r);
  acc = acc * edge * u.fade;
  cov = clamp(cov, 0.0, 1.0) * edge * u.fade;
  return vec4(acc, cov);
}

// Dots (style 2): eight dots on the orbit, one chasing bright around the ring.
fn dots_spin(in: VsOut) -> vec4<f32> {
  let aspect = u.res.x / max(u.res.y, 1.0);
  var p = in.uv - vec2(0.5, 0.5);
  p.x = p.x * aspect;
  let R = max(u.params2.y, 0.05);
  let speed = u.params.z;
  var acc = vec3(0.0);
  var cov = 0.0;
  for (var i = 0; i < 8; i = i + 1) {
    let a = TAU * f32(i) / 8.0;
    let dp = vec2(cos(a), sin(a)) * R;
    let dd = p - dp;
    let dotg = exp(-dot(dd, dd) * 3000.0);
    let ph = fract(u.time * speed / TAU + f32(i) / 8.0);
    let br = 0.18 + 0.82 * pow(ph, 3.0);
    acc = acc + u.comet.rgb * dotg * br;
    cov = cov + dotg * br;
  }
  let r = length(p);
  let edge = 1.0 - smoothstep(0.42, 0.49, r);
  acc = acc * edge * u.fade;
  cov = clamp(cov, 0.0, 1.0) * edge * u.fade;
  return vec4(acc, cov);
}

// Pulse (style 3): a breathing core with a soft expanding ring.
fn pulse_spin(in: VsOut) -> vec4<f32> {
  let aspect = u.res.x / max(u.res.y, 1.0);
  var p = in.uv - vec2(0.5, 0.5);
  p.x = p.x * aspect;
  let r = length(p);
  let t = u.time * max(u.params.z, 0.1);
  let breath = 0.5 + 0.5 * sin(t * 2.0);
  let core = exp(-r * r * (900.0 + 1400.0 * (1.0 - breath)));
  let rr = fract(t * 0.4) * 0.34;
  let fade_ring = 1.0 - fract(t * 0.4);
  let ring = exp(-(r - rr) * (r - rr) * 1600.0) * fade_ring * 0.7;
  var acc = u.comet.rgb * (core + ring) + u.track.rgb * ring * 0.3;
  var cov = core + ring;
  let edge = 1.0 - smoothstep(0.42, 0.49, r);
  acc = acc * edge * u.fade;
  cov = clamp(cov, 0.0, 1.0) * edge * u.fade;
  return vec4(acc, cov);
}

@fragment
fn fs_main(in: VsOut) -> @location(0) vec4<f32> {
  let style = u.params2.z;
  if (style < 0.5) {
    return comet_spin(in);
  } else if (style < 1.5) {
    return ring_spin(in);
  } else if (style < 2.5) {
    return dots_spin(in);
  }
  return pulse_spin(in);
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
  sun_col: vec4<f32>,
  params5: vec4<f32>,
  params6: vec4<f32>,
  params7: vec4<f32>,
  cloud_lit: vec4<f32>,
  cloud_shadow: vec4<f32>,
  params8: vec4<f32>,
  params9: vec4<f32>,
  frost: vec4<f32>,
  scene_a: vec4<f32>,
  scene_b: vec4<f32>,
  scene_c1: vec4<f32>,
  scene_c2: vec4<f32>,
  scene_c3: vec4<f32>,
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
  col = col + u.sun_col.rgb * exp(-sd * sd / (ssz * ssz)) * u.params4.w;
  col = mix(col, vec3(1.0, 0.99, 0.96), smoothstep(0.045, 0.028, sd) * 0.85);
  let sundir = normalize(sp);

  // Two parallax cloud layers, far/soft then near/detailed.
  let lit = u.cloud_lit.rgb;
  let sh = u.cloud_shadow.rgb;
  let cspd = u.params3.x;       // cloud speed mult
  let camt = u.params2.w;       // cloud amount mult
  col = cloud_layer(col, p, sundir, 3.0, 1.5, 0.013 * cspd, 0.65 * camt, 0.40, 0.74, lit, sh);
  col = cloud_layer(col, p, sundir, 5.4, 2.4, 0.027 * cspd, 0.50 * camt, 0.46, 0.82, lit, sh);

  // Soft haze thickening toward the horizon.
  col = mix(col, horizon, smoothstep(0.35, 0.0, height) * u.params5.z);
  return col;
}

// Aurora borealis: flowing green→magenta curtains over a night sky, with a faint
// starfield behind. Each curtain is a wavy ribbon (fbm + sin waver) with a vertical
// "ray" striation scrolled over time and a slow brightness pulse. Colors lean on the
// theme background/glow for the night base so it sits in the chosen palette.
fn aurora_sky(uv: vec2<f32>, p: vec2<f32>, aspect: f32) -> vec3<f32> {
  let bg = u.bg_bot.rgb;
  let top = mix(bg, u.glow.rgb, 0.35);
  var col = mix(top, bg, smoothstep(0.0, 1.0, uv.y));

  // Faint stars behind the curtains.
  let gp = uv * vec2(aspect, 1.0) * 18.0;
  let cell = floor(gp);
  let h = hash21(cell);
  if (h > 0.93) {
    let cp = fract(gp) - vec2(0.5, 0.5);
    col = col + vec3(0.8, 0.85, 1.0) * exp(-dot(cp, cp) * 80.0) * (h - 0.93) * 12.0;
  }

  // Curtains.
  let green = vec3(0.18, 1.0, 0.66);
  let magenta = vec3(0.62, 0.30, 1.0);
  var tint = vec3(0.0);
  for (var b = 0; b < 3; b = b + 1) {
    let fb = f32(b);
    let ph = u.time * (0.12 + fb * 0.03) + fb * 2.0;
    // Wavy top edge of the curtain; light hangs *downward* from it and fades, so it
    // reads as a vertical sheet rather than a horizontal ribbon.
    let topEdge = 0.22 + fb * 0.09
                + 0.05 * sin(uv.x * 3.0 + ph)
                + 0.07 * fbm(vec2(uv.x * 1.5 + fb, ph * 0.5));
    let below = uv.y - topEdge;
    let vert = smoothstep(0.0, 0.015, below) * exp(-below * (3.2 + fb * 1.5));
    // Vertical ray striations shimmering across the sheet.
    let rays = 0.45 + 0.55 * sin(uv.x * 50.0 + fbm(vec2(uv.x * 4.0, ph)) * 8.0 + u.time * 0.4);
    let glowv = 0.6 + 0.4 * sin(u.time * 0.7 + fb + uv.x * 2.0);
    let inten = vert * rays * glowv;
    tint = tint + mix(green, magenta, fract(uv.x * 0.7 + fb * 0.3)) * inten;
  }
  // Curtains live in the upper sky; fade toward the horizon.
  let fade = smoothstep(0.0, 0.4, 1.0 - uv.y);
  return col + tint * 0.6 * fade;
}

// Storm: dark churning clouds with periodic lightning. Each ~1.25s window has a chance
// to flash; the flash lights the whole cloudscape and draws a jagged vertical bolt that
// decays fast. Pseudo-random per-window via hash21(floor(time*rate)).
fn storm_sky(uv: vec2<f32>, p: vec2<f32>, aspect: f32) -> vec3<f32> {
  var col = mix(vec3(0.06, 0.07, 0.10), vec3(0.02, 0.025, 0.04), smoothstep(0.0, 1.0, uv.y));
  // Churning clouds (two fbm octaves drifting opposite ways).
  let c1 = fbm(p * 3.0 + vec2(u.time * 0.04, 0.0));
  let c2 = fbm(p * 6.0 + vec2(u.time * -0.06, 1.7));
  let clouds = clamp(c1 * 0.7 + c2 * 0.4, 0.0, 1.0);
  // Scene pool (M8): scene_a = [lightning rate, strike chance, cloud density, _],
  // scene_c1 = bolt colour, scene_c2 = flash colour.
  col = col + vec3(0.05, 0.055, 0.075) * clouds * u.scene_a.z;

  // Lightning: per-window strike chance + fast decay envelope.
  let rate = u.scene_a.x;
  let seg = floor(u.time * rate);
  let ft = fract(u.time * rate);
  let strike = hash21(vec2(seg, 3.0));
  var flash = 0.0;
  if (strike < u.scene_a.y) {
    flash = exp(-ft * 9.0) * (0.7 + 0.3 * sin(ft * 90.0));
  }
  // Whole-sky flash, brighter where clouds are dense.
  col = col + u.scene_c2.rgb * flash * (0.4 + clouds);
  // A jagged bolt at a per-strike x, upper sky only.
  let boltx = 0.25 + 0.5 * hash21(vec2(seg, 7.0));
  let jag = boltx + 0.018 * sin(uv.y * 50.0) + 0.02 * fbm(vec2(uv.y * 9.0, seg));
  let bd = abs(uv.x - jag);
  let bolt = exp(-bd * bd * 7000.0) * smoothstep(0.72, 0.0, uv.y) * flash;
  col = col + u.scene_c1.rgb * bolt * 2.5;
  return col;
}

// Rain: a calm overcast sky with several parallax layers of falling streaks.
fn rain_sky(uv: vec2<f32>, p: vec2<f32>, aspect: f32) -> vec3<f32> {
  var col = mix(vec3(0.16, 0.18, 0.22), vec3(0.10, 0.115, 0.14), smoothstep(0.0, 1.0, uv.y));
  var rain = 0.0;
  for (var l = 0; l < 3; l = l + 1) {
    let fl = f32(l);
    let sc = vec2(46.0 + fl * 18.0, 7.0 + fl * 2.0);
    var rp = vec2(uv.x * aspect, uv.y) * sc;
    rp.x = rp.x + fl * 11.0 + uv.y * u.scene_a.z;        // diagonal slant
    rp.y = rp.y + u.time * (u.scene_a.x + fl * 2.5) * sc.y / 7.0;
    let cell = floor(rp);
    let h = hash21(cell + fl * 31.0);
    let f = fract(rp) - vec2(0.5, 0.5);
    // thin in x, a short dash in y
    let streak = step(1.0 - u.scene_a.y, h) * exp(-f.x * f.x * 80.0) * smoothstep(0.5, 0.0, abs(f.y));
    rain = rain + streak * (0.7 + fl * 0.2);
  }
  return col + u.scene_c1.rgb * rain * u.scene_a.w;
}

// Snow: a soft winter twilight with drifting, swaying flakes across parallax layers.
fn snow_sky(uv: vec2<f32>, p: vec2<f32>, aspect: f32) -> vec3<f32> {
  var col = mix(vec3(0.58, 0.63, 0.72), vec3(0.40, 0.45, 0.56), smoothstep(0.0, 1.0, uv.y));
  var snow = 0.0;
  for (var l = 0; l < 3; l = l + 1) {
    let fl = f32(l);
    let sc = 13.0 + fl * 7.0;
    var sp = vec2(uv.x * aspect, uv.y) * sc;
    sp.y = sp.y + u.time * (u.scene_a.x + fl * 0.5);
    sp.x = sp.x + sin(u.time * 0.5 + fl + sp.y * 0.25) * u.scene_a.z;   // sway
    let cell = floor(sp);
    let h = hash21(cell + fl * 23.0);
    let f = fract(sp) - vec2(0.5, 0.5);
    let flake = step(1.0 - u.scene_a.y, h) * exp(-dot(f, f) * (u.scene_a.w + fl * 14.0));
    snow = snow + flake * (0.6 + fl * 0.3);
  }
  return col + u.scene_c1.rgb * snow;
}

// A faint static starfield, shared by the night-based scenes (meteor, moon).
fn scene_stars(uv: vec2<f32>, aspect: f32) -> f32 {
  let par = u.params8.xy * u.params8.z * 0.05;
  let gp = (uv - par) * vec2(aspect, 1.0) * 22.0;
  let cell = floor(gp);
  let h = hash21(cell);
  if (h > 0.9) {
    let cp = fract(gp) - vec2(0.5, 0.5);
    return exp(-dot(cp, cp) * 90.0) * (h - 0.9) * 10.0;
  }
  return 0.0;
}

// Meteor shower: many diagonal shooting stars with tapered trails over a night sky.
// Each meteor re-spawns at a fresh position every life-cycle (integer part of its
// phase); the fractional part drives its travel + a fade in/out envelope.
fn meteor_sky(uv: vec2<f32>, p: vec2<f32>, aspect: f32) -> vec3<f32> {
  let bg = u.bg_bot.rgb;
  let top = mix(bg, u.glow.rgb, 0.22);
  var col = mix(top, bg, smoothstep(0.0, 1.0, uv.y));
  col = col + vec3(0.75, 0.8, 1.0) * scene_stars(uv, aspect);

  let dir = normalize(vec2(-0.6, 0.55)); // down-left (uv y is down)
  var m = 0.0;
  for (var i = 0; i < 10; i = i + 1) {
    let fi = f32(i);
    let sp = 0.16 + 0.14 * hash21(vec2(fi, 2.0));
    let prog = u.time * sp + hash21(vec2(fi, 9.0));
    let life = floor(prog);
    let t = fract(prog);
    let ox = hash21(vec2(fi, life));
    let oy = -0.15 + 0.35 * hash21(vec2(fi, life + 3.0));
    let origin = vec2(ox * 1.3, oy);
    let head = origin + dir * t * 1.7;
    var rel = uv - head;
    rel.x = rel.x * aspect;
    let along = dot(rel, dir);
    let perp = length(rel - along * dir);
    let trail = exp(-perp * perp * 9000.0) * exp(along * 26.0) * step(along, 0.0);
    let headg = exp(-dot(rel, rel) * 5000.0);
    let env = smoothstep(0.0, 0.08, t) * smoothstep(1.0, 0.75, t);
    m = m + (trail * 0.85 + headg) * env;
  }
  return col + vec3(0.85, 0.92, 1.0) * m;
}

// Moon: a large phased moon (terminator drifts slowly) with fbm "maria", a soft halo
// and a starfield. The lit mask is the moon disc minus an offset shadow disc.
fn moon_sky(uv: vec2<f32>, p: vec2<f32>, aspect: f32) -> vec3<f32> {
  let bg = u.bg_bot.rgb;
  var col = mix(mix(bg, u.glow.rgb, 0.18), bg, smoothstep(0.0, 1.0, uv.y));
  col = col + vec3(0.75, 0.8, 1.0) * scene_stars(uv, aspect);

  let mc = vec2(0.32, 0.30);
  var rel = uv - mc;
  rel.x = rel.x * aspect;
  let r = length(rel);
  let rad = 0.16;
  let disc = smoothstep(rad, rad - 0.006, r);
  // Phase: an offset shadow disc sweeps across (slow).
  let phase = sin(u.time * 0.06);
  let sr = length(rel - vec2(phase * 0.20, 0.0));
  let lit = smoothstep(rad - 0.006, rad, sr);
  let maria = 0.82 + 0.18 * fbm(rel * 22.0);
  col = col + vec3(0.93, 0.93, 0.86) * disc * lit * maria;
  // Soft halo.
  col = col + vec3(0.5, 0.55, 0.72) * exp(-r * r * 26.0) * 0.28;
  return col;
}

// Synthwave: a deep purple→pink dusk, a striped neon sun on the horizon, and a glowing
// cyan perspective grid on the "ground" (lower half), scrolling toward the viewer.
fn synthwave_sky(uv: vec2<f32>, p: vec2<f32>, aspect: f32) -> vec3<f32> {
  // Scene-param pool (M8): scene_a=[grid speed, density, perspective, glow],
  // scene_b=[sun size, sun stripes, sun bloom, horizon], scene_c1/2/3 = colours.
  let horizon = u.scene_b.w;
  let purple = u.scene_c2.rgb;
  let pink = u.scene_c3.rgb;
  var col = mix(purple, pink, smoothstep(0.0, horizon, uv.y) * smoothstep(1.0, 0.3, uv.y / horizon));
  col = mix(purple * 0.6, col, smoothstep(0.0, 0.5, uv.y));

  // Striped sun above the horizon.
  let sc = vec2(0.5, horizon);
  var sr = uv - sc;
  sr.x = sr.x * aspect;
  let sd = length(sr);
  let sun_r = u.scene_b.x;
  let sun = smoothstep(sun_r, sun_r - 0.005, sd);
  let stripes = step(0.0, sin(uv.y * u.scene_b.y)) * step(uv.y, horizon); // dark bands, upper sun
  let suncol = mix(vec3(1.0, 0.85, 0.3), vec3(1.0, 0.25, 0.55), smoothstep(horizon - sun_r, horizon, uv.y));
  col = mix(col, suncol, sun * (1.0 - stripes * 0.85));
  col = col + vec3(1.0, 0.4, 0.6) * exp(-sd * sd * 9.0) * u.scene_b.z; // sun glow

  // Neon perspective grid on the ground (uv.y > horizon).
  if (uv.y > horizon) {
    let gy = uv.y - horizon + 0.02;
    let z = 1.0 / gy;
    let scroll = u.time * u.scene_a.x;
    let lh = abs(fract(z * u.scene_a.y - scroll) - 0.5);
    let px = (uv.x - 0.5) * z * u.scene_a.z;
    let lv = abs(fract(px) - 0.5);
    let grid = smoothstep(0.06, 0.0, lh) + smoothstep(0.05, 0.0, lv);
    let depth = smoothstep(0.0, 0.25, gy);
    col = col + u.scene_c1.rgb * grid * depth * u.scene_a.w;
  }
  return col;
}

// Fog: drifting fbm fog banks over a muted gradient, with stars dimmed by the haze.
fn fog_sky(uv: vec2<f32>, p: vec2<f32>, aspect: f32) -> vec3<f32> {
  let bg = u.bg_bot.rgb;
  var col = mix(bg * 1.4 + vec3(0.04), bg, smoothstep(0.0, 1.0, uv.y));
  var fog = 0.0;
  for (var i = 0; i < 3; i = i + 1) {
    let fi = f32(i);
    let drift = vec2(u.time * (0.02 + fi * 0.012), fi * 1.3);
    fog = fog + fbm(p * (1.4 + fi * 1.1) + drift) * (0.45 - fi * 0.1);
  }
  let fogc = clamp(fog, 0.0, 1.0);
  col = col + vec3(0.7, 0.74, 0.82) * scene_stars(uv, aspect) * (1.0 - fogc);
  col = mix(col, vec3(0.66, 0.70, 0.78), fogc * 0.7);
  return col;
}

// Plasma: classic demoscene field — summed sines (axis, diagonal, radial) mapped to
// color through three phase-shifted sines. Vivid and continuously morphing.
fn plasma_sky(uv: vec2<f32>, p: vec2<f32>, aspect: f32) -> vec3<f32> {
  let t = u.time * 0.5;
  let x = uv.x * aspect;
  let y = uv.y;
  var v = sin(x * 8.0 + t);
  v = v + sin((y * 8.0 + t) * 1.1);
  v = v + sin((x + y) * 6.0 + t);
  let cx = x - 0.5 * aspect;
  v = v + sin(length(vec2(cx, y - 0.5)) * 14.0 - t * 1.3);
  v = v * 0.25;
  let ph = v * 3.14159265;
  return vec3(
    0.5 + 0.5 * sin(ph),
    0.5 + 0.5 * sin(ph + 2.094),
    0.5 + 0.5 * sin(ph + 4.188),
  );
}

// Fire: scrolling fbm "heat" weighted toward the bottom, mapped through a black→red→
// orange→white-yellow ramp so flames lick upward.
fn fire_sky(uv: vec2<f32>, p: vec2<f32>, aspect: f32) -> vec3<f32> {
  let q = vec2(uv.x * aspect, uv.y);
  let flow = fbm(q * vec2(3.0, 4.5) + vec2(0.0, u.time * u.scene_a.x));   // scroll upward
  let flow2 = fbm(q * vec2(6.0, 8.0) + vec2(3.0, u.time * u.scene_a.x * 1.5));
  let n = flow * 0.65 + flow2 * 0.35;
  // uv.y = 1 at the bottom; weight heat toward it.
  var heat = clamp((uv.y - u.scene_a.y) * 1.25, 0.0, 1.0) * (0.35 + 1.1 * n);
  heat = pow(clamp(heat, 0.0, 1.0), 1.5);
  var col = vec3(0.02, 0.005, 0.0);
  // scene_c1 is the normalised base hue; the 1.6 boost (kept internal) re-creates the
  // hot, over-bright deep flame the literal vec3(1.6, …) gave.
  col = col + u.scene_c1.rgb * 1.6 * heat;
  col = col + u.scene_c2.rgb * pow(heat, 3.0);
  return col;
}

// Water: animated caustics — a domain-warped sine network over a blue-green depth
// gradient, the bright web concentrated where the warped product nears zero.
fn water_sky(uv: vec2<f32>, p: vec2<f32>, aspect: f32) -> vec3<f32> {
  let t = u.time * 0.6;
  var q = vec2(uv.x * aspect, uv.y) * 5.0;
  // Two domain-warp passes for organic ripple.
  q = q + vec2(sin(q.y * 1.3 + t), cos(q.x * 1.3 + t)) * 0.6;
  q = q + vec2(sin(q.y * 0.7 - t * 0.8), cos(q.x * 0.7 + t * 0.8)) * 0.4;
  let web = abs(sin(q.x) * sin(q.y) + 0.5 * sin((q.x + q.y) + t));
  let caustic = pow(1.0 - clamp(web, 0.0, 1.0), 3.0);
  let base = mix(vec3(0.0, 0.12, 0.22), vec3(0.0, 0.30, 0.42), smoothstep(0.0, 1.0, uv.y));
  return base + vec3(0.45, 0.95, 1.0) * caustic * 0.85;
}

// The night sky (auto, after dark): gradient + radial glow + fbm nebula + parallax
// twinkling starfield. Extracted so the frosted-card backdrop can reuse it.
fn night_sky(uv: vec2<f32>, p: vec2<f32>, aspect: f32) -> vec3<f32> {
  var col = mix(u.bg_top.rgb, u.bg_bot.rgb, smoothstep(0.0, 1.0, uv.y));

  // True radial glow / sun-haze, centered via params5.xy (0–1 UV).
  var gc = vec2(u.params5.x, u.params5.y) - vec2(0.5, 0.5);
  gc.x = gc.x * aspect;
  let gd = length(p - gc);
  // Optional gentle breathing of the glow (params8.w = depth; 0 = steady).
  let glow_pulse = 1.0 + u.params8.w * 0.35 * sin(u.time * 0.7);
  col = col + u.glow.rgb * exp(-gd * gd * u.params3.y) * u.params.w * glow_pulse;

  // fbm nebula for depth, concentrated near the glow.
  let nbs = u.params6.z;
  let neb = fbm(p * 2.2 + vec2(u.time * 0.015 * nbs, u.time * -0.010 * nbs));
  col = col + u.glow.rgb * neb * u.params3.z * smoothstep(0.95, 0.0, gd);

  var stars = 0.0;
  var spark = 0.0;
  let layers = i32(u.params6.x);
  for (var l = 0; l < layers; l = l + 1) {
    let scale = 8.0 * pow(1.9, f32(l));
    let par = u.params8.xy * u.params8.z * 0.04 * (f32(l) + 1.0);
    let gp = (uv - par) * vec2(aspect, 1.0) * scale;
    let cell = floor(gp);
    let thr = u.params2.x;
    let h = hash21(cell + f32(l) * 17.0);
    if (h > thr) {
      let h2 = hash21(cell + f32(l) * 17.0 + 5.0);
      let cp = fract(gp) - vec2(0.5, 0.5);
      var tw = 0.5 + 0.5 * sin(u.time * (0.8 + h2 * 3.6) * u.params2.y + h * 40.0);
      tw = tw * tw * tw;
      let bright = (h - thr) / max(1.0 - thr, 0.01);
      stars = stars + exp(-dot(cp, cp) * 70.0 / max(u.params6.y, 0.1)) * tw * bright;
      let cross = exp(-abs(cp.x) * 55.0) * exp(-cp.y * cp.y * 900.0)
                + exp(-abs(cp.y) * 55.0) * exp(-cp.x * cp.x * 900.0);
      spark = spark + cross * tw * bright * 0.5;
    }
  }
  col = col + u.star.rgb * stars * u.params.z;
  col = col + vec3(1.0, 1.0, 1.0) * spark * 0.5 * u.params.z;
  return col;
}

// The base scene for the current mode — no comet/grain/vignette overlays. Shared by
// fs_main and the frosted-card backdrop (fs_frost), so the blur matches the sky.
fn scene_base(uv: vec2<f32>, p: vec2<f32>, aspect: f32) -> vec3<f32> {
  let day = u.params.x;
  let mode = u.params5.w;
  if (mode > 0.5) {
    if (mode < 1.5) { return aurora_sky(uv, p, aspect); }
    else if (mode < 2.5) { return storm_sky(uv, p, aspect); }
    else if (mode < 3.5) { return rain_sky(uv, p, aspect); }
    else if (mode < 4.5) { return snow_sky(uv, p, aspect); }
    else if (mode < 5.5) { return meteor_sky(uv, p, aspect); }
    else if (mode < 6.5) { return moon_sky(uv, p, aspect); }
    else if (mode < 7.5) { return synthwave_sky(uv, p, aspect); }
    else if (mode < 8.5) { return fog_sky(uv, p, aspect); }
    else if (mode < 9.5) { return plasma_sky(uv, p, aspect); }
    else if (mode < 10.5) { return fire_sky(uv, p, aspect); }
    else { return water_sky(uv, p, aspect); }
  }
  if (day > 0.5) { return day_sky(uv, p, aspect); }
  return night_sky(uv, p, aspect);
}

@fragment
fn fs_main(in: VsOut) -> @location(0) vec4<f32> {
  let uv = in.uv;
  let aspect = u.res.x / max(u.res.y, 1.0);
  var p = uv - vec2(0.5, 0.5);
  p.x = p.x * aspect;
  let mode = u.params5.w;

  var col = scene_base(uv, p, aspect);

  // Drifting comet: upper-right -> lower-left on an InOutSine ease, then pause.
  let period = u.params2.z;
  let cu = fract(u.time / period);
  let sf = (period - u.params7.y) / period;   // params7.y-second pause after each sweep
  if (cu < sf) {
    let pp = ease(cu / sf);
    // Comet path + tail direction, rotated about screen-center by comet tilt (params7.x).
    let tilt = u.params7.x;
    let ctil = cos(tilt);
    let stil = sin(tilt);
    let hc = vec2(mix(1.08, -0.22, pp), mix(0.10, 0.65, pp)) - vec2(0.5, 0.5);
    var head = vec2(hc.x * ctil - hc.y * stil, hc.x * stil + hc.y * ctil);
    head.x = head.x * aspect;
    let d0 = vec2(0.92, -0.39); // tail recedes up-right (pre-tilt)
    let dir = normalize(vec2(d0.x * ctil - d0.y * stil, d0.x * stil + d0.y * ctil));
    let rel = p - head;
    let along = dot(rel, dir);
    let perp = length(rel - along * dir);
    // Teardrop tail: narrow at the head, fanning out and fading along the heading —
    // not a constant-width straight beam.
    let t = max(along, 0.0);
    let width = (0.010 + t * 0.10) * u.params7.z;
    let tail = exp(-(perp * perp) / (width * width)) * exp(-t * u.params3.w) * step(0.0, along);
    let nuc = exp(-dot(rel, rel) * 3000.0);   // white-hot nucleus
    let coma = exp(-dot(rel, rel) * 300.0);   // soft round coma
    let ccol = mix(u.comet.rgb, vec3(1.0, 1.0, 1.0), nuc * 0.7);
    // Gate the comet to auto/aurora skies (step(mode,1.5) = 1 for mode<=1.5, else 0);
    // a sweeping comet over storm/rain/snow reads wrong.
    col = col + ccol * (nuc * 1.5 + coma * 0.45 + tail * 0.85) * u.params.y * step(mode, 1.5);
  }

  // Ordered dither — one sub-LSB of noise so 8-bit targets never band.
  col = col + vec3((hash21(in.pos.xy) - 0.5) / 255.0);

  // Film grain (animated) + vignette (M7-C) — both off at 0.
  col = col + vec3((hash21(in.pos.xy * 1.3 + vec2(u.time * 60.0)) - 0.5) * u.params9.x);
  let vig = 1.0 - u.params9.y * smoothstep(0.35, 0.85, length(uv - vec2(0.5, 0.5)));
  col = col * vig;

  col = col * u.fade;
  return vec4(col, 1.0);
}

// Frosted-card backdrop: a blurred sample of the base scene behind the card, masked to
// the card's rounded rect. `frost` is the card rect (screen-normalized); we map this
// quad's local uv to the global screen uv so the blur lines up with the sky behind it.
@fragment
fn fs_frost(in: VsOut) -> @location(0) vec4<f32> {
  let aspect = u.res.x / max(u.res.y, 1.0);
  let g = vec2(u.frost.x + in.uv.x * u.frost.z, u.frost.y + in.uv.y * u.frost.w);
  var acc = vec3(0.0);
  var wsum = 0.0;
  let rad_blur = 0.013;
  for (var i = -2; i <= 2; i = i + 1) {
    for (var j = -2; j <= 2; j = j + 1) {
      let off = vec2(f32(i), f32(j)) * (rad_blur * 0.5);
      let suv = g + off;
      var sp = suv - vec2(0.5, 0.5);
      sp.x = sp.x * aspect;
      let w = exp(-f32(i * i + j * j) * 0.5);
      acc = acc + scene_base(suv, sp, aspect) * w;
      wsum = wsum + w;
    }
  }
  var col = acc / max(wsum, 0.0001);
  // Rounded-rect alpha so the frost matches the card's corners.
  let card_px = u.frost.zw * u.res;
  let p_px = in.uv * card_px;
  let halfc = card_px * 0.5;
  let crad = u.params9.z;
  let q = abs(p_px - halfc) - (halfc - vec2(crad, crad));
  let dist = length(max(q, vec2(0.0, 0.0))) + min(max(q.x, q.y), 0.0) - crad;
  let alpha = 1.0 - smoothstep(-1.0, 1.0, dist);
  return vec4(col * u.fade, alpha * u.fade);
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

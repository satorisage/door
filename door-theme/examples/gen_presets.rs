// A generator: building each preset by tweaking fields off a base is clearer than a
// 30-field struct literal, so the reassign-after-default lint doesn't apply here.
#![allow(clippy::field_reassign_with_default)]
//! Regenerate the packaged theme presets in `dist/door/presets/`. Run from the repo
//! root: `cargo run -p door-theme --example gen_presets`. Each preset is a full
//! `greeter.toml` (night top-level + a `[day]` block) produced by `render_pair`, so it
//! always matches the schema. The two "insane" presets crank the GPU controls.

use door_theme::{Color, Theme};
use std::fs;

fn col(hex: &str) -> Color {
    Color::parse(hex).unwrap_or_else(|| panic!("bad color {hex}"))
}

fn write(slug: &str, night: &Theme, day: &Theme) {
    let body = Theme::render_pair(night, day, "07:00", "19:00");
    let path = format!("dist/door/presets/{slug}.toml");
    fs::write(&path, body).unwrap_or_else(|e| panic!("write {path}: {e}"));
    println!("wrote {path}");
}

fn main() {
    fs::create_dir_all("dist/door/presets").expect("mkdir presets");

    // ── Tokyo Night — the built-in default, captured as a loadable preset ──
    write("tokyo-night", &Theme::default(), &Theme::day());

    // ── Supernova — a hot magenta/cyan inferno (night) / blazing sun (day) ──
    let mut n = Theme::default();
    n.background = col("#0a0014");
    n.card = col("#1a0626cc");
    n.field = col("#2a0e3a");
    n.accent = col("#ff2d95");
    n.foreground = col("#ffd6f5");
    n.muted = col("#b06a9a");
    n.spinner_comet = col("#ff5cf0");
    n.spinner_track = col("#00e5ff");
    n.spinner_glow = 1.0;
    n.spinner_trail = 1.0;
    n.spinner_pulse = 2.6;
    n.spinner_ring = 0.42;
    n.spinner_size = 92.0;
    n.comet_color = col("#ff3df0");
    n.error_color = col("#ff5577");
    n.glow_color = col("#ff2d95");
    n.sky_glow = 0.95;
    n.star_density = 1.0;
    n.star_twinkle = 3.6;
    n.comet_enabled = true;
    n.comet_interval = 4.0;
    n.nebula_amount = 0.5;
    n.glow_falloff = 2.4;
    n.comet_tail_decay = 4.5;
    n.cloud_amount = 2.0;
    n.cloud_speed = 3.2;
    n.accent_breathing = 2.6;
    n.card_shadow_blur = 60.0;
    n.card_shadow_opacity = 0.72;
    let mut d = Theme::day();
    d.background = col("#ffd9a0");
    d.card = col("#fff0e0aa");
    d.field = col("#ffffffcc");
    d.accent = col("#ff3b00");
    d.foreground = col("#3a1500");
    d.muted = col("#a05a30");
    d.spinner_comet = col("#ff3b00");
    d.spinner_track = col("#ff8a00");
    d.spinner_trail = 0.85;
    d.comet_color = col("#ff6a00");
    d.error_color = col("#c01030");
    d.glow_color = col("#ffcf8a");
    d.sky_glow = 1.1;
    write("supernova", &n, &d);

    // ── Nebula — deep-space cyan/violet (night) / bright aurora (day) ──
    let mut n = Theme::default();
    n.background = col("#01010a");
    n.card = col("#0a0a1ad0");
    n.field = col("#14143a");
    n.accent = col("#7df9ff");
    n.foreground = col("#d6f5ff");
    n.muted = col("#5a6aa0");
    n.spinner_comet = col("#7df9ff");
    n.spinner_track = col("#b388ff");
    n.spinner_glow = 1.0;
    n.spinner_trail = 1.0;
    n.spinner_pulse = 1.6;
    n.spinner_ring = 0.36;
    n.spinner_size = 84.0;
    n.comet_color = col("#a0f0ff");
    n.error_color = col("#ff6b9d");
    n.glow_color = col("#9b6bff");
    n.sky_glow = 0.9;
    n.star_density = 1.0;
    n.star_twinkle = 2.0;
    n.comet_enabled = true;
    n.comet_interval = 5.0;
    n.nebula_amount = 0.5;
    n.glow_falloff = 2.2;
    n.comet_tail_decay = 4.0;
    n.cloud_amount = 1.8;
    n.cloud_speed = 2.2;
    n.accent_breathing = 1.6;
    n.card_shadow_blur = 50.0;
    n.card_shadow_opacity = 0.66;
    let mut d = Theme::day();
    d.background = col("#cfe8ff");
    d.card = col("#e8f5ffaa");
    d.field = col("#ffffffcc");
    d.accent = col("#00b3a4");
    d.foreground = col("#0a2a3a");
    d.muted = col("#4a7a8a");
    d.spinner_comet = col("#008c9e");
    d.spinner_track = col("#00b3a4");
    d.spinner_trail = 0.7;
    d.comet_color = col("#4ac0d0");
    d.glow_color = col("#8fd0ff");
    d.sky_glow = 1.3;
    d.cloud_amount = 2.0;
    d.cloud_speed = 2.4;
    write("nebula", &n, &d);
}

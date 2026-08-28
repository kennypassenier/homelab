//! Effects engine: glitch, decrypt-reveal, pulse, scanline, ticker, flicker.
//!
//! All effects are *stateless per frame* where possible — they derive from
//! (anim_tick, element id) so no per-element bookkeeping is needed. Everything
//! is O(text length) and allocation-light, keeping the 30 FPS budget honest.

use std::hash::{DefaultHasher, Hash, Hasher};

use ratatui::style::Color;

use crate::theme::THEME;

pub const GLITCH_CHARS: &[char] = &[
    '!', '@', '#', '$', '%', '^', '&', '*', '?', '~', '░', '▒', '▓', '/', '\\', '<', '>',
];
pub const REVEAL_CHARS: &[char] = &['▓', '▒', '░'];

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum FxLevel {
    Off,
    Subtle,
    Full,
}

impl FxLevel {
    pub fn cycle(self) -> Self {
        match self {
            FxLevel::Off => FxLevel::Subtle,
            FxLevel::Subtle => FxLevel::Full,
            FxLevel::Full => FxLevel::Off,
        }
    }
    pub fn label(self) -> &'static str {
        match self {
            FxLevel::Off => "FX:OFF",
            FxLevel::Subtle => "FX:SUBTLE",
            FxLevel::Full => "FX:FULL",
        }
    }
}

fn hash2(a: u64, b: u64) -> u64 {
    let mut h = DefaultHasher::new();
    a.hash(&mut h);
    b.hash(&mut h);
    h.finish()
}

/// Deterministic glitch: roughly once every ~8s per element, an element's text
/// scrambles for a 2-3 tick burst. Returns None when not glitching.
pub fn glitch(text: &str, id: u64, tick: u64, level: FxLevel) -> Option<String> {
    if level == FxLevel::Off {
        return None;
    }
    // Window of 2 ticks; each window rolls a chance keyed on (id, window).
    let window = tick / 2;
    let roll = hash2(id, window) % 1000;
    let threshold = match level {
        FxLevel::Full => 8,   // ~0.8% of windows → ~ every 8s at 30fps
        FxLevel::Subtle => 3, // rarer
        FxLevel::Off => 0,
    };
    if roll >= threshold {
        return None;
    }
    let mut out = String::with_capacity(text.len());
    for (i, c) in text.chars().enumerate() {
        if c.is_alphanumeric() {
            let r = hash2(hash2(id, window), (i as u64) ^ tick);
            if r % 100 < 35 {
                out.push(GLITCH_CHARS[(r % GLITCH_CHARS.len() as u64) as usize]);
                continue;
            }
        }
        out.push(c);
    }
    Some(out)
}

/// Decrypt-reveal: text materializes left-to-right through block characters.
/// `progress` in 0.0..=1.0.
pub fn decrypt(text: &str, progress: f32, id: u64, tick: u64) -> String {
    if progress >= 1.0 {
        return text.to_string();
    }
    let chars: Vec<char> = text.chars().collect();
    let visible = (chars.len() as f32 * progress) as usize;
    let mut out = String::with_capacity(text.len());
    for (i, c) in chars.iter().enumerate() {
        if i < visible || c.is_whitespace() {
            out.push(*c);
        } else if i < visible + 3 {
            // The "decryption edge": churning block chars.
            let r = hash2(id, (i as u64) ^ tick);
            out.push(REVEAL_CHARS[(r % 3) as usize]);
        } else {
            out.push('░');
        }
    }
    out
}

/// Sinusoidal pulse between the panel color and a dark cyan, ~2.6s cycle.
pub fn pulse_bg(tick: u64, level: FxLevel) -> Color {
    if level == FxLevel::Off {
        return Color::Rgb(0x0F, 0x2A, 0x2A);
    }
    let t = (tick as f32) / 30.0; // seconds
    let s = ((t / 2.6) * std::f32::consts::TAU).sin() * 0.5 + 0.5; // 0..1
    let lerp = |a: u8, b: u8| -> u8 { (a as f32 + (b as f32 - a as f32) * s) as u8 };
    Color::Rgb(lerp(0x0A, 0x0F), lerp(0x20, 0x3A), lerp(0x20, 0x3A))
}

/// Scanline sweep: every ~10s a highlight row sweeps down a panel of height
/// `h` over ~1s. Returns the currently-lit row, if any.
pub fn scanline(h: u16, tick: u64, id: u64, level: FxLevel) -> Option<u16> {
    if level != FxLevel::Full || h == 0 {
        return None;
    }
    let period: u64 = 300; // 10s at 30fps
    let sweep: u64 = 36; // ~1.2s
    let phase = (tick + hash2(id, 7) % period) % period;
    if phase < sweep {
        Some(((phase as f32 / sweep as f32) * h as f32) as u16)
    } else {
        None
    }
}

/// Build the bottom telemetry ticker text for the current tick.
pub fn ticker_text(segments: &[String], width: u16, tick: u64) -> String {
    let mut s = String::new();
    for seg in segments {
        s.push_str(seg);
        s.push_str("  ::  ");
    }
    if s.is_empty() {
        return s;
    }
    let chars: Vec<char> = s.chars().collect();
    let len = chars.len();
    let offset = ((tick / 3) as usize) % len;
    let mut out = String::with_capacity(width as usize);
    for i in 0..width as usize {
        out.push(chars[(offset + i) % len]);
    }
    out
}

/// Power-cycle flicker on tab switches: a few dark ticks, one bright flash.
#[derive(Clone, Copy, PartialEq, Eq)]
pub enum FlickerPhase {
    None,
    Dark,
    Flash,
}

pub fn flicker_phase(ticks_left: u8) -> FlickerPhase {
    match ticks_left {
        0 => FlickerPhase::None,
        1 => FlickerPhase::Flash,
        _ => FlickerPhase::Dark,
    }
}

/// Color for spinner frames.
pub fn spinner(tick: u64) -> char {
    const FRAMES: &[char] = &['⠋', '⠙', '⠹', '⠸', '⠼', '⠴', '⠦', '⠧', '⠇', '⠏'];
    FRAMES[((tick / 3) % FRAMES.len() as u64) as usize]
}

/// Braille sparkline from a slice of 0..=100 samples.
pub fn braille_spark(data: &[u64], width: usize) -> String {
    const BARS: &[char] = &['▁', '▂', '▃', '▄', '▅', '▆', '▇', '█'];
    let n = data.len();
    let take = width.min(n);
    let slice = &data[n - take..];
    let mut out = String::with_capacity(take);
    for v in slice {
        let idx = ((*v).min(100) as usize * (BARS.len() - 1)) / 100;
        out.push(BARS[idx]);
    }
    out
}

/// Interpolate a load percentage to a status color.
pub fn load_color(pct: u64) -> Color {
    if pct < 60 {
        THEME.green
    } else if pct < 85 {
        THEME.yellow
    } else {
        THEME.red
    }
}

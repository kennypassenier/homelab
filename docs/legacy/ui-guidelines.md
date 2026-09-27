# UI Guidelines — Homelab CLIENT TUI

> **Scope:** These guidelines govern every visual decision in the Ratatui-based
> `client-app` binary. All current and future tabs, modals, and widgets must
> follow the rules below. When a rule conflicts with a convenience shortcut,
> the rule wins.

---

## Table of Contents

1. [Design Philosophy](#1-design-philosophy)
2. [Color Palette](#2-color-palette)
3. [Typography Rules](#3-typography-rules)
4. [Border & Frame Style](#4-border--frame-style)
5. [Layout Conventions](#5-layout-conventions)
6. [Animation Architecture](#6-animation-architecture)
7. [Glitch Effects](#7-glitch-effects)
8. [Power Cycle / Flicker Effect](#8-power-cycle--flicker-effect)
9. [Data Decryption Reveal](#9-data-decryption-reveal)
10. [Pulse / Breathing Highlights](#10-pulse--breathing-highlights)
11. [Ambient UI Elements](#11-ambient-ui-elements)
12. [Tick Loop Architecture](#12-tick-loop-architecture)
13. [Animation State in `App`](#13-animation-state-in-app)
14. [Implementation Checklist](#14-implementation-checklist)

---

## 1. Design Philosophy

The CLIENT TUI is a **command-and-control interface for a personal homelab**.
It should feel like the terminal on a spaceship: dense, live, and aggressive.
The aesthetic is **hyper-modern cyberpunk** — neon accents on near-black
backgrounds, fragmented geometry, constant micro-motion.

**Core rules:**

- **Nothing is static.** Every idle moment is an opportunity for a subtle
  animation: a pulse, a ticker advance, a glitch flash.
- **Signal over noise.** Animations must be perceptually lightweight; they
  must *never* obscure actionable data or shift layout.
- **System vocabulary.** Labels read like terminal output, not GUI text. Use
  `SYS_CORE`, `HOST_MESH`, `[ONLINE]`, `>> STATUS <<` rather than friendly
  prose.
- **Monochrome base, neon highlight.** The eye should move naturally from dark
  background → bright accent → status color → data.

---

## 2. Color Palette

All colors are defined in `theme.rs` and returned via `Theme::cyberpunk()`.
**Never hard-code colors outside `theme.rs` or the widget that owns a
specific semantic role** (e.g., log-level colors stay in `ui.rs`).

### 2.1 Background Hierarchy

| Role                   | Ratatui Color            | Hex approx  | Usage                              |
|------------------------|--------------------------|-------------|------------------------------------|
| Terminal canvas        | `Rgb(10, 10, 16)`        | `#0A0A10`   | Implicit; set in `clear` calls     |
| Panel background       | `Rgb(16, 18, 28)`        | `#10121C`   | Block fill behind bordered widgets |
| Elevated panel         | `Rgb(22, 24, 36)`        | `#161824`   | Modals, popovers                   |
| Dimmed / powered-down  | `Rgb(30, 30, 34)`        | `#1E1E22`   | Inactive / disabled widgets        |

### 2.2 Primary Accents

| Name               | Ratatui Color              | Hex       | Usage                                    |
|--------------------|----------------------------|-----------|------------------------------------------|
| Neon Cyan          | `Color::Cyan` / `Rgb(0,255,255)` | `#00FFFF` | Active borders, titles, highlighted text |
| Bright Magenta     | `Color::Magenta` / `Rgb(255,0,255)` | `#FF00FF` | Secondary accent, modal borders          |
| Neon Green         | `Color::Green`             | `#00FF00` | Success states, `[ONLINE]`, live status  |
| Acid Yellow        | `Color::Yellow`            | `#FFFF00` | Warnings, actionable hints, scroll arrows|

### 2.3 Data / Content Colors

| Name         | Ratatui Color        | Hex approx | Usage                            |
|--------------|----------------------|------------|----------------------------------|
| Data White   | `Color::White`       | `#E0E0E0`  | Primary readable content         |
| Data Gray    | `Color::Gray`        | `#808080`  | Secondary / metadata fields      |
| Dimmed Gray  | `Color::DarkGray`    | `#404040`  | Timestamps, inactive items       |
| Error Red    | `Color::Red`         | `#FF0000`  | Errors, destructive confirmations|

### 2.4 Source / Stack Identity Colors

These are fixed per-stack and defined in `app::LOG_SOURCES`.  They must not
be reused for unrelated semantic meaning.

| Stack           | Color              |
|-----------------|--------------------|
| lxc-cloudflared | `Color::Blue`      |
| lxc-downloader  | `Color::Magenta`   |
| lxc-gateway     | `Color::Yellow`    |
| lxc-media       | `Color::Cyan`      |
| lxc-monitoring  | `Color::Green`     |
| lxc-paperless   | `Color::LightCyan` |
| lxc-vikunja     | `Color::LightMagenta` |
| HOST            | `Color::White` + `BOLD` |
| CLIENT          | `Color::Cyan` + `BOLD`  |

---

## 3. Typography Rules

### 3.1 Case Convention

| Context                        | Case / Style             | Example                        |
|--------------------------------|--------------------------|--------------------------------|
| Block/panel titles             | `UPPER_SNAKE_CASE`       | `[ HOST_MESH :: ACTIVE ]`      |
| Tab bar labels                 | Title Case               | `Host Management`              |
| Status badges                  | UPPERCASE                | `[ONLINE]`, `[PAUSED]`         |
| Raw data fields (IPs, names)   | lowercase as-is          | `192.168.1.101`                |
| User-visible prose (hints)     | lowercase                | `[a] add / update ssh alias`   |
| Error messages                 | Sentence case            | `Alias cannot be empty`        |

### 3.2 Block Title Formatting

Block titles follow one of two patterns:

```
// Targeted framing — use for important panels:
" [ SYS_CORE :: ACTIVE ] "
">> HOST_MESH <<"

// Simple label — use for data panels:
" SSH Aliases  [a] add / update "
" Logs [live] "
```

Hint keys in titles always use `[key]` format in lowercase.

### 3.3 Unicode Symbols

Prefer block-drawing and geometric symbols over ASCII fallbacks.

| Purpose             | Symbol  | Unicode    |
|---------------------|---------|------------|
| Running indicator   | `●`     | `U+25CF`   |
| Stopped indicator   | `○`     | `U+25CB`   |
| Scroll left         | `◀`     | `U+25C0`   |
| Scroll right        | `▶`     | `U+25B6`   |
| Section divider     | `──`    | `U+2500`   |
| Em-dash             | `—`     | `U+2014`   |
| Warning triangle    | `⚠`     | `U+26A0`   |
| Checkmark           | `✓`     | `U+2713`   |
| Cross               | `✗`     | `U+2717`   |
| Filled block (decrypt reveal) | `█▓▒░` | `U+2588–U+2591` |

---

## 4. Border & Frame Style

### 4.1 Standard Borders

| Context            | `BorderType`         | Accent color          |
|--------------------|----------------------|-----------------------|
| Normal panel       | `BorderType::Rounded`| `theme.border_style()`|
| Active / focused   | `BorderType::Double` | `Color::Cyan`         |
| Modal / popover    | `BorderType::Rounded`| `Color::Cyan` or `Color::Magenta` |
| Danger confirmation| `BorderType::Rounded`| `Color::Red` + `BOLD` |

### 4.2 Fragmented / HUD-style Borders (Future)

For the full cyberpunk vision, replace continuous borders with **corner-only
framing** using custom `Set` border sets from `ratatui::symbols::border`.  
The `symbols::border::Set` allows specifying each corner and line segment
independently, enabling targeting-reticle or HUD effects:

```rust
// Example: corners-only border (no horizontal/vertical lines)
use ratatui::symbols::border;
let hud_set = border::Set {
    top_left:     "┌",   top_right:     "┐",
    bottom_left:  "└",   bottom_right:  "┘",
    // Clear the connecting lines so only corners render
    horizontal_top:    "",  horizontal_bottom:  "",
    vertical_left:     "",  vertical_right:     "",
};
```

Use HUD borders on:
- The active tab's body panel
- The currently selected list item's inline highlight region
- Glitch-state panels (corners flash to `▛▜▙▟` during a glitch tick)

### 4.3 Half-block Decoration

Use `▀` (U+2580) and `▄` (U+2584) for thick horizontal separators between
major sections, styled in the accent color. Do not use these as full borders —
only as decorative dividers 1 row tall.

---

## 5. Layout Conventions

- **Minimum terminal size:** 80 × 24. Handle smaller gracefully (show a
  "TERMINAL TOO SMALL" message in red at the top).
- **Tab bar:** always 3 rows tall, full width, `BorderType::Rounded`.
- **Status footer:** always 1 row, no border, `Color::DarkGray`.
- **Modals:** centered, 50% width, y=33%, height varies per modal. Use
  `ratatui::widgets::Clear` underneath every modal to erase background noise.
- **Side-by-side panels:** use `Percentage` splits. Avoid `Length` for major
  panels so the layout scales with terminal width.
- **Inner padding:** leave 1 column of padding inside bordered blocks by
  including a space prefix in text content, not via `Block::inner()` (which
  conflicts with ratatui 0.26 layout).

---

## 6. Animation Architecture

### 6.1 Tick Rate

The application runs **two independent intervals**:

| Timer               | Interval        | Purpose                                         |
|---------------------|-----------------|--------------------------------------------------|
| `log_tick`          | 400 ms          | Advances mock telemetry log entries              |
| `anim_tick`         | 33 ms (~30 FPS) | Drives all visual animations (glitches, ticker, pulse, decrypt) |

Both are `tokio::time::interval` instances in `main.rs`, handled in the
`tokio::select!` loop **completely decoupled from keyboard input**.

```rust
// main.rs event loop sketch (do not deviate from this pattern):
let mut log_tick   = tokio::time::interval(Duration::from_millis(400));
let mut anim_tick  = tokio::time::interval(Duration::from_millis(33));

loop {
    tokio::select! {
        _ = signal::ctrl_c()        => break,
        _ = log_tick.tick()         => { app.tick_logs(); }
        _ = anim_tick.tick()        => { app.tick_anim(); }
        Some(key) = event_rx.recv() => { /* handle_key_event */ }
    }
    terminal.draw(|f| draw_ui(f, &app))?;
}
```

### 6.2 Frame Budget

At 30 FPS the budget is ~33 ms. Ratatui renders are cheap (diffing), so the
real cost is `tick_anim()` logic. Keep each animation step O(1) or O(n) on
small slices. Never do file I/O or allocation in `tick_anim()`.

---

## 7. Glitch Effects

### 7.1 What Glitches

- Block/panel **titles** (the `title` string of a `Block`).
- The **currently selected list item** in Scaffolding and Host Management.
- The **tab bar label** of the active tab.

### 7.2 Trigger Probability

Each eligible element has a **0.4% chance per `anim_tick`** of entering
glitch state. At 30 FPS this fires roughly once every ~8 seconds per element
on average — noticeable but not distracting.

### 7.3 Glitch State Fields in `App`

```rust
pub struct GlitchState {
    /// How many anim ticks remain in the glitch (typically 2–3 ticks ≈ 66–99 ms).
    pub ticks_remaining: u8,
    /// The scrambled version of the text to display during the glitch.
    pub scrambled: String,
}

// In App:
pub title_glitch:    Option<GlitchState>,   // glitches the active tab title
pub selected_glitch: Option<GlitchState>,   // glitches the selected list item
```

### 7.4 Rendering a Glitched Title

```rust
// In ui.rs — when building a Block title:
let raw_title = " [ HOST_MESH :: ACTIVE ] ";
let display_title = if let Some(g) = &app.title_glitch {
    g.scrambled.as_str()  // e.g. " [ H@$T_M!SH :: ACT!VE ] "
} else {
    raw_title
};

let title_style = if app.title_glitch.is_some() {
    Style::default().fg(Color::Magenta).add_modifier(Modifier::BOLD | Modifier::RAPID_BLINK)
} else {
    Style::default().fg(Color::Cyan).add_modifier(Modifier::BOLD)
};
```

### 7.5 Scramble Algorithm

```rust
// In app.rs — tick_anim()
const GLITCH_CHARS: &[char] = &['!','@','#','$','%','^','&','*','?','~','░','▒','▓'];

fn scramble(text: &str, rng: &mut impl Rng) -> String {
    text.chars()
        .map(|c| {
            if c.is_alphanumeric() && rng.gen_bool(0.35) {
                *GLITCH_CHARS.choose(rng).unwrap()
            } else {
                c
            }
        })
        .collect()
}
```

---

## 8. Power Cycle / Flicker Effect

### 8.1 Trigger

The power cycle fires:
- When **switching tabs** (the body area dims → flickers → restores).
- When a **modal closes** (the background panel briefly cycles).

### 8.2 State

```rust
pub struct FlickerState {
    pub phase: FlickerPhase,
    pub ticks: u8,
}

pub enum FlickerPhase {
    /// Everything drops to `Dimmed` color scheme. (2 ticks)
    Dark,
    /// Brief flash back to full bright. (1 tick)
    Flash,
    /// Back to normal rendering. Terminal state.
    Done,
}
```

### 8.3 Rendering

During `FlickerPhase::Dark`, wrap every `Style` going into the render tree by
overriding `fg` to `Color::DarkGray` and `bg` to the dimmed panel background.
Apply this by threading `&app.flicker` through `draw_ui` and checking it
before each style assignment.

During `FlickerPhase::Flash`, temporarily use `Color::White` as `fg` on
borders to simulate an over-bright frame.

---

## 9. Data Decryption Reveal

### 9.1 Trigger

Fire whenever:
- A new **tab** is entered (the body content reveals over ~300 ms).
- A **modal** opens (its text content reveals over ~200 ms).

### 9.2 State

```rust
pub struct DecryptState {
    /// 0.0 = fully encrypted (all blocks), 1.0 = fully revealed.
    pub progress: f32,
    /// Ticks elapsed since reveal started (at 33 ms per tick).
    pub tick: u8,
    /// Total ticks for the full reveal (e.g. 9 ticks ≈ 300 ms).
    pub total_ticks: u8,
}
```

### 9.3 Rendering

For each character in a text run that is "not yet revealed":

```rust
fn decrypt_char(c: char, revealed_frac: f32, char_index: usize, total_chars: usize) -> char {
    // Each character has a threshold; characters below the threshold show blocks.
    let threshold = char_index as f32 / total_chars as f32;
    if revealed_frac < threshold {
        // Pick a block character based on how close we are to revealing.
        let block_idx = ((1.0 - revealed_frac) * 3.0) as usize;
        ['▓', '▒', '░', ' '][block_idx.min(3)]
    } else {
        c
    }
}
```

Apply per-`Span` in the render function:

```rust
let text: String = raw_text
    .chars()
    .enumerate()
    .map(|(i, c)| decrypt_char(c, decrypt.progress, i, raw_text.len()))
    .collect();
```

---

## 10. Pulse / Breathing Highlights

### 10.1 Purpose

Replace the static `reversed` background highlight on selected list items with
a sinusoidal brightness pulse.

### 10.2 State

```rust
/// Continuous phase angle (0.0 .. 2π), advanced by a fixed step per anim_tick.
pub pulse_phase: f32,
```

In `tick_anim()`:
```rust
// 0.08 rad per tick ≈ one full cycle every ~78 ticks ≈ 2.6 s at 30 FPS.
app.pulse_phase = (app.pulse_phase + 0.08) % (2.0 * std::f32::consts::PI);
```

### 10.3 Rendering

```rust
fn pulse_style(phase: f32) -> Style {
    // sin oscillates -1..1; map to a brightness range.
    let brightness = (phase.sin() * 0.5 + 0.5); // 0.0 .. 1.0
    // Interpolate between DarkGray background and Cyan background.
    let r = (brightness * 0.0) as u8;
    let g = (brightness * 200.0) as u8;
    let b = (brightness * 180.0) as u8;
    Style::default()
        .fg(Color::Black)
        .bg(Color::Rgb(r, g, b))
        .add_modifier(Modifier::BOLD)
}
```

Use `pulse_style(app.pulse_phase)` for the currently selected item instead of
`.add_modifier(Modifier::REVERSED)`.

---

## 11. Ambient UI Elements

### 11.1 Telemetry Ticker

A single-line bar at the **bottom of the terminal** (below the footer),
continuously scrolling cryptic hex and status messages left-to-right.

**Content examples:**
```
0x4A2F :: SYNC_OK :: pve-01 uptime=47d12h :: eth0 tx=1.2MB/s rx=430KB/s :: CRC_OK
```

**State:**
```rust
pub ticker_offset: usize,     // character offset into the ticker string
pub ticker_content: String,   // pre-built string, regenerated every ~60 s
```

**Rendering:** Render as a 1-row `Paragraph` with `Color::DarkGray`. Each
`anim_tick`, advance `ticker_offset` by 1. When the offset reaches the string
length, wrap around (the string is designed as a loop with a separator).

**Layout:** Add a `Constraint::Length(1)` at the very bottom of the root
layout in `draw_ui`. The tab-bar and body shrink by 1 row.

### 11.2 Micro-Graphs (Sparklines)

On the **Host Management** tab, each LXC row gets a 10-character braille
sparkline for CPU and RAM. Use `ratatui::widgets::Sparkline`.

**State:**
```rust
// One ring buffer per LXC container (max 60 samples).
pub lxc_cpu: HashMap<String, VecDeque<u64>>,
pub lxc_ram: HashMap<String, VecDeque<u64>>,
```

Mock data is pushed on each `anim_tick` with a gentle random walk. Real data
comes from the future WebSocket daemon.

**Rendering:** Inline in the LXC table row using a split: data columns (80%)
+ sparkline column (20%).

### 11.3 Scanline Sweep

An occasional horizontal highlight that sweeps down a specific panel.

**State:**
```rust
pub scanline: Option<ScanlineState>,

pub struct ScanlineState {
    pub panel: ScanlinePanel,   // which panel is being swept
    pub row: u16,               // current y position within the panel
    pub max_row: u16,           // panel height
}
```

**Trigger:** Once every ~10 seconds, fire a scanline on the LXC Containers
panel (Host Management) or the Logs panel.

**Rendering:** In the panel's render function, check if
`app.scanline.as_ref().map(|s| s.row)` matches the current list item row. If
so, overlay a `Style::default().bg(Color::Rgb(0, 40, 40))` ("dim teal") on
that row.

---

## 12. Tick Loop Architecture

The single authoritative tick loop lives in `main.rs`. No other module should
spawn timers.

```
main.rs
├── log_tick  (400 ms)  → app.tick_logs()
│     Appends a mock LogLine to app.logs, caps at 500 lines.
│
├── anim_tick (33 ms)   → app.tick_anim()
│     ├── advance pulse_phase
│     ├── advance ticker_offset
│     ├── step decrypt_state (if Some)
│     ├── step flicker_state (if Some)
│     ├── step scanline state (if Some)
│     ├── maybe trigger glitch (rng.gen_bool(0.004) per element)
│     └── push mock sparkline samples
│
└── keyboard events     → events::handle_key_event()
      No animation side-effects here. Navigation only.
```

`tick_anim()` is a pure state mutation method on `App`. It receives a mutable
reference to `App` plus a `&mut SmallRng` (seeded once in `App::new()`).

```rust
// app.rs
use rand::{SeedableRng, rngs::SmallRng, Rng};

pub struct App {
    // ... existing fields ...

    // ── Animation state ───────────────────────────────────────────────────
    pub pulse_phase:    f32,
    pub ticker_offset:  usize,
    pub ticker_content: String,
    pub title_glitch:   Option<GlitchState>,
    pub selected_glitch: Option<GlitchState>,
    pub flicker:        Option<FlickerState>,
    pub decrypt:        Option<DecryptState>,
    pub scanline:       Option<ScanlineState>,
    pub lxc_cpu:        std::collections::HashMap<String, std::collections::VecDeque<u64>>,
    pub lxc_ram:        std::collections::HashMap<String, std::collections::VecDeque<u64>>,
    rng: SmallRng,
}
```

---

## 13. Animation State in `App`

### 13.1 Initialisation (in `App::new()`)

```rust
rng: SmallRng::from_entropy(),
pulse_phase: 0.0,
ticker_offset: 0,
ticker_content: build_ticker_string(),  // generates the looping hex string
title_glitch: None,
selected_glitch: None,
flicker: None,
decrypt: Some(DecryptState { progress: 0.0, tick: 0, total_ticks: 9 }),
scanline: None,
lxc_cpu: HashMap::new(),
lxc_ram: HashMap::new(),
```

On startup, fire a `DecryptState` immediately so the initial render appears to
"boot up".

### 13.2 `tick_anim()` Pseudocode

```rust
pub fn tick_anim(&mut self) {
    // 1. Pulse
    self.pulse_phase = (self.pulse_phase + 0.08) % TAU;

    // 2. Ticker
    self.ticker_offset = (self.ticker_offset + 1) % self.ticker_content.len();

    // 3. Decrypt reveal
    if let Some(d) = &mut self.decrypt {
        d.tick += 1;
        d.progress = d.tick as f32 / d.total_ticks as f32;
        if d.tick >= d.total_ticks { self.decrypt = None; }
    }

    // 4. Flicker
    if let Some(f) = &mut self.flicker {
        match f.phase {
            FlickerPhase::Dark  => { f.ticks += 1; if f.ticks >= 2 { f.phase = FlickerPhase::Flash; f.ticks = 0; } }
            FlickerPhase::Flash => { f.phase = FlickerPhase::Done; }
            FlickerPhase::Done  => { self.flicker = None; }
        }
    }

    // 5. Scanline
    if let Some(s) = &mut self.scanline {
        s.row += 1;
        if s.row >= s.max_row { self.scanline = None; }
    }

    // 6. Glitch roll
    if self.rng.gen_bool(0.004) && self.title_glitch.is_none() {
        let title = current_active_title(self);
        self.title_glitch = Some(GlitchState {
            ticks_remaining: self.rng.gen_range(2..=3),
            scrambled: scramble(title, &mut self.rng),
        });
    }
    if let Some(g) = &mut self.title_glitch {
        if g.ticks_remaining == 0 { self.title_glitch = None; }
        else { g.ticks_remaining -= 1; }
    }

    // 7. Mock sparkline samples
    for cpu in self.lxc_cpu.values_mut() {
        let last = cpu.back().copied().unwrap_or(30);
        let delta = self.rng.gen_range(-5i64..=5) as i64;
        let next = (last as i64 + delta).clamp(0, 100) as u64;
        cpu.push_back(next);
        if cpu.len() > 60 { cpu.pop_front(); }
    }
    // ... same for lxc_ram
}
```

---

## 14. Implementation Checklist

Use this as a progressive enhancement list. Ship features in order; do not
skip ahead.

### Phase A — Infrastructure (no visible change yet)

- [ ] Add `rng: SmallRng` to `App`; seed in `new()`
- [ ] Add `anim_tick` interval (33 ms) to `main.rs` event loop
- [ ] Add `tick_anim()` stub to `App` (no-op initially)
- [ ] Add all animation state fields to `App` with zero/`None` defaults

### Phase B — Ambient Motion

- [ ] Implement `pulse_phase` advance in `tick_anim()` and apply `pulse_style()` to selected items in Scaffolding and Host Management
- [ ] Implement `ticker_content` + `ticker_offset` and render ticker bar at the bottom of `draw_ui()`
- [ ] Implement mock sparkline samples in `tick_anim()` and render `Sparkline` widgets in the LXC table

### Phase C — Transition Effects

- [ ] Implement `DecryptState` + `tick_anim()` step; apply `decrypt_char()` in `draw_logs()` and modal renders
- [ ] Fire `DecryptState` on tab switch (in `tab_right()` / `tab_left()`)
- [ ] Implement `FlickerState` + fire on modal close

### Phase D — Glitch

- [ ] Implement `scramble()` helper and `GlitchState`
- [ ] Roll glitch chance in `tick_anim()` and apply scrambled title in `draw_ui()` tab bar
- [ ] Apply selected-item glitch in Scaffolding list renderer

### Phase E — Scanline

- [ ] Implement `ScanlineState`; trigger every ~300 anim ticks
- [ ] Apply scanline overlay in `draw_host_management()` LXC table
- [ ] Apply scanline overlay in `draw_logs()` log list

### Phase F — Border Upgrade

- [ ] Replace `BorderType::Rounded` with HUD corner-only borders on focused panels
- [ ] Add `▀` dividers between major sections in Dashboard tab

---

*Last updated: 2026-05-26*  
*Applies to: `client-app/` — all files under `src/`*

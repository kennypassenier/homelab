# Client TUI — Refactor Status

Last updated: pre-reboot checkpoint (session May 2026).

## Overview

The Rust `client-app/src/main.rs` has been split into logical modules. All new
files are written and correct. **One cleanup step remains** (see below).

---

## Module Layout (target state)

| File | Contents | Status |
|---|---|---|
| `src/main.rs` | Entry point + slim event loop only (~97 lines) | ⚠️ needs tail trimmed |
| `src/app.rs` | `Tab`, `App`, `AppDropdown`, `StackDropdown`, all `impl App` | ✅ done |
| `src/ui.rs` | `draw_ui()` + per-tab renderers | ✅ done |
| `src/events.rs` | `handle_key_event()`, `EventOutcome`, wizard dispatch | ✅ done |
| `src/blast_radius.rs` | `ActiveModal`, wizard state types, modal draw functions | ✅ unchanged |
| `src/scaffold.rs` | `AppServiceTemplate`, `create_app_dirs()`, `scaffold_stack_with_services()` | ✅ unchanged |
| `src/gitops.rs` | `commit_and_push()` | ✅ unchanged |
| `src/theme.rs` | `Theme::cyberpunk()` | ✅ unchanged |
| `src/app_list.rs` | `list_apps_for_stack()` | ✅ fixed path bug |

---

## Bug fixes included

- `app_list.rs` line 7: `"../stacks/{}"` → `"stacks/{}"` (binary CWD is homelab root)
- `main.rs` (via `app.rs`): `load_stacks()` now reads `"stacks"` not `"../stacks"` — stacks
  will show correctly in the Scaffolding tab.

---

## One remaining task — trim main.rs

`main.rs` currently has the new 97-line body at the top, but the old duplicate
code (structs, old `fn main`, old `async_main`) is still appended from line 100.

**After reboot, run this once:**

```bash
cd /home/kenny/Projects/homelab
python3 -c "
lines = open('client-app/src/main.rs').readlines()
open('client-app/src/main.rs','w').writelines(lines[:97])
print('main.rs is now', len(open('client-app/src/main.rs').readlines()), 'lines')
"
```

Then verify the build is clean:

```bash
cd client-app && cargo build --release 2>&1 | head -40
```

---

## Event loop architecture (correct pattern)

```
loop {
    terminal.draw(|f| ui::draw_ui(f, &app))?;   // draw FIRST, unconditionally

    tokio::select! {
        _ = &mut sigint => { cleanup; return; }
        res = async { poll → read → events::handle_key_event(&mut app, key) } => {
            if res.is_err() { break; }
        }
    }
}
```

Key rule: `terminal.draw` must **never** be inside the `select!` async block.

---

## Why the terminal was stuck

A prior TUI test run left the terminal in raw/alternate-buffer mode. All
subsequent shell commands appeared to open the alternate buffer with no output.
After rebooting the problem will be gone.

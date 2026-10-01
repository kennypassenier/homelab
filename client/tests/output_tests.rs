//! What the command line prints, as opposed to what it asks the host.

use homelab_client::output::{Tone, check_tones};

/// check-output-buries-problem (expert panel, 2026-09-27): the whole check
/// was printed in red, `noted` ("nothing to do") included, so nine red lines
/// hid the one that needed action. Each group keeps its own colour.
/// covers: fix-103
#[test]
fn fix_103_each_severity_group_is_printed_in_its_own_tone() {
    let msg = "fleet check: 1 broken · 1 drift · 1 noted (nothing to do)\n\
               broken — not doing its job now:\n  [broken] a — x\n      remedy: r\n\
               drift — works, bites on the next deploy or outage:\n  [drift] b — y\n      remedy: r\n\
               noted — nothing to do:\n  [noted] c — z\n      remedy: nothing";
    let tones: Vec<Tone> = check_tones(msg).into_iter().map(|(t, _)| t).collect();
    assert_eq!(
        tones,
        vec![
            Tone::Broken,
            Tone::Broken,
            Tone::Broken,
            Tone::Broken,
            Tone::Drift,
            Tone::Drift,
            Tone::Drift,
            Tone::Noted,
            Tone::Noted,
            Tone::Noted,
        ]
    );
    // The summary takes the tone of the worst group in it.
    let only_noted = check_tones("fleet check: 2 noted (nothing to do)\nnoted — nothing to do:");
    assert_eq!(only_noted[0].0, Tone::Noted);
    assert_eq!(
        check_tones("fleet check: repo and reality agree")[0].0,
        Tone::Plain
    );
}

/// colour-codes-when-piped (expert panel, 2026-09-27): the colours were
/// hard-coded escape codes, printed into pipes, logs and saved output, so a
/// grep over them broke. Colour only on a terminal, and never with NO_COLOR.
/// covers: fix-109
#[test]
fn fix_109_colour_only_on_a_terminal_without_no_color() {
    use homelab_client::output::should_colour;
    assert!(should_colour(true, None));
    assert!(!should_colour(false, None), "a pipe gets no escape codes");
    assert!(!should_colour(true, Some("1")), "NO_COLOR wins");
    assert!(
        should_colour(true, Some("")),
        "an empty NO_COLOR is not set (no-color.org)"
    );
}

/// The real binary, its output captured the way a pipe or a log does.
/// covers: fix-109
#[test]
fn fix_109_piped_output_carries_no_escape_codes() {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap();
    let out = std::process::Command::new(env!("CARGO_BIN_EXE_homelab"))
        .args(["plan", "stacks/drill"])
        .current_dir(root)
        .env_remove("NO_COLOR")
        .output()
        .unwrap();
    let text = String::from_utf8_lossy(&out.stdout);
    assert!(text.contains("valid"), "{}", text);
    assert!(!text.contains('\x1b'), "escape codes in a pipe: {:?}", text);
}

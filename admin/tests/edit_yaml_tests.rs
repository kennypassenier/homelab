//! feat-stacks-2, feat-firewall-1: comment-keeping edits of the real stack
//! files (read from the repository, so a new layout there is tried here).

use homelab_admin::core::yamledit::{EditError, Item, Op, edit, path, value_at};
use serde_yaml::Value;

fn stack_file(name: &str) -> String {
    let p = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../stacks")
        .join(name)
        .join("lxc-compose.yml");
    std::fs::read_to_string(p).unwrap()
}

fn yaml(s: &str) -> Value {
    serde_yaml::from_str(s).unwrap()
}

fn rules_len(text: &str) -> usize {
    let v: Value = serde_yaml::from_str(text).unwrap();
    value_at(&v, &path("firewall.rules"))
        .and_then(|r| r.as_sequence())
        .map(|q| q.len())
        .unwrap_or(0)
}

#[test]
fn feat_stacks_2_keeping_every_item_changes_nothing() {
    for stack in ["kp-soft", "admin", "gateway", "media", "metrics"] {
        let text = stack_file(stack);
        let n = rules_len(&text);
        if n == 0 {
            continue;
        }
        let items = (0..n).map(Item::Keep).collect();
        let out = edit(
            &text,
            &[Op::Seq {
                path: path("firewall.rules"),
                items,
            }],
        )
        .unwrap();
        assert_eq!(out, text, "{stack}");
    }
}

#[test]
fn feat_stacks_2_a_changed_number_keeps_the_comments_around_it() {
    let text = stack_file("kp-soft");
    let out = edit(
        &text,
        &[
            Op::Set {
                path: path("resources.memory_mb"),
                value: Value::from(3072),
            },
            Op::Set {
                path: path("boot.order"),
                value: Value::from(80),
            },
        ],
    )
    .unwrap();
    assert!(out.contains("  memory_mb: 3072\n"));
    assert!(out.contains("  order: 80\n"));
    // Every comment line of the file is still there, in order.
    let comments = |t: &str| {
        t.lines()
            .filter(|l| l.trim_start().starts_with('#'))
            .map(str::to_string)
            .collect::<Vec<_>>()
    };
    assert_eq!(comments(&out), comments(&text));
    assert_eq!(out.lines().count(), text.lines().count());
}

#[test]
fn feat_stacks_2_a_trailing_comment_on_the_line_stays() {
    let text = "a: 1\nb: 2   # why two\nc:\n  d: x # note\n";
    let out = edit(
        text,
        &[
            Op::Set {
                path: path("b"),
                value: Value::from(3),
            },
            Op::Set {
                path: path("c.d"),
                value: Value::from("y z"),
            },
            Op::Set {
                path: path("c.e"),
                value: Value::from(true),
            },
            Op::Set {
                path: path("f"),
                value: yaml("{g: 1, h: [a, b]}"),
            },
        ],
    )
    .unwrap();
    assert_eq!(
        out,
        "a: 1\nb: 3   # why two\nc:\n  d: y z # note\n  e: true\nf:\n  g: 1\n  h:\n    - a\n    - b\n"
    );
}

#[test]
fn feat_firewall_1_a_rule_is_added_changed_and_removed_with_its_comments() {
    let text = stack_file("kp-soft");
    let n = rules_len(&text);
    let new_rule = yaml(
        "{dir: in, action: ACCEPT, source: 10.10.10.20, proto: tcp, dport: '8080', note: the dashboard}",
    );
    // Remove the last rule (ssh from the desktop), change the first one's
    // note, add one at the end.
    let mut items: Vec<Item> = (0..n - 1).map(Item::Keep).collect();
    items[0] = Item::Retext(
        0,
        yaml(
            "{comment: the rescue address, dir: out, action: DROP, dest: 10.10.10.250, note: never}",
        ),
    );
    items.push(Item::New(new_rule));
    let out = edit(
        &text,
        &[Op::Seq {
            path: path("firewall.rules"),
            items,
        }],
    )
    .unwrap();
    assert!(
        out.contains("    - comment: the rescue address\n      dir: out\n      action: DROP\n      dest: 10.10.10.250\n      note: never\n"),
        "{out}"
    );
    assert!(!out.contains("source: 10.10.10.10"));
    assert!(out.contains("    - dir: in\n      action: ACCEPT\n      source: 10.10.10.20\n      proto: tcp\n      dport: '8080'\n      note: the dashboard\n"), "{out}");
    // A kept rule's own note (not retexted, not removed) stayed untouched.
    // (Kuma's rules and their form-item provenance comment were retired
    // with Uptime Kuma on 2026-10-01; this note on the Loki rule is the
    // next one down that a Keep leaves alone.)
    assert!(out.contains("note: Loki push (Alloy), added 2026-09-25"));
    assert_eq!(rules_len(&out), n);
}

#[test]
fn feat_firewall_1_a_moved_rule_takes_its_comment_lines_along() {
    let text = "firewall:\n  rules:\n    # first\n    - dir: in\n      action: ACCEPT\n    # second\n    - dir: out\n      action: DROP\n";
    let out = edit(
        text,
        &[Op::Seq {
            path: path("firewall.rules"),
            items: vec![Item::Keep(1), Item::Keep(0)],
        }],
    )
    .unwrap();
    assert_eq!(
        out,
        "firewall:\n  rules:\n    # second\n    - dir: out\n      action: DROP\n    # first\n    - dir: in\n      action: ACCEPT\n"
    );
}

#[test]
fn feat_firewall_1_an_empty_list_grows_and_a_flow_list_stays_flow() {
    let text =
        "apps: [kp-soft, jobtracker]\nnatives: []\nfirewall:\n  enabled: false\n  rules: []\n";
    let out = edit(
        text,
        &[
            Op::Seq {
                path: path("apps"),
                items: vec![
                    Item::Keep(0),
                    Item::Keep(1),
                    Item::New(Value::from("new-app")),
                ],
            },
            Op::Seq {
                path: path("firewall.rules"),
                items: vec![Item::New(yaml("{dir: in, action: ACCEPT}"))],
            },
        ],
    )
    .unwrap();
    assert_eq!(
        out,
        "apps: [kp-soft, jobtracker, new-app]\nnatives: []\nfirewall:\n  enabled: false\n  rules:\n    - dir: in\n      action: ACCEPT\n"
    );
    // And back to empty.
    let back = edit(
        &out,
        &[Op::Seq {
            path: path("firewall.rules"),
            items: vec![],
        }],
    )
    .unwrap();
    assert!(back.contains("  rules: []\n"), "{back}");
}

#[test]
fn feat_stacks_2_a_multi_line_comment_value_is_written_as_a_block() {
    let text = "firewall:\n  enabled: true\n  comment: |-\n    old line one\n    old line two\n  policy_in: DROP\n";
    let out = edit(
        text,
        &[Op::Set {
            path: path("firewall.comment"),
            value: Value::from("new one\nnew two"),
        }],
    )
    .unwrap();
    assert_eq!(
        out,
        "firewall:\n  enabled: true\n  comment: |-\n    new one\n    new two\n  policy_in: DROP\n"
    );
}

#[test]
fn feat_stacks_2_what_the_editor_cannot_place_is_refused_not_guessed() {
    assert!(matches!(
        edit(
            "a: 1\n",
            &[Op::Set {
                path: path("b.c"),
                value: Value::from(1)
            }]
        ),
        Err(EditError::NotFound(_))
    ));
    assert!(matches!(
        edit(
            "a: {b: 1}\n",
            &[Op::Set {
                path: path("a.b"),
                value: Value::from(2)
            }]
        ),
        Err(EditError::Unsupported(_))
    ));
    assert!(matches!(edit("a: [\n", &[]), Err(EditError::Parse(_))));
}

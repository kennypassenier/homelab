//! small-sharp-edges (expert panel, 2026-09-27).

use homelab_client::version::{template_build_args, TemplateArgs};

fn s(v: &[&str]) -> Vec<String> {
    v.iter().map(|x| x.to_string()).collect()
}

/// `template-build` turned an argument it could not parse into vmid 999 and
/// version 1: the F309 pattern, which the `--help` guard closed for
/// `--help` only. A typo would have built a template.
/// covers: fix-110
#[test]
fn fix_110_template_build_refuses_an_argument_it_cannot_read() {
    assert_eq!(
        template_build_args(&[]),
        Ok(TemplateArgs {
            temp_vmid: 999,
            version: 1,
            unprivileged: true,
            base_template: None,
        })
    );
    assert_eq!(
        template_build_args(&s(&["998", "5", "--privileged", "--base", "d13.tar.zst"])),
        Ok(TemplateArgs {
            temp_vmid: 998,
            version: 5,
            unprivileged: false,
            base_template: Some("d13.tar.zst".into()),
        })
    );
    assert!(
        template_build_args(&s(&["99o"])).is_err(),
        "a typo in the vmid"
    );
    assert!(
        template_build_args(&s(&["998", "v5"])).is_err(),
        "a typo in the version"
    );
    assert!(
        template_build_args(&s(&["998", "5", "6"])).is_err(),
        "a stray argument"
    );
    assert!(
        template_build_args(&s(&["--base"])).is_err(),
        "--base without a value"
    );
    assert!(
        template_build_args(&s(&["--priviliged"])).is_err(),
        "a mistyped flag"
    );
}

fn run(args: &[&str]) -> std::process::Output {
    let home = std::env::temp_dir().join(format!("homelab-sharp-{}", std::process::id()));
    let _ = std::fs::create_dir_all(&home);
    std::process::Command::new(env!("CARGO_BIN_EXE_homelab"))
        .args(args)
        .current_dir(&home)
        .env("HOME", &home)
        .env_remove("HOMELAB_TOKEN")
        .env_remove("HOMELAB_REPO")
        .output()
        .unwrap()
}

/// `homelab new` (and `testplan`) never reach the host but demanded a
/// token, and the refusal named no remedy.
/// covers: fix-110
#[test]
fn fix_110_local_verbs_need_no_token_and_the_refusal_says_where_it_goes() {
    let out = run(&["new"]);
    let err = String::from_utf8_lossy(&out.stderr);
    assert!(
        !err.contains("HOMELAB_TOKEN"),
        "new asked for a token: {}",
        err
    );
    assert!(err.contains("usage: homelab new"), "{}", err);

    let out = run(&["status"]);
    let err = String::from_utf8_lossy(&out.stderr);
    assert!(err.contains("HOMELAB_TOKEN is not set"), "{}", err);
    assert!(err.contains("~/.config/homelab/env"), "no remedy: {}", err);
}

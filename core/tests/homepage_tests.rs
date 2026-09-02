//! T51: the front page rendered from the routes the orchestrator already
//! writes, instead of a file somebody maintains.

use homelab_core::ops::homepage::{entries_from_route, services_yaml, Entry};

/// The real media route fragment, verbatim from CT 104 on 2026-09-01.
const MEDIA_ROUTE: &str = r#"# Traefik route fragment for the media stack.
#
# Six names, all at the address that does not change in a C4 rebuild.

http:
  routers:
    jellyfin:
      rule: "Host(`fin.kp-soft.dev`)"
      entryPoints: [web]
      service: jellyfin
    sonarr:
      rule: "Host(`son.kp-soft.dev`)"
      entryPoints: [web]
      service: sonarr

  services:
    jellyfin:
      loadBalancer:
        servers:
          - url: "http://10.10.10.6:8096"
"#;

/// covers: F166
#[test]
fn a_route_fragment_yields_its_hostnames_and_its_backends() {
    let got = entries_from_route(MEDIA_ROUTE);
    assert_eq!(
        got,
        vec![
            Entry {
                app: "jellyfin".into(),
                host: "fin.kp-soft.dev".into(),
                backend: Some("http://10.10.10.6:8096".into()),
            },
            Entry {
                app: "sonarr".into(),
                host: "son.kp-soft.dev".into(),
                backend: None,
            },
        ],
        "the hostname is the front door, the backend is what a widget must call"
    );

    // The original rule still holds and is the one that matters: the
    // internal address may never become a LINK. It was excluded from the
    // parser entirely until 2026-09-02, which also made widgets impossible —
    // a widget going out through the public name meets Cloudflare Access and
    // gets a login page instead of an answer (Kenny's own note in the file
    // this replaced). So it is carried, and used for widgets only.
    let page = services_yaml(&[("media".into(), got)], None, &Default::default());
    for line in page.lines() {
        if line.trim_start().starts_with("href:") || line.trim_start().starts_with("siteMonitor:") {
            assert!(
                !line.contains("10.10.10.6"),
                "an internal address must never become a link: {}",
                line
            );
        }
    }
}

#[test]
fn a_stack_without_routes_is_skipped_rather_than_rendered_empty() {
    let out = services_yaml(
        &[
            ("registry".into(), vec![]),
            (
                "media".into(),
                vec![Entry {
                    app: "jellyfin".into(),
                    host: "fin.kp-soft.dev".into(),
                    backend: None,
                }],
            ),
        ],
        None,
        &Default::default(),
    );
    assert!(
        !out.contains("registry"),
        "an empty heading reads as 'broken' when it means 'no front door': {}",
        out
    );
    assert!(out.contains("- media:"));
    assert!(out.contains("href: https://fin.kp-soft.dev/"));
}

#[test]
fn rendering_twice_produces_the_same_bytes() {
    let stacks = vec![(
        "media".into(),
        vec![
            Entry {
                app: "jellyfin".into(),
                host: "fin.kp-soft.dev".into(),
                backend: None,
            },
            Entry {
                app: "sonarr".into(),
                host: "son.kp-soft.dev".into(),
                backend: None,
            },
        ],
    )];
    assert_eq!(
        services_yaml(&stacks, None, &Default::default()),
        services_yaml(&stacks, None, &Default::default()),
        "a deploy that changes nothing must write an identical file, or every \
         deploy reports a change that is not one"
    );
}

/// covers: F190
///
/// A generated file lands in a directory that belongs to a CONTAINER, and
/// the orchestrator writes it as host root. On an unprivileged container
/// that is the wrong owner, and Uptime Kuma proves it is not cosmetic: it
/// chowns everything under `/app/data` at startup, cannot own a host-root
/// file, exits non-zero and crash-loops. That is how the monitoring went
/// down on 2026-09-02 — over a backup file, and measuring afterwards found
/// this code had written two files with exactly the same problem.
#[tokio::test]
async fn a_generated_file_takes_the_owner_of_the_directory_it_lands_in() {
    let exec = homelab_core::executor::MockExecutor::new();
    homelab_core::ops::util::write_file_owned_like_dir(
        &exec,
        "/appdata/home/homepage-config/services.yaml",
        "- gateway:\n",
        0o644,
    )
    .await
    .unwrap();

    let chowns = exec.calls_containing("chown");
    assert_eq!(
        chowns.len(),
        1,
        "expected exactly one chown after the write, got {:?}",
        exec.calls()
    );
    assert!(
        chowns[0].contains("--reference")
            && chowns[0].contains("/appdata/home/homepage-config")
            && chowns[0].contains("/appdata/home/homepage-config/services.yaml"),
        "the owner must be copied from the parent directory, not computed: {}",
        chowns[0]
    );
}

/// The chown is best-effort: a convenience file must never fail a deploy.
#[tokio::test]
async fn a_failing_chown_does_not_fail_the_write() {
    let exec = homelab_core::executor::MockExecutor::new();
    exec.enqueue(
        "chown",
        homelab_core::executor::CmdOutput::failed(1, "chown: not permitted"),
    );
    let r = homelab_core::ops::util::write_file_owned_like_dir(
        &exec,
        "/appdata/uptime/kuma-seeder-config/host-monitors.json",
        "{}",
        0o644,
    )
    .await;
    assert!(r.is_ok(), "a failed chown must not fail the write");
}

/// covers: F188
///
/// The real overlay, verbatim from the repo, joined with routes shaped like
/// the ones the gateway actually holds. The join key is `href` because
/// display names were measured against the real pair of files and would
/// have guessed wrong eight times out of twenty-six.
#[test]
fn the_overlay_supplies_what_a_route_cannot_and_nothing_else() {
    use homelab_core::ops::homepage::parse_overlay;
    let ov = parse_overlay(include_str!(
        "../../stacks/home/homepage/services-overlay.yml"
    ));
    assert_eq!(ov.blocks.len(), 27, "the real overlay has 27 entries");
    assert_eq!(
        ov.blocks.iter().filter(|b| b.hide).count(),
        2,
        "two routes are deliberately kept off the page: the page itself, and \
         a second name for a service already listed"
    );
    assert_eq!(ov.group_order.first().map(String::as_str), Some("Media"));

    let out = services_yaml(
        &[
            (
                "media".into(),
                vec![
                    Entry {
                        app: "jellyfin".into(),
                        host: "fin.kp-soft.dev".into(),
                        backend: Some("http://10.10.10.6:8096".into()),
                    },
                    // No overlay entry: a service nobody has described yet.
                    Entry {
                        app: "flaresolverr".into(),
                        host: "flare.kp-soft.dev".into(),
                        backend: None,
                    },
                ],
            ),
            (
                "gateway".into(),
                vec![Entry {
                    app: "grafana".into(),
                    host: "grafana.kp-soft.dev".into(),
                    backend: None,
                }],
            ),
        ],
        Some(&ov),
        &Default::default(),
    );

    // Kenny's name, group and icon survive the merge, and the widget is
    // generated beside them rather than written by him (V6b).
    assert!(out.contains("- Media:"), "{}", out);
    assert!(out.contains("    - Jellyfin:"), "{}", out);
    assert!(out.contains("icon: jellyfin.svg"), "{}", out);
    assert!(out.contains("type: jellyfin"), "{}", out);
    assert!(out.contains("enableNowPlaying: true"), "{}", out);

    // A routed service with no overlay entry still appears — under its
    // stack, with the plain link. That is the whole point: a new service
    // shows up by itself.
    assert!(
        out.contains("- media:"),
        "unknown service keeps its stack: {}",
        out
    );
    assert!(out.contains("    - flaresolverr:"), "{}", out);
    assert!(
        out.contains("siteMonitor: https://flare.kp-soft.dev/"),
        "{}",
        out
    );

    // Grafana is Kenny's, and he filed it under Infrastructuur rather than
    // under the gateway stack it happens to run in.
    assert!(out.contains("    - Grafana:"), "{}", out);
    let infra = out.find("- Infrastructuur:").expect("group missing");
    let graf = out.find("    - Grafana:").unwrap();
    assert!(
        graf > infra,
        "Grafana must land in the group the overlay names"
    );

    // Two overlay entries have no route at all and are deliberate: the one
    // http:// link on the page, and a deep link into Grafana for a service
    // with no page of its own. Dropping them would delete a decision.
    assert!(
        out.contains("    - kp-soft:"),
        "manual link dropped: {}",
        out
    );
    assert!(out.contains("http://10.10.10.16:8080/"), "{}", out);
    assert!(out.contains("    - Loki:"), "manual link dropped: {}", out);

    // The named groups come first, in the overlay's order.
    let media = out.find("- Media:").unwrap();
    assert!(media < infra, "group_order must be honoured");
}

/// Same input, same bytes — a deploy that changes nothing must report
/// "unchanged" rather than rewrite the file every night.
#[test]
fn merging_twice_produces_the_same_bytes() {
    use homelab_core::ops::homepage::parse_overlay;
    let ov = parse_overlay(include_str!(
        "../../stacks/home/homepage/services-overlay.yml"
    ));
    let stacks = vec![(
        "media".into(),
        vec![Entry {
            app: "jellyfin".into(),
            host: "fin.kp-soft.dev".into(),
            backend: None,
        }],
    )];
    assert_eq!(
        services_yaml(&stacks, Some(&ov), &Default::default()),
        services_yaml(&stacks, Some(&ov), &Default::default())
    );
}

/// covers: F195
///
/// Kenny asked whether the live tiles — what is streaming, how many films —
/// could be automatic. All three parts of a widget turned out to be
/// readable: the type from the app name, the address from the route's own
/// backend, and the key from the application itself.
#[test]
fn a_widget_is_derived_from_the_route_and_the_application() {
    use homelab_core::ops::homepage::{parse_overlay, widget_for};
    let mut keys = std::collections::HashMap::new();
    keys.insert("jellyfin".to_string(), "THEKEY".to_string());

    let ov = parse_overlay(include_str!(
        "../../stacks/home/homepage/services-overlay.yml"
    ));
    let out = services_yaml(
        &[(
            "media".into(),
            vec![Entry {
                app: "jellyfin".into(),
                host: "fin.kp-soft.dev".into(),
                backend: Some("http://10.10.10.6:8096".into()),
            }],
        )],
        Some(&ov),
        &keys,
    );

    assert!(out.contains("widget:"), "no widget generated: {}", out);
    assert!(out.contains("type: jellyfin"), "{}", out);
    // The LAN address, not the public name: through Cloudflare Access a
    // widget gets a login page instead of an answer.
    assert!(out.contains("url: http://10.10.10.6:8096"), "{}", out);
    assert!(out.contains("key: THEKEY"), "{}", out);
    assert!(out.contains("enableNowPlaying: true"), "{}", out);
    // And Kenny's own lines are still there, above it.
    assert!(out.contains("icon: jellyfin.svg"), "{}", out);
    assert!(out.contains("description: Films en series"), "{}", out);

    // The route name and the widget name differ for seerr, and the table is
    // where that lives rather than in anybody's config file.
    assert_eq!(widget_for("seerr").map(|w| w.kind), Some("jellyseerr"));
    assert!(widget_for("qbittorrent").is_none(), "no widget invented");
}

/// A key that cannot be read must fail visibly rather than vanish: the tile
/// still gets a widget, pointing at Homepage's own variable, so Homepage
/// reports the problem instead of the widget quietly not being there.
#[test]
fn a_missing_key_still_produces_a_widget() {
    let out = services_yaml(
        &[(
            "media".into(),
            vec![Entry {
                app: "sonarr".into(),
                host: "son.kp-soft.dev".into(),
                backend: Some("http://10.10.10.6:8989".into()),
            }],
        )],
        None,
        &Default::default(),
    );
    assert!(out.contains("type: sonarr"), "{}", out);
    assert!(
        out.contains(r#"key: "{{HOMEPAGE_VAR_SONARR}}""#),
        "the fallback must be QUOTED — an unquoted {{ is a YAML flow mapping: {}",
        out
    );
}

/// covers: F195
///
/// Not every route is a front door. Two routers may forward to the same
/// address (`almanac` and `almanac-block-metrics`), and some routes exist
/// for a reason that is not "put me on the page" — a second name for a
/// service already listed, and the page you are looking at.
#[test]
fn a_second_route_to_the_same_door_is_not_a_second_tile() {
    use homelab_core::ops::homepage::parse_overlay;
    let ov = parse_overlay(
        "group_order: [Huis]\n\
         \n\
         - href: https://almanac.kp-soft.dev/\n\
         \x20 group: Huis\n\
         \x20 name: Almanac\n\
         \n\
         - href: https://home.kp-soft.dev/\n\
         \x20 hide: true\n",
    );
    let out = services_yaml(
        &[(
            "almanac".into(),
            vec![
                Entry {
                    app: "almanac".into(),
                    host: "almanac.kp-soft.dev".into(),
                    backend: None,
                },
                Entry {
                    app: "almanac-block-metrics".into(),
                    host: "almanac.kp-soft.dev".into(),
                    backend: None,
                },
                Entry {
                    app: "homepage".into(),
                    host: "home.kp-soft.dev".into(),
                    backend: None,
                },
            ],
        )],
        Some(&ov),
        &Default::default(),
    );

    assert_eq!(
        out.matches("almanac.kp-soft.dev").count(),
        2,
        "one tile, so href + siteMonitor — not two tiles: {}",
        out
    );
    assert!(
        !out.contains("almanac-block-metrics"),
        "the second router to the same door must not become a tile: {}",
        out
    );
    assert!(
        !out.contains("home.kp-soft.dev"),
        "a hidden route must not appear at all: {}",
        out
    );
}

/// covers: F197
///
/// The bug this catches shipped once and no test saw it: every assertion
/// asked whether a line was PRESENT, not where it sat. The overlay's lines
/// were emitted at four spaces where a service's fields belong at eight, so
/// the file was invalid YAML that read fine to a human.
#[test]
fn overlay_lines_sit_under_their_service_not_beside_it() {
    use homelab_core::ops::homepage::parse_overlay;
    let ov = parse_overlay(include_str!(
        "../../stacks/home/homepage/services-overlay.yml"
    ));
    let out = services_yaml(
        &[(
            "media".into(),
            vec![Entry {
                app: "jellyfin".into(),
                host: "fin.kp-soft.dev".into(),
                backend: Some("http://10.10.10.6:8096".into()),
            }],
        )],
        Some(&ov),
        &Default::default(),
    );
    for line in out.lines() {
        let t = line.trim_start();
        if t.starts_with("icon:") || t.starts_with("description:") || t.starts_with("widget:") {
            let indent = line.len() - t.len();
            assert_eq!(
                indent, 8,
                "a service's fields belong at eight spaces, got {}: {:?}",
                indent, line
            );
        }
    }
}

/// covers: F197
///
/// Media is first because Kenny asked for it, and within Media Jellyfin is
/// first for the same reason. Without an explicit order the tiles follow
/// whichever stack sorted first, which put qBittorrent (stack `downloader`)
/// above Jellyfin (stack `media`).
#[test]
fn tiles_follow_the_order_written_in_the_overlay() {
    use homelab_core::ops::homepage::parse_overlay;
    let ov = parse_overlay(include_str!(
        "../../stacks/home/homepage/services-overlay.yml"
    ));
    let out = services_yaml(
        &[
            (
                "downloader".into(),
                vec![Entry {
                    app: "qbittorrent".into(),
                    host: "qbit.kp-soft.dev".into(),
                    backend: None,
                }],
            ),
            (
                "media".into(),
                vec![Entry {
                    app: "jellyfin".into(),
                    host: "fin.kp-soft.dev".into(),
                    backend: None,
                }],
            ),
        ],
        Some(&ov),
        &Default::default(),
    );
    let jf = out.find("- Jellyfin:").expect("jellyfin missing");
    let qb = out.find("- qBittorrent:").expect("qbittorrent missing");
    assert!(
        jf < qb,
        "the overlay puts Jellyfin above qBittorrent; the stack order must not win:\n{}",
        out
    );
}

/// covers: F200
///
/// The test that should have existed from the first line of this generator.
/// Two defects shipped because every assertion asked whether some text was
/// PRESENT: fields emitted at four spaces instead of eight (F197), and an
/// unquoted `{{HOMEPAGE_VAR_X}}`, which YAML reads as a flow mapping and
/// which made Homepage render a YAMLException instead of a page. Both are
/// invisible to `contains()` and obvious to a parser.
#[test]
fn the_generated_page_is_valid_yaml() {
    use homelab_core::ops::homepage::parse_overlay;
    let ov = parse_overlay(include_str!(
        "../../stacks/home/homepage/services-overlay.yml"
    ));
    let mut keys = std::collections::HashMap::new();
    keys.insert("jellyfin".to_string(), "deadbeef".to_string());

    let out = services_yaml(
        &[
            (
                "media".into(),
                vec![
                    Entry {
                        app: "jellyfin".into(),
                        host: "fin.kp-soft.dev".into(),
                        backend: Some("http://10.10.10.6:8096".into()),
                    },
                    // No key on disk: takes the quoted Homepage variable.
                    Entry {
                        app: "sonarr".into(),
                        host: "son.kp-soft.dev".into(),
                        backend: Some("http://10.10.10.6:8989".into()),
                    },
                ],
            ),
            (
                "paperwork".into(),
                vec![Entry {
                    app: "paperless".into(),
                    host: "docs.kp-soft.dev".into(),
                    backend: Some("http://10.10.10.14:8000".into()),
                }],
            ),
            (
                "gateway".into(),
                vec![Entry {
                    app: "grafana".into(),
                    host: "grafana.kp-soft.dev".into(),
                    backend: Some("http://10.10.10.4:3000".into()),
                }],
            ),
        ],
        Some(&ov),
        &keys,
    );

    let parsed: serde_yaml::Value = serde_yaml::from_str(&out)
        .unwrap_or_else(|e| panic!("Homepage would refuse this file: {}\n---\n{}", e, out));

    // And it has the shape Homepage expects: a sequence of one-key groups,
    // each holding a sequence of one-key services.
    let groups = parsed.as_sequence().expect("a list of groups");
    assert!(!groups.is_empty());
    for g in groups {
        let m = g.as_mapping().expect("each group is a mapping");
        assert_eq!(m.len(), 1, "a group has exactly one name");
        let items = m.values().next().unwrap();
        assert!(
            items.as_sequence().is_some(),
            "a group holds a list of services, got {:?}",
            items
        );
    }
}

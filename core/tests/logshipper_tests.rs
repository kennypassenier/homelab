//! C1/C2 · the replacement for a log shipper that reached end of life.

use homelab_core::ops::logshipper::{config, install_script, permissions_script, CONFIG_PATH};

fn cfg() -> String {
    config(
        "kyu",
        "109-app-kyu",
        "http://10.10.10.13:3100/loki/api/v1/push",
        &[],
    )
}

/// The labels are the contract. Three Grafana dashboards group by
/// `container_name` and one filters on `job="docker"`; delivering the same
/// lines under different labels leaves every panel empty while the container
/// reports itself perfectly healthy.
#[test]
fn every_label_promtail_set_is_still_set() {
    let c = cfg();
    for label in ["job", "stack", "host", "container_name", "stream"] {
        assert!(c.contains(label), "label {} is missing:\n{}", label, c);
    }
    assert!(
        c.contains("stack    = \"kyu\"") || c.contains("stack = \"kyu\""),
        "{}",
        c
    );
    assert!(c.contains("109-app-kyu"), "{}", c);
}

/// F72, pinned: docker puts the container's name in `attrs.tag`, and reading
/// `attrs.name` instead is what left three dashboards empty for months.
#[test]
fn the_container_name_is_read_from_tag_and_not_from_name() {
    let c = cfg();
    assert!(
        c.contains("container_name = \"tag\""),
        "the field docker actually writes is `tag`:\n{}",
        c
    );
    assert!(
        !c.contains("container_name = \"name\""),
        "reading `name` is F72 all over again"
    );
}

/// The half promtail never had on these containers.
#[test]
fn the_journal_is_read_because_on_a_native_container_that_is_the_log() {
    let c = cfg();
    assert!(c.contains("loki.source.journal"), "{}", c);
    assert!(
        c.contains("systemd-journal"),
        "the journal lines need a job label of their own: {}",
        c
    );
}

#[test]
fn it_points_at_the_loki_it_was_given_and_nowhere_else() {
    let c = config(
        "media",
        "106-app-media",
        "http://10.10.10.13:3100/loki/api/v1/push",
        &[],
    );
    assert!(
        c.contains("http://10.10.10.13:3100/loki/api/v1/push"),
        "{}",
        c
    );
    assert_eq!(
        c.matches("loki.write \"").count(),
        1,
        "exactly one writer: a second endpoint would ship every line twice: {}",
        c
    );
    assert_eq!(
        c.matches("loki.write.default.receiver").count(),
        3,
        "and all three sources — docker, journal, syslog — must reach it, or \
         one of them delivers nothing while the container looks healthy: {}",
        c
    );
}

/// A deploy runs this every time. Installing a package on every run would
/// make the deploy something nobody dares repeat.
#[test]
fn installing_is_idempotent_and_says_so_without_doing_anything() {
    let s = install_script();
    assert!(
        s.contains("command -v alloy") && s.contains("exit 0"),
        "it must return early when alloy is already there: {}",
        s
    );
    assert!(
        s.find("command -v alloy").unwrap() < s.find("apt-get install -y -qq alloy").unwrap(),
        "the check has to come before the install"
    );
}

/// fix-152 (2026-09-28 06:09, the first deploy of inbox on CT 118): the script
/// installed `gpg curl` BEFORE its first `apt-get update`, against package
/// lists from the template build (2025-10-01), so apt asked deb.debian.org
/// for a libssh2 version that had moved on and got 404. Alloy was not
/// installed and the deploy said so in one warning line; the container
/// shipped no logs. The lists are refreshed before the first install.
#[test]
fn fix_152_the_package_lists_are_refreshed_before_the_first_install() {
    let s = install_script();
    let first_install = s.find("apt-get install").expect("an install");
    let first_update = s.find("apt-get update").expect("an update");
    assert!(
        first_update < first_install,
        "apt-get update must run before the first apt-get install: {}",
        s
    );
}

/// The apt route is the point: it is what keeps the shipper patched, which is
/// the whole reason for leaving promtail behind.
#[test]
fn the_package_comes_from_a_signed_repository() {
    let s = install_script();
    assert!(
        s.contains("signed-by=/etc/apt/keyrings/grafana.gpg"),
        "{}",
        s
    );
    assert!(s.contains("gpg --dearmor"), "{}", s);
    assert!(
        !s.contains("--allow-unauthenticated") && !s.contains("[trusted=yes]"),
        "an unverified repository would be worse than the EOL package: {}",
        s
    );
}

/// A shipper that cannot read the files it is pointed at delivers nothing and
/// reports itself healthy — the fault this whole migration exists to escape.
#[test]
fn alloy_is_given_read_access_to_all_three_sources() {
    let p = permissions_script();
    assert!(p.contains("adm"), "syslog: {}", p);
    assert!(p.contains("systemd-journal"), "journald: {}", p);
    assert!(p.contains("docker"), "container logs: {}", p);
    assert!(
        p.contains("getent group docker"),
        "the docker group does not exist on a native container, and failing \
         there would block the very stacks this starts with: {}",
        p
    );
}

#[test]
fn the_config_goes_where_the_packaged_unit_already_looks() {
    assert_eq!(CONFIG_PATH, "/etc/alloy/config.alloy");
}

/// The fault the first live deploy produced: Alloy started, said nothing was
/// wrong, the deploy reported success, and Loki answered 404 to every batch.
mod push_endpoint {
    use homelab_core::ops::logshipper::{config, push_url};

    #[test]
    fn the_base_address_becomes_the_push_endpoint() {
        assert_eq!(
            push_url("http://10.10.10.13:3100"),
            "http://10.10.10.13:3100/loki/api/v1/push",
            "host.toml holds the base address, and pushing there is a 404"
        );
        assert_eq!(
            push_url("http://10.10.10.13:3100/"),
            "http://10.10.10.13:3100/loki/api/v1/push"
        );
    }

    #[test]
    fn an_address_that_already_names_the_path_is_left_alone() {
        let full = "http://10.10.10.13:3100/loki/api/v1/push";
        assert_eq!(push_url(full), full, "never second-guess an explicit one");
    }

    #[test]
    fn the_generated_config_carries_the_push_path_and_not_the_base() {
        let c = config("kyu", "109-app-kyu", "http://10.10.10.13:3100", &[]);
        assert!(
            c.contains("url = \"http://10.10.10.13:3100/loki/api/v1/push\""),
            "{}",
            c
        );
    }
}

/// "The service started" is not "the logs are shipping".
mod delivery_verdict {
    use homelab_core::ops::logshipper::{delivery, Delivery};

    const DROPPING: &str = r#"
# HELP loki_write_sent_bytes_total
loki_write_sent_bytes_total{component_id="loki.write.default"} 0
loki_write_dropped_bytes_total{component_id="loki.write.default"} 48213
"#;
    const SHIPPING: &str = r#"
loki_write_sent_bytes_total{component_id="loki.write.default"} 91240
loki_write_dropped_bytes_total{component_id="loki.write.default"} 0
"#;
    const QUIET: &str = r#"
loki_write_sent_bytes_total{component_id="loki.write.default"} 0
loki_write_dropped_bytes_total{component_id="loki.write.default"} 0
"#;

    /// The exact state the first live deploy was in while reporting success.
    #[test]
    fn dropped_bytes_mean_the_far_end_refused_them() {
        assert_eq!(delivery(DROPPING), Delivery::Dropping { dropped: 48213 });
    }

    #[test]
    fn sent_bytes_with_none_dropped_is_the_only_good_answer() {
        assert_eq!(delivery(SHIPPING), Delivery::Shipping { sent: 91240 });
    }

    /// Nothing sent and nothing dropped is genuinely ambiguous, and saying so
    /// beats picking the flattering reading.
    #[test]
    fn nothing_either_way_is_reported_as_nothing_either_way() {
        assert_eq!(delivery(QUIET), Delivery::Quiet);
    }

    #[test]
    fn no_answer_is_not_a_healthy_answer() {
        assert!(matches!(delivery(""), Delivery::Unknown(_)));
        assert!(matches!(delivery("   \n"), Delivery::Unknown(_)));
    }

    /// Dropping wins over sending: a shipper that delivered something and
    /// then started losing batches is broken, not fine.
    #[test]
    fn dropping_outranks_sending() {
        let both = "loki_write_sent_bytes_total{a=\"1\"} 100\n\
                    loki_write_dropped_bytes_total{a=\"1\"} 5\n";
        assert_eq!(delivery(both), Delivery::Dropping { dropped: 5 });
    }

    #[test]
    fn several_endpoints_are_summed_rather_than_the_first_one_taken() {
        let two = "loki_write_sent_bytes_total{a=\"1\"} 10\n\
                   loki_write_sent_bytes_total{a=\"2\"} 32\n\
                   loki_write_dropped_bytes_total{a=\"1\"} 0\n";
        assert_eq!(delivery(two), Delivery::Shipping { sent: 42 });
    }
}

/// Alloy names a job after the component that produced it, so the journal
/// arrived as `job="loki.source.journal.journal"` on the first live run.
/// The dashboards filter on `job`, so the label has to be forced.
#[test]
fn the_journal_job_label_is_forced_and_not_left_to_alloy() {
    let c = config("kyu", "109-app-kyu", "http://10.10.10.13:3100", &[]);
    let relabel = c
        .split("loki.relabel \"journal\"")
        .nth(1)
        .expect("the journal relabel block must exist");
    assert!(
        relabel.contains("target_label = \"job\"")
            && relabel.contains("replacement  = \"systemd-journal\""),
        "without this the job label is the component's name:\n{}",
        relabel
    );
}

/// gap-11 · a syslog receiver for a device that cannot ship its own logs.
///
/// OPNsense sends RFC 5424 over UDP to the gateway container, and the
/// listener that receives it was hand-added on CT 104 on 2026-09-18 as a
/// second file beside the rendered one. The orchestrator knew nothing about
/// it, so the next deploy of the gateway would have kept remote logging
/// working only by accident — the extra file survived because the deploy
/// never looked at it. The receiver is declared in the stack file now and
/// rendered here, for the stack that declares it and for no other.
mod syslog_receiver {
    use homelab_core::manifest::SyslogReceiver;
    use homelab_core::ops::logshipper::{config, single_file_mode_script};

    fn opnsense() -> SyslogReceiver {
        SyslogReceiver {
            host: "opnsense".into(),
            listen: "0.0.0.0:1514".into(),
            protocol: "udp".into(),
            format: "rfc5424".into(),
            allow_from: vec![],
        }
    }

    /// The labels are the contract again: the vault note and the Grafana
    /// query both read `{job="syslog", host="opnsense"}`.
    #[test]
    fn a_declared_receiver_listens_and_labels_its_lines_like_the_hand_made_one_did() {
        let c = config(
            "gateway",
            "104-app-gateway",
            "http://10.10.10.13:3100",
            &[opnsense()],
        );
        assert!(c.contains("loki.source.syslog"), "{}", c);
        assert!(c.contains("address       = \"0.0.0.0:1514\""), "{}", c);
        assert!(c.contains("protocol      = \"udp\""), "{}", c);
        assert!(c.contains("syslog_format = \"rfc5424\""), "{}", c);
        for label in [
            "job   = \"syslog\"",
            "stack = \"gateway\"",
            "host  = \"opnsense\"",
        ] {
            assert!(c.contains(label), "label {} missing:\n{}", label, c);
        }
        // app / level / facility come out of the RFC 5424 header, and the
        // dashboards filter on them.
        for src in [
            "__syslog_message_app_name",
            "__syslog_message_severity",
            "__syslog_message_facility",
        ] {
            assert!(c.contains(src), "relabel source {} missing:\n{}", src, c);
        }
        assert_eq!(
            c.matches("loki.write.default.receiver").count(),
            4,
            "docker, journal, syslog file AND the receiver must reach the writer:\n{}",
            c
        );
    }

    /// fix-93 (expert panel 2026-09-27, loki-unauthenticated-open): the
    /// receiver took lines from any sender on the LAN, so any container could
    /// write lines labelled as the firewall. With `allow_from` only those
    /// addresses are kept; the rest are dropped before they reach Loki. A
    /// `keep` among the source's own relabel rules does not drop anything
    /// (run against Alloy 1.20.0 the same day: the refused line arrived), so
    /// the filter is a pipeline `loki.relabel` between the source and the
    /// writer, on a `sender` label the source copies and the filter removes.
    /// covers: fix-93
    #[test]
    fn fix_93_a_receiver_keeps_only_the_senders_it_names() {
        let mut r = opnsense();
        r.allow_from = vec!["10.10.10.1".into()];
        let c = config(
            "gateway",
            "104-app-gateway",
            "http://10.10.10.13:3100",
            &[r],
        );
        let copy = "  rule {\n    source_labels = [\"__syslog_connection_ip_address\"]\n    target_label  = \"sender\"\n  }\n";
        assert!(c.contains(copy), "{c}");
        assert!(
            c.contains("  forward_to    = [loki.relabel.syslog_opnsense_sender.receiver]\n"),
            "{c}"
        );
        let filter = "loki.relabel \"syslog_opnsense_sender\" {\n  forward_to = [loki.write.default.receiver]\n  rule {\n    source_labels = [\"sender\"]\n    regex         = \"10\\\\.10\\\\.10\\\\.1\"\n    action        = \"keep\"\n  }\n  rule {\n    regex  = \"sender\"\n    action = \"labeldrop\"\n  }\n}\n";
        assert!(c.contains(filter), "{c}");
        assert_eq!(
            c.matches("loki.write.default.receiver").count(),
            4,
            "the filter, not the source, now reaches the writer:\n{c}"
        );
        // Unset, nothing is filtered, as before.
        let c = config(
            "gateway",
            "104-app-gateway",
            "http://10.10.10.13:3100",
            &[opnsense()],
        );
        assert!(!c.contains("__syslog_connection_ip_address"), "{c}");
        assert!(!c.contains("syslog_opnsense_sender"), "{c}");
    }

    /// Ten other containers render this same file. A receiver that leaked
    /// into all of them would open UDP 1514 on every one and, worse, label
    /// whatever arrived there as OPNsense.
    #[test]
    fn a_stack_that_declares_no_receiver_opens_no_port() {
        let c = config("kyu", "109-app-kyu", "http://10.10.10.13:3100", &[]);
        assert!(!c.contains("loki.source.syslog"), "{}", c);
        assert!(!c.contains("1514"), "{}", c);
    }

    /// Two receivers on one container are two components, and Alloy refuses
    /// a file that names one component twice.
    #[test]
    fn two_receivers_get_two_distinct_component_names() {
        let second = SyslogReceiver {
            host: "omada-controller".into(),
            listen: "0.0.0.0:1515".into(),
            protocol: "udp".into(),
            format: "rfc3164".into(),
            allow_from: vec![],
        };
        let c = config(
            "gateway",
            "104-app-gateway",
            "http://10.10.10.13:3100",
            &[opnsense(), second],
        );
        assert_eq!(c.matches("loki.source.syslog \"").count(), 2, "{}", c);
        assert!(
            c.contains("loki.source.syslog \"syslog_opnsense\""),
            "{}",
            c
        );
        // A hyphen is legal in a host label and not in an Alloy component
        // label, so the name is derived, not copied.
        assert!(
            c.contains("loki.source.syslog \"syslog_omada_controller\""),
            "{}",
            c
        );
        assert!(c.contains("syslog_format = \"rfc3164\""), "{}", c);
    }

    /// The hand-made change on CT 104 switched Alloy to directory mode
    /// (`CONFIG_FILE="/etc/alloy"`), which loads every `*.alloy` in the
    /// directory. That is the mechanism by which a file the orchestrator
    /// never wrote kept being loaded — exactly the drift class this project
    /// keeps finding. The deploy renders ONE file and puts the unit back to
    /// reading one file; anything else in the directory is removed and said
    /// out loud, never silently.
    #[test]
    fn the_deploy_puts_alloy_back_to_reading_the_one_file_it_renders() {
        let s = single_file_mode_script();
        assert!(
            s.contains("CONFIG_FILE=\"/etc/alloy/config.alloy\""),
            "the package default must be restored: {}",
            s
        );
        // Standing rule 20: a scripted edit asserts its target is present
        // before it replaces anything.
        assert!(
            s.find("grep").unwrap() < s.find("sed").unwrap(),
            "the directory-mode line is checked for before it is rewritten: {}",
            s
        );
        assert!(s.contains("restored-single-file-mode"), "{}", s);
        assert!(
            s.contains("removed "),
            "a removed file is named, not hidden: {}",
            s
        );
        assert!(
            s.contains("config.alloy\" ] && continue") || s.contains("config.alloy\" ] || rm"),
            "the rendered file itself must survive the sweep: {}",
            s
        );
    }
}

/// Expert panel 2026-09-27 (container-logs-missing-in-loki): no container
/// log line reached Loki from 2026-09-03 on. Measured on CT 106 the same
/// day: `/var/lib/docker/containers` is `drwx--x--- root root`, and
/// `runuser -u alloy -- ls` on it answers "Permission denied". The docker
/// group opens the socket, not that directory, so the group membership above
/// never gave Alloy the files. It needs CAP_DAC_READ_SEARCH (read and list
/// anything, write nothing), given through a systemd drop-in.
#[test]
fn alloy_gets_read_search_capability_for_the_root_only_docker_directory() {
    let p = permissions_script();
    assert!(
        p.contains(homelab_core::ops::logshipper::READ_DROPIN_PATH),
        "{p}"
    );
    assert!(p.contains("AmbientCapabilities=CAP_DAC_READ_SEARCH"), "{p}");
    assert!(p.contains("systemctl daemon-reload"), "{p}");
    assert!(
        p.contains(homelab_core::ops::logshipper::DROPIN_WRITTEN),
        "the script says when it changed something, so the deploy restarts \
         Alloy on containers whose config did not change: {p}"
    );
}

/// The deploy asks the question that was never asked: can the alloy user
/// actually list the container log directory?
#[test]
fn readability_is_read_from_the_probe_output() {
    use homelab_core::ops::logshipper::{readability, Readability};
    assert_eq!(readability("readable\n"), Readability::Readable);
    assert_eq!(readability("denied\n"), Readability::Denied);
    assert_eq!(readability("no-docker\n"), Readability::NoDocker);
    assert!(matches!(readability(""), Readability::Unknown(_)));
}

/// fix-44 follow-up, measured on the first live deploy (CT 104, 2026-09-27
/// 18:09): the deploy granted the capability and Alloy read eight container
/// logs (`loki_source_file_file_bytes_total` per file, Loki gained the
/// `container_name` label), yet the probe said "cannot read". `runuser -u
/// alloy` starts a fresh session without the service's ambient capability,
/// so it asked about the user, not about the running Alloy. The probe now
/// also reads the capability of the running service.
#[test]
fn the_readability_probe_asks_about_the_running_service_not_a_new_session() {
    let s = homelab_core::ops::logshipper::readability_script();
    assert!(s.contains("systemctl show -p MainPID --value alloy"), "{s}");
    assert!(s.contains("CapAmb"), "{s}");
}

/// Expert panel 2026-09-27 (loki-label-hygiene): `loki.source.file` adds a
/// `filename` label carrying the container's 64-hex id, so every recreate of
/// a container opened new Loki streams for the same app. The dashboards ask
/// by `container_name`; the file path is dropped.
#[test]
fn the_docker_pipeline_drops_the_per_container_filename_label() {
    let c = config("media", "106-app-media", "http://10.10.10.13:3100", &[]);
    let docker = &c[c.find("loki.process \"docker\"").unwrap()..];
    let docker = &docker[..docker.find("forward_to").unwrap()];
    assert!(docker.contains("stage.label_drop"), "{docker}");
    assert!(docker.contains("\"filename\""), "{docker}");
}

/// Expert panel 2026-09-27 (docker-log-timestamps-wrong): the docker pipeline
/// used the moment Alloy read a line as its time. After the read-access fix
/// (fix-44) Alloy read each container's whole log at once, so hours of lines
/// landed in Loki at one instant. Docker writes the real time in `time`.
#[test]
fn container_lines_keep_the_time_docker_wrote() {
    let c = config("media", "106-app-media", "http://10.10.10.13:3100", &[]);
    let docker = &c[c.find("loki.process \"docker\"").unwrap()..];
    let docker = &docker[..docker.find("forward_to").unwrap()];
    assert!(docker.contains("time = \"time\""), "{docker}");
    assert!(docker.contains("stage.timestamp"), "{docker}");
    assert!(docker.contains("RFC3339Nano"), "{docker}");
}

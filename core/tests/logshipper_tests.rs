//! C1/C2 · the replacement for a log shipper that reached end of life.

use homelab_core::ops::logshipper::{config, install_script, permissions_script, CONFIG_PATH};

fn cfg() -> String {
    config(
        "kyu",
        "109-app-kyu",
        "http://10.10.10.4:3100/loki/api/v1/push",
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
        "http://10.10.10.4:3100/loki/api/v1/push",
    );
    assert!(
        c.contains("http://10.10.10.4:3100/loki/api/v1/push"),
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
            push_url("http://10.10.10.4:3100"),
            "http://10.10.10.4:3100/loki/api/v1/push",
            "host.toml holds the base address, and pushing there is a 404"
        );
        assert_eq!(
            push_url("http://10.10.10.4:3100/"),
            "http://10.10.10.4:3100/loki/api/v1/push"
        );
    }

    #[test]
    fn an_address_that_already_names_the_path_is_left_alone() {
        let full = "http://10.10.10.4:3100/loki/api/v1/push";
        assert_eq!(push_url(full), full, "never second-guess an explicit one");
    }

    #[test]
    fn the_generated_config_carries_the_push_path_and_not_the_base() {
        let c = config("kyu", "109-app-kyu", "http://10.10.10.4:3100");
        assert!(
            c.contains("url = \"http://10.10.10.4:3100/loki/api/v1/push\""),
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
    let c = config("kyu", "109-app-kyu", "http://10.10.10.4:3100");
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

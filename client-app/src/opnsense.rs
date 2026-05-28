use anyhow::{Result, anyhow, bail};
use reqwest::blocking::Client;
use serde_json::{Value, json};
use std::collections::HashSet;
use std::env;
use std::net::Ipv4Addr;
use std::str::FromStr;

const MANAGED_BY: &str = "managed-by=homelab";

pub struct ReservationOutcome {
    pub updated: bool,
    pub deleted_conflicts: usize,
    pub reserved_ipv4: String,
}

struct Settings {
    base_url: String,
    api_key: String,
    api_secret: String,
    insecure_tls: bool,
}

#[derive(Clone, Debug)]
struct ReservationRecord {
    uuid: String,
    hostname: String,
    description: String,
    ip_address: String,
    hw_address: String,
}

#[derive(Clone, Debug)]
struct SubnetRecord {
    uuid: String,
    cidr: String,
}

pub fn ensure_stack_reservation(
    config: &crate::scaffold::StackConfig,
    known_stacks: &[String],
) -> Result<ReservationOutcome> {
    let reserved_ipv4 = config
        .reserved_ipv4
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .ok_or_else(|| anyhow!("reserved IPv4 must be set before syncing DHCP"))?
        .to_string();

    let settings = Settings::from_env()?;
    let client = Client::builder()
        .danger_accept_invalid_certs(settings.insecure_tls)
        .build()?;

    let subnets = search_subnets(&client, &settings)?;
    let subnet_uuid = find_subnet_uuid(&subnets, &reserved_ipv4)
        .ok_or_else(|| anyhow!("no OPNsense Kea subnet matches {}", reserved_ipv4))?;

    let reservations = search_reservations(&client, &settings)?;
    let managed_description = managed_description(config);
    let stack_hostnames = known_stack_hostnames(known_stacks);

    let mut exact_match: Option<ReservationRecord> = None;
    let mut deletions = Vec::new();

    for reservation in reservations {
        let matches_ip = reservation.ip_address == reserved_ipv4;
        let matches_mac = reservation.hw_address.eq_ignore_ascii_case(&config.hwaddr);
        let matches_hostname = reservation.hostname == config.hostname;
        let managed_for_stack = is_managed_for_stack(&reservation, config, &stack_hostnames);

        if matches_ip || matches_mac || matches_hostname {
            if managed_for_stack {
                if reservation.ip_address == reserved_ipv4
                    && reservation.hw_address.eq_ignore_ascii_case(&config.hwaddr)
                    && reservation.hostname == config.hostname
                {
                    exact_match = Some(reservation);
                } else {
                    deletions.push(reservation);
                }
            } else {
                bail!(
                    "reservation conflict for {} is not stack-owned (hostname={}, ip={}, mac={})",
                    config.stack_name,
                    reservation.hostname,
                    reservation.ip_address,
                    reservation.hw_address
                );
            }
        }
    }

    for reservation in &deletions {
        post_json(
            &client,
            &settings,
            &format!("/api/kea/dhcpv4/del_reservation/{}", reservation.uuid),
            Value::Object(Default::default()),
        )?;
    }

    let payload = json!({
        "reservation": {
            "subnet": subnet_uuid,
            "ip_address": reserved_ipv4,
            "hw_address": config.hwaddr,
            "hostname": config.hostname,
            "description": managed_description,
            "client_id": Value::Null,
            "next_server": Value::Null,
            "option_data": Value::Null,
            "option": Value::Null
        }
    });

    let updated = if let Some(existing) = exact_match {
        post_json(
            &client,
            &settings,
            &format!("/api/kea/dhcpv4/set_reservation/{}", existing.uuid),
            payload,
        )?;
        true
    } else {
        post_json(
            &client,
            &settings,
            "/api/kea/dhcpv4/add_reservation",
            payload,
        )?;
        false
    };

    let _ = post_json(
        &client,
        &settings,
        "/api/kea/service/reconfigure",
        Value::Object(Default::default()),
    );

    Ok(ReservationOutcome {
        updated,
        deleted_conflicts: deletions.len(),
        reserved_ipv4,
    })
}

fn search_reservations(client: &Client, settings: &Settings) -> Result<Vec<ReservationRecord>> {
    let rows = search_rows(client, settings, "/api/kea/dhcpv4/search_reservation")?;
    Ok(rows
        .into_iter()
        .filter_map(|row| {
            Some(ReservationRecord {
                uuid: row.get("uuid")?.as_str()?.to_string(),
                hostname: row
                    .get("hostname")
                    .and_then(Value::as_str)
                    .unwrap_or_default()
                    .to_string(),
                description: row
                    .get("description")
                    .and_then(Value::as_str)
                    .unwrap_or_default()
                    .to_string(),
                ip_address: row
                    .get("ip_address")
                    .and_then(Value::as_str)
                    .unwrap_or_default()
                    .to_string(),
                hw_address: row
                    .get("hw_address")
                    .and_then(Value::as_str)
                    .unwrap_or_default()
                    .to_string(),
            })
        })
        .collect())
}

fn search_subnets(client: &Client, settings: &Settings) -> Result<Vec<SubnetRecord>> {
    let rows = search_rows(client, settings, "/api/kea/dhcpv4/search_subnet")?;
    Ok(rows
        .into_iter()
        .filter_map(|row| {
            Some(SubnetRecord {
                uuid: row.get("uuid")?.as_str()?.to_string(),
                cidr: row.get("subnet")?.as_str()?.to_string(),
            })
        })
        .collect())
}

fn search_rows(client: &Client, settings: &Settings, path: &str) -> Result<Vec<Value>> {
    let response = post_json(
        client,
        settings,
        path,
        json!({
            "current": 1,
            "rowCount": -1,
            "searchPhrase": ""
        }),
    )?;

    Ok(response
        .get("rows")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default())
}

fn post_json(client: &Client, settings: &Settings, path: &str, body: Value) -> Result<Value> {
    let url = format!("{}{}", settings.base_url, path);
    let response = client
        .post(&url)
        .basic_auth(&settings.api_key, Some(&settings.api_secret))
        .json(&body)
        .send()?;

    let status = response.status();
    let text = response.text()?;
    if !status.is_success() {
        bail!("{} returned {}: {}", path, status, text);
    }

    let value: Value = serde_json::from_str(&text).unwrap_or_else(|_| json!({ "raw": text }));
    if let Some(result) = value.get("result").and_then(Value::as_str) {
        if result == "failed" {
            bail!(
                "{} failed: {}",
                path,
                value
                    .get("validations")
                    .cloned()
                    .unwrap_or_else(|| value.clone())
            );
        }
    }
    Ok(value)
}

fn find_subnet_uuid(subnets: &[SubnetRecord], ip_address: &str) -> Option<String> {
    let ip = Ipv4Addr::from_str(ip_address).ok()?;
    subnets
        .iter()
        .find(|subnet| ip_in_cidr(ip, &subnet.cidr))
        .map(|subnet| subnet.uuid.clone())
}

fn ip_in_cidr(ip: Ipv4Addr, cidr: &str) -> bool {
    let Some((base, prefix)) = cidr.split_once('/') else {
        return false;
    };
    let Ok(base_ip) = Ipv4Addr::from_str(base) else {
        return false;
    };
    let Ok(prefix) = prefix.parse::<u32>() else {
        return false;
    };
    if prefix > 32 {
        return false;
    }

    let mask = if prefix == 0 {
        0
    } else {
        u32::MAX << (32 - prefix)
    };

    (u32::from(ip) & mask) == (u32::from(base_ip) & mask)
}

fn managed_description(config: &crate::scaffold::StackConfig) -> String {
    format!(
        "{} stack={} hostname={} vmid={}",
        MANAGED_BY, config.stack_name, config.hostname, config.vmid
    )
}

fn known_stack_hostnames(known_stacks: &[String]) -> HashSet<String> {
    known_stacks
        .iter()
        .map(|stack| format!("lxc-{}", stack))
        .collect()
}

fn is_managed_for_stack(
    reservation: &ReservationRecord,
    config: &crate::scaffold::StackConfig,
    known_stack_hostnames: &HashSet<String>,
) -> bool {
    reservation.description.contains(MANAGED_BY)
        || reservation.description.contains(&format!("stack={}", config.stack_name))
        || reservation.hostname == config.hostname
        || reservation.hw_address.eq_ignore_ascii_case(&config.hwaddr)
        || known_stack_hostnames.contains(&reservation.hostname)
}

impl Settings {
    fn from_env() -> Result<Self> {
        let base_url = env::var("OPNSENSE_BASE_URL")
            .map_err(|_| anyhow!("OPNSENSE_BASE_URL is not set"))?
            .trim_end_matches('/')
            .to_string();
        let api_key = env::var("OPNSENSE_API_KEY")
            .map_err(|_| anyhow!("OPNSENSE_API_KEY is not set"))?;
        let api_secret = env::var("OPNSENSE_API_SECRET")
            .map_err(|_| anyhow!("OPNSENSE_API_SECRET is not set"))?;
        let insecure_tls = env::var("OPNSENSE_TLS_INSECURE")
            .map(|value| matches!(value.as_str(), "1" | "true" | "TRUE" | "yes" | "YES"))
            .unwrap_or(false);

        Ok(Self {
            base_url,
            api_key,
            api_secret,
            insecure_tls,
        })
    }
}
//! Edit forms: the browser's field and rule checks, in the browser's words.

use super::*;

pub(super) fn text_of(v: Option<&Value>) -> String {
    match v {
        Some(Value::String(s)) => s.clone(),
        Some(Value::Null) | None => String::new(),
        Some(Value::Bool(b)) => b.to_string(),
        Some(other) => other.to_string(),
    }
}

pub(super) fn all_digits(t: &str) -> bool {
    !t.is_empty() && t.chars().all(|c| c.is_ascii_digit())
}

pub(super) fn matches(pattern: &str, text: &str) -> bool {
    regex::Regex::new(&format!("^(?:{pattern})$"))
        .map(|r| r.is_match(text))
        .unwrap_or(false)
}

/// `checkFields`: what is wrong with the values, by field name.
pub fn check_fields<'a>(
    fields: impl IntoIterator<Item = &'a Field>,
    values: &Values,
) -> BTreeMap<String, String> {
    let mut errors = BTreeMap::new();
    for f in fields {
        if f.kind == FieldKind::Check {
            continue;
        }
        let text = match values.get(&f.name) {
            Some(Value::String(s)) => s.trim().to_string(),
            _ => String::new(),
        };
        if text.is_empty() {
            if f.required {
                errors.insert(f.name.clone(), say("needed", &[("label", &f.label)]));
            }
            continue;
        }
        if f.kind == FieldKind::Number {
            let (min, max) = (f.min.unwrap_or(0), f.max.unwrap_or(i64::MAX));
            let ok =
                all_digits(&text) && text.parse::<i64>().is_ok_and(|n| (min..=max).contains(&n));
            if !ok {
                let (lo, hi) = (min.to_string(), max.to_string());
                errors.insert(
                    f.name.clone(),
                    say("number", &[("label", &f.label), ("min", &lo), ("max", &hi)]),
                );
            }
        } else if f.kind == FieldKind::Typed && Some(&text) != f.expect.as_ref() {
            errors.insert(
                f.name.clone(),
                say("typed", &[("expect", f.expect.as_deref().unwrap_or(""))]),
            );
        } else if f.pattern.as_deref().is_some_and(|p| !matches(p, &text)) {
            let lower = f.label.to_lowercase();
            errors.insert(
                f.name.clone(),
                say("pattern", &[("text", &text), ("lower", &lower)]),
            );
        }
    }
    errors
}

/// `ruleProblems`: what Proxmox would reject or misread, by field.
pub fn rule_problems(v: &Values) -> BTreeMap<String, String> {
    let mut out = BTreeMap::new();
    let peer = text_of(v.get("peer")).trim().to_string();
    if !peer.is_empty() {
        let re = regex::Regex::new(r"^(\d{1,3})\.(\d{1,3})\.(\d{1,3})\.(\d{1,3})(?:/(\d{1,2}))?$")
            .expect("the address pattern reads");
        match re.captures(&peer) {
            Some(c) if (1..=4).all(|i| c[i].parse::<u32>().unwrap_or(999) <= 255) => {
                if let Some(bits) = c.get(5) {
                    let bits: u32 = bits.as_str().parse().unwrap_or(99);
                    let o = |i: usize| c[i].parse::<u32>().unwrap_or(0);
                    let ip = (o(1) << 24) | (o(2) << 16) | (o(3) << 8) | o(4);
                    if bits > 32 {
                        out.insert("peer".into(), say("peer_prefix", &[("peer", &peer)]));
                    } else {
                        let mask = if bits == 0 {
                            0
                        } else {
                            u32::MAX << (32 - bits)
                        };
                        if ip & mask != ip {
                            out.insert("peer".into(), say("peer_host_bits", &[("peer", &peer)]));
                        }
                    }
                }
            }
            _ => {
                out.insert("peer".into(), say("peer_form", &[("peer", &peer)]));
            }
        }
    }
    let ports: String = text_of(v.get("dport"))
        .chars()
        .filter(|c| !c.is_whitespace())
        .collect();
    let proto = text_of(v.get("proto"));
    if !ports.is_empty() {
        if proto.is_empty() || proto == "icmp" {
            let key = if proto == "icmp" {
                "icmp_ports"
            } else {
                "port_proto"
            };
            out.insert("dport".into(), say(key, &[]));
        } else {
            let ok = |n: Option<f64>| {
                n.is_some_and(|n| n.fract() == 0.0 && (1.0..=65535.0).contains(&n))
            };
            let num = |s: Option<&str>| -> Option<f64> {
                let s = s?;
                if s.is_empty() {
                    Some(0.0)
                } else {
                    s.parse::<f64>().ok().filter(|n| n.is_finite())
                }
            };
            for part in ports.split(',') {
                let mut it = part.split(':');
                let a = num(it.next());
                let b = num(it.next());
                let bad = !ok(a) || (part.contains(':') && (!ok(b) || b <= a));
                if bad {
                    out.insert("dport".into(), say("port_range", &[("part", part)]));
                    break;
                }
            }
        }
    }
    if text_of(v.get("note")).contains('\n') {
        out.insert("note".into(), say("note_line", &[]));
    }
    out
}

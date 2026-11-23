use std::net::{IpAddr, Ipv4Addr, Ipv6Addr};

pub(super) fn is_external_identity(value: &str) -> bool {
    let trimmed = value.trim();
    trimmed == value
        && trimmed.len() >= 3
        && !contains_placeholder_token(trimmed)
        && !contains_local_only_token(trimmed)
        && !contains_first_party_attestation_token(trimmed)
}

pub(super) fn is_public_network(value: &str) -> bool {
    let trimmed = value.trim();
    if trimmed != value || trimmed.is_empty() || contains_placeholder_token(trimmed) {
        return false;
    }
    let lower = trimmed.to_ascii_lowercase();
    if lower == "testnet-or-mainnet"
        || ["local", "devnet", "regtest", "simnet", "private", "fake"].iter().any(|token| lower.contains(token))
    {
        return false;
    }
    lower == "mainnet"
        || lower == "testnet"
        || lower.ends_with("-mainnet")
        || lower.ends_with("-testnet")
        || lower.ends_with(" mainnet")
        || lower.ends_with(" testnet")
}

pub(super) fn contains_placeholder_token(value: &str) -> bool {
    let lower = value.to_ascii_lowercase();
    [
        "replace_with",
        "replace-",
        "placeholder",
        "todo",
        "tbd",
        "unknown",
        "example",
        "sample",
        "dummy",
        "not_applicable",
        "not-applicable",
        "n/a",
    ]
    .iter()
    .any(|token| lower.contains(token))
}

pub(super) fn contains_local_only_token(value: &str) -> bool {
    let lower = value.to_ascii_lowercase();
    ["local", "devnet", "regtest", "simnet", "fake", "internal", "mock"].iter().any(|token| lower.contains(token))
}

fn contains_first_party_attestation_token(value: &str) -> bool {
    let lower = value.to_ascii_lowercase();
    [
        "novaseal",
        "nova seal",
        "cellscript",
        "cell script",
        "a19q3",
        "first-party",
        "first_party",
        "self-attested",
        "self_attested",
        "self attested",
        "self-attestation",
        "self_attestation",
        "self attestation",
    ]
    .iter()
    .any(|token| lower.contains(token))
}

pub(super) fn is_https_report_uri(value: &str) -> bool {
    if value != value.trim() || contains_placeholder_token(value) || value.bytes().any(|byte| byte.is_ascii_whitespace()) {
        return false;
    }
    let Some(rest) = value.strip_prefix("https://") else {
        return false;
    };
    let authority = rest.split(['/', '?', '#']).next().unwrap_or_default();
    let Some(host) = report_uri_host(authority) else {
        return false;
    };
    is_public_report_host(host)
}

fn report_uri_host(authority: &str) -> Option<&str> {
    if authority.is_empty() || authority.contains('@') {
        return None;
    }
    if let Some(rest) = authority.strip_prefix('[') {
        let (host, suffix) = rest.split_once(']')?;
        if !valid_optional_port_suffix(suffix) {
            return None;
        }
        return Some(host);
    }
    if authority.matches(':').count() > 1 {
        return None;
    }
    match authority.split_once(':') {
        Some((host, port)) if valid_port(port) && !host.is_empty() => Some(host),
        Some(_) => None,
        None => Some(authority),
    }
}

fn valid_optional_port_suffix(suffix: &str) -> bool {
    if suffix.is_empty() {
        return true;
    }
    suffix.strip_prefix(':').is_some_and(valid_port)
}

fn valid_port(port: &str) -> bool {
    port.parse::<u16>().is_ok_and(|value| value != 0)
}

fn is_public_report_host(host: &str) -> bool {
    if host != host.trim() || host.is_empty() || contains_placeholder_token(host) {
        return false;
    }
    if let Ok(ip) = host.parse::<IpAddr>() {
        return is_public_report_ip(ip);
    }
    let lower = host.to_ascii_lowercase();
    if lower == "localhost"
        || lower.ends_with(".localhost")
        || lower.ends_with(".invalid")
        || lower.ends_with(".local")
        || lower.ends_with(".test")
        || lower.starts_with('.')
        || lower.ends_with('.')
        || !lower.contains('.')
    {
        return false;
    }
    let mut has_alpha = false;
    for label in lower.split('.') {
        if label.is_empty()
            || label.len() > 63
            || label.starts_with('-')
            || label.ends_with('-')
            || !label.bytes().all(|byte| byte.is_ascii_alphanumeric() || byte == b'-')
        {
            return false;
        }
        has_alpha |= label.bytes().any(|byte| byte.is_ascii_alphabetic());
    }
    has_alpha
}

fn is_public_report_ip(ip: IpAddr) -> bool {
    match ip {
        IpAddr::V4(value) => is_public_report_ipv4(value),
        IpAddr::V6(value) => is_public_report_ipv6(value),
    }
}

fn is_public_report_ipv4(ip: Ipv4Addr) -> bool {
    let [a, b, c, _d] = ip.octets();
    if a == 0
        || a == 10
        || a == 127
        || a >= 224
        || (a == 100 && (64..=127).contains(&b))
        || (a == 169 && b == 254)
        || (a == 172 && (16..=31).contains(&b))
        || (a == 192 && b == 0 && c == 0)
        || (a == 192 && b == 0 && c == 2)
        || (a == 192 && b == 168)
        || (a == 198 && (18..=19).contains(&b))
        || (a == 198 && b == 51 && c == 100)
        || (a == 203 && b == 0 && c == 113)
    {
        return false;
    }
    true
}

fn is_public_report_ipv6(ip: Ipv6Addr) -> bool {
    let segments = ip.segments();
    if ip.is_unspecified()
        || ip.is_loopback()
        || ip.is_multicast()
        || (segments[0] & 0xfe00) == 0xfc00
        || (segments[0] & 0xffc0) == 0xfe80
        || (segments[0] == 0x2001 && segments[1] == 0x0db8)
    {
        return false;
    }
    true
}

use super::super::output::print_json;
use super::registry_keys::{
    generate_registry_key_material, registry_capability_key_id, remove_registry_private_key, store_pending_registry_private_key,
    store_registry_private_key,
};
use super::{parse_registry_api_url, utc_timestamp_after_seconds, GeneratedRegistryKeyMaterial};
use crate::error::Result;
use colored::Colorize;
use std::time::Duration;

pub(super) fn submit_registry_publish_request(
    endpoint: &str,
    request: &crate::package::registry::RegistryPublishRequest,
    idempotency_key: &str,
    json_output: bool,
) -> Result<()> {
    let client = registry_http_client()?;
    let response = submit_registry_publish_request_with_retry(&client, endpoint, request, idempotency_key)?;
    let status = response.status();
    let body = response.text().map_err(|error| {
        crate::error::CompileError::without_span(format!("failed to read registry publish response from '{}': {}", endpoint, error))
            .with_category(crate::error::CompileErrorCategory::Network)
            .with_source(error)
    })?;
    if !status.is_success() {
        return Err(crate::error::CompileError::without_span(format!(
            "registry publish request failed with HTTP {}: {}",
            status,
            body.trim()
        ))
        .with_category(registry_http_error_category(status)));
    }

    let parsed = serde_json::from_str::<serde_json::Value>(&body).unwrap_or_else(|_| serde_json::json!({ "response": body }));
    if json_output {
        print_json(&parsed)?;
    } else {
        println!("{}", "Published to registry write API".green());
        if let Some(status) = parsed.get("status").and_then(serde_json::Value::as_str) {
            println!("  Status: {}", status);
        }
        if let Some(direct_url) = parsed.get("direct_url").and_then(serde_json::Value::as_str) {
            println!("  Direct URL: {}", direct_url);
        }
        if let Some(request_id) = parsed.get("request_id").and_then(serde_json::Value::as_str) {
            println!("  Request id: {}", request_id);
        }
    }
    Ok(())
}

pub(in crate::cli) fn registry_http_client() -> Result<reqwest::blocking::Client> {
    reqwest::blocking::Client::builder().timeout(Duration::from_secs(30)).redirect(reqwest::redirect::Policy::none()).build().map_err(
        |error| {
            crate::error::CompileError::without_span(format!("failed to build registry HTTP client: {}", error))
                .with_category(crate::error::CompileErrorCategory::Network)
                .with_source(error)
        },
    )
}

#[derive(serde::Serialize)]
struct RegistryAuthorisationSessionCreateRequest {
    capability_pubkey: String,
    requested_scopes: Vec<String>,
    artifact_kind: String,
    capability_expires_at: String,
    cli_version: String,
}

#[derive(serde::Deserialize)]
struct RegistryAuthorisationSessionCreateResponse {
    session_id: String,
    poll_token: String,
    browser_url: String,
    expires_at: String,
}

#[derive(serde::Deserialize)]
struct RegistryAuthorisationSessionPollResponse {
    status: String,
    capability_key_id: Option<String>,
    namespace_status: Option<String>,
}

enum RegistryAuthorisationSessionPollOutcome {
    Expired,
    State(RegistryAuthorisationSessionPollResponse),
}

fn fetch_registry_authorisation_session(
    client: &reqwest::blocking::Client,
    endpoint: &str,
    poll_token: &str,
) -> Result<RegistryAuthorisationSessionPollOutcome> {
    let response = client.get(endpoint).bearer_auth(poll_token).send().map_err(|error| {
        crate::error::CompileError::without_span(format!("failed to poll browser authorisation session: {error}"))
            .with_category(crate::error::CompileErrorCategory::Network)
            .with_source(error)
    })?;
    let status = response.status();
    let body = response.text().map_err(|error| {
        crate::error::CompileError::without_span(format!("failed to read authorisation session status: {error}"))
            .with_category(crate::error::CompileErrorCategory::Network)
            .with_source(error)
    })?;
    if status == reqwest::StatusCode::GONE {
        return Ok(RegistryAuthorisationSessionPollOutcome::Expired);
    }
    if !status.is_success() {
        return Err(crate::error::CompileError::without_span(format!(
            "browser authorisation session failed with HTTP {status}: {}",
            body.trim()
        ))
        .with_category(registry_http_error_category(status)));
    }
    let poll = serde_json::from_str::<RegistryAuthorisationSessionPollResponse>(&body).map_err(|error| {
        crate::error::CompileError::without_span(format!("registry returned an invalid authorisation status: {error}"))
            .with_category(crate::error::CompileErrorCategory::Network)
            .with_source(error)
    })?;
    Ok(RegistryAuthorisationSessionPollOutcome::State(poll))
}

pub(super) fn registry_authorisation_status_removes_pending_key(status: &str) -> bool {
    matches!(status, "cancelled" | "expired")
}

fn resolve_registry_authorisation_session_poll(
    poll: RegistryAuthorisationSessionPollResponse,
    generated: &GeneratedRegistryKeyMaterial,
    namespace: &str,
) -> Result<Option<String>> {
    activate_registry_key_after_wallet_approval(&poll.status, poll.capability_key_id.as_deref(), &generated.key_id, || {
        store_registry_private_key(&generated.key_id, &generated.private_key_pkcs8)
    })?;
    match poll.status.as_str() {
        "pending" => Ok(None),
        "authorised" => {
            let key_id = poll
                .capability_key_id
                .ok_or_else(|| crate::error::CompileError::without_span("authorised session did not return a capability key"))?;
            eprintln!("Publishing authorisation confirmed; continuing with cellc.");
            Ok(Some(key_id))
        }
        "review_pending" => {
            let key_id = poll.capability_key_id.as_deref().ok_or_else(|| {
                crate::error::CompileError::without_span("review_pending session did not return a capability key")
                    .with_category(crate::error::CompileErrorCategory::Authentication)
            })?;
            Err(crate::error::CompileError::without_span(format!(
                "wallet authorisation succeeded, but namespace '{namespace}' is awaiting Registry review; rerun publish with --capability-key-id {key_id} after approval"
            ))
            .with_category(crate::error::CompileErrorCategory::Authentication))
        }
        status if registry_authorisation_status_removes_pending_key(status) => {
            remove_registry_private_key(&generated.key_id)?;
            Err(crate::error::CompileError::without_span(format!(
                "browser authorisation session was {status}; run `cellc publish --authorise` again"
            ))
            .with_category(crate::error::CompileErrorCategory::Authentication))
        }
        other => Err(crate::error::CompileError::without_span(format!(
            "registry returned unknown authorisation session status '{other}' (namespace status: {})",
            poll.namespace_status.as_deref().unwrap_or("unknown")
        ))
        .with_category(crate::error::CompileErrorCategory::Network)),
    }
}

pub(super) fn activate_registry_key_after_wallet_approval<F>(
    status: &str,
    returned_key_id: Option<&str>,
    generated_key_id: &str,
    persist: F,
) -> Result<()>
where
    F: FnOnce() -> Result<()>,
{
    if !matches!(status, "authorised" | "review_pending") {
        return Ok(());
    }
    let Some(returned_key_id) = returned_key_id else {
        return Err(crate::error::CompileError::without_span(format!("{status} session did not return a publishing key"))
            .with_category(crate::error::CompileErrorCategory::Authentication));
    };
    if returned_key_id != generated_key_id {
        return Err(crate::error::CompileError::without_span(
            "registry approved a different publishing key than the one held locally",
        )
        .with_category(crate::error::CompileErrorCategory::Authentication));
    }
    persist()
}

pub(super) fn authorise_registry_publish_key(
    api_base: &str,
    namespace: &str,
    name: &str,
    artifact_kind: &str,
    no_open: bool,
) -> Result<String> {
    let generated = generate_registry_key_material()?;
    let client = registry_http_client()?;
    let endpoint = format!("{}/v1/authorisation-sessions", api_base.trim_end_matches('/'));
    let response = client
        .post(&endpoint)
        .json(&RegistryAuthorisationSessionCreateRequest {
            capability_pubkey: generated.public_key.clone(),
            requested_scopes: vec![format!("publish:{namespace}/{name}")],
            artifact_kind: artifact_kind.to_string(),
            capability_expires_at: utc_timestamp_after_seconds(90 * 24 * 60 * 60),
            cli_version: crate::VERSION.to_string(),
        })
        .send()
        .map_err(|error| {
            crate::error::CompileError::without_span(format!("failed to create browser authorisation session: {error}"))
                .with_category(crate::error::CompileErrorCategory::Network)
                .with_source(error)
        })?;
    let status = response.status();
    let body = response.text().map_err(|error| {
        crate::error::CompileError::without_span(format!("failed to read browser authorisation session response: {error}"))
            .with_category(crate::error::CompileErrorCategory::Network)
            .with_source(error)
    })?;
    if !status.is_success() {
        return Err(crate::error::CompileError::without_span(format!(
            "registry refused browser authorisation session with HTTP {status}: {}",
            body.trim()
        ))
        .with_category(registry_http_error_category(status)));
    }
    let session: RegistryAuthorisationSessionCreateResponse = serde_json::from_str(&body).map_err(|error| {
        crate::error::CompileError::without_span(format!("registry returned an invalid authorisation session: {error}"))
            .with_category(crate::error::CompileErrorCategory::Network)
            .with_source(error)
    })?;
    validate_registry_authorisation_session(&session, api_base)?;
    if generated.key_id != registry_capability_key_id(&generated.public_key) {
        return Err(crate::error::CompileError::without_span(
            "generated capability key identity changed before browser authorisation",
        )
        .with_category(crate::error::CompileErrorCategory::Authentication));
    }
    store_pending_registry_private_key(&generated.key_id, &generated.private_key_pkcs8, &session.session_id, &session.expires_at)?;
    eprintln!("Authorise publishing {namespace}/{name} in your CKB wallet:");
    eprintln!("  {}", session.browser_url);
    eprintln!("Pending publishing key: {}", generated.key_id);
    if !no_open && let Err(error) = open_registry_authorisation_url(&session.browser_url) {
        eprintln!("Browser did not open automatically: {error}");
    }
    eprintln!("Waiting for wallet approval…");

    let poll_endpoint = format!("{}/v1/authorisation-sessions/{}", api_base.trim_end_matches('/'), session.session_id);
    let deadline = std::time::Instant::now() + Duration::from_secs(15 * 60);
    while std::time::Instant::now() < deadline {
        match fetch_registry_authorisation_session(&client, &poll_endpoint, &session.poll_token)? {
            RegistryAuthorisationSessionPollOutcome::Expired => {
                remove_registry_private_key(&generated.key_id)?;
                return Err(crate::error::CompileError::without_span(
                    "browser authorisation session expired before wallet approval; run `cellc publish --authorise` again",
                )
                .with_category(crate::error::CompileErrorCategory::Authentication));
            }
            RegistryAuthorisationSessionPollOutcome::State(poll) => {
                if let Some(key_id) = resolve_registry_authorisation_session_poll(poll, &generated, namespace)? {
                    return Ok(key_id);
                }
                std::thread::sleep(Duration::from_secs(2));
            }
        }
    }

    // The server may have committed wallet approval immediately before the local
    // deadline. One final authoritative read closes that race. A still-pending or
    // unreachable session keeps its pending key so a later CLI invocation can recover it.
    match fetch_registry_authorisation_session(&client, &poll_endpoint, &session.poll_token)? {
        RegistryAuthorisationSessionPollOutcome::Expired => {
            remove_registry_private_key(&generated.key_id)?;
            Err(crate::error::CompileError::without_span(
                "browser authorisation session expired before wallet approval; run `cellc publish --authorise` again",
            )
            .with_category(crate::error::CompileErrorCategory::Authentication))
        }
        RegistryAuthorisationSessionPollOutcome::State(poll) => {
            if let Some(key_id) = resolve_registry_authorisation_session_poll(poll, &generated, namespace)? {
                return Ok(key_id);
            }
            Err(crate::error::CompileError::without_span(format!(
                "browser authorisation is still pending; publishing key {} remains in the OS keychain for recovery",
                generated.key_id
            ))
            .with_category(crate::error::CompileErrorCategory::Authentication))
        }
    }
}

fn validate_registry_authorisation_session(session: &RegistryAuthorisationSessionCreateResponse, api_base: &str) -> Result<()> {
    if !session.session_id.starts_with("auth_")
        || session.session_id.len() != 37
        || !session.session_id[5..].bytes().all(|byte| byte.is_ascii_hexdigit())
        || !session.poll_token.starts_with("poll_")
        || session.poll_token.len() != 37
        || !session.poll_token[5..].bytes().all(|byte| byte.is_ascii_hexdigit())
    {
        return Err(crate::error::CompileError::without_span("registry returned malformed authorisation credentials")
            .with_category(crate::error::CompileErrorCategory::Network));
    }
    let browser = reqwest::Url::parse(&session.browser_url).map_err(|error| {
        crate::error::CompileError::without_span(format!("registry returned an invalid browser URL: {error}"))
            .with_category(crate::error::CompileErrorCategory::Network)
    })?;
    let api = parse_registry_api_url(api_base)?;
    let browser_host = browser.host_str().unwrap_or_default();
    let fragment = browser.fragment().unwrap_or_default();
    let fragment_value = |name: &str| {
        fragment.split('&').find_map(|part| {
            let (key, value) = part.split_once('=')?;
            (key == name).then_some(value)
        })
    };
    let browser_session_id = fragment_value("authorisation_session");
    let browser_token = fragment_value("browser_token").unwrap_or_default();
    let loopback = browser_host.parse::<std::net::IpAddr>().is_ok_and(|address| address.is_loopback())
        || browser_host.eq_ignore_ascii_case("localhost");
    if (browser.scheme() != "https" && !(browser.scheme() == "http" && loopback))
        || !browser.username().is_empty()
        || browser.password().is_some()
        || browser.query().is_some()
        || browser_session_id != Some(session.session_id.as_str())
        || !browser_token.starts_with("browser_")
        || browser_token.len() != 40
        || !browser_token[8..].bytes().all(|byte| byte.is_ascii_hexdigit())
        || !browser.path().ends_with("/registry/submit")
        || (api.scheme() == "https" && browser.scheme() != "https")
    {
        return Err(crate::error::CompileError::without_span("registry returned an unsafe browser authorisation URL")
            .with_category(crate::error::CompileErrorCategory::Network));
    }
    Ok(())
}

fn open_registry_authorisation_url(url: &str) -> std::io::Result<()> {
    #[cfg(target_os = "macos")]
    let status = std::process::Command::new("open").arg(url).status()?;
    #[cfg(target_os = "windows")]
    let status = std::process::Command::new("rundll32").args(["url.dll,FileProtocolHandler", url]).status()?;
    #[cfg(all(unix, not(target_os = "macos")))]
    let status = std::process::Command::new("xdg-open").arg(url).status()?;
    #[cfg(not(any(unix, windows)))]
    return Err(std::io::Error::other("automatic browser opening is unsupported on this platform"));
    if status.success() {
        Ok(())
    } else {
        Err(std::io::Error::other("browser launcher returned a failure status"))
    }
}

fn submit_registry_publish_request_with_retry(
    client: &reqwest::blocking::Client,
    endpoint: &str,
    request: &crate::package::registry::RegistryPublishRequest,
    idempotency_key: &str,
) -> Result<reqwest::blocking::Response> {
    let mut transport_error = None;
    for attempt in 0..2 {
        let response = client.post(endpoint).header("Idempotency-Key", idempotency_key).json(request).send();
        match response {
            Ok(response) => {
                if attempt == 0 && is_retryable_registry_status(response.status()) {
                    std::thread::sleep(Duration::from_millis(500));
                    continue;
                }
                return Ok(response);
            }
            Err(error) if attempt == 0 => {
                transport_error = Some(error);
                std::thread::sleep(Duration::from_millis(500));
            }
            Err(error) => {
                return Err(crate::error::CompileError::without_span(format!(
                    "failed to submit registry publish request to '{}': {}",
                    endpoint, error
                ))
                .with_category(crate::error::CompileErrorCategory::Network)
                .with_source(error));
            }
        }
    }
    let message = transport_error
        .map(|error| error.to_string())
        .unwrap_or_else(|| "registry publish request did not produce a response".to_string());
    Err(crate::error::CompileError::without_span(format!("failed to submit registry publish request to '{}': {}", endpoint, message))
        .with_category(crate::error::CompileErrorCategory::Network))
}

fn is_retryable_registry_status(status: reqwest::StatusCode) -> bool {
    matches!(
        status,
        reqwest::StatusCode::REQUEST_TIMEOUT
            | reqwest::StatusCode::BAD_GATEWAY
            | reqwest::StatusCode::SERVICE_UNAVAILABLE
            | reqwest::StatusCode::GATEWAY_TIMEOUT
            | reqwest::StatusCode::INTERNAL_SERVER_ERROR
    )
}

fn registry_http_error_category(status: reqwest::StatusCode) -> crate::error::CompileErrorCategory {
    if matches!(status, reqwest::StatusCode::UNAUTHORIZED | reqwest::StatusCode::FORBIDDEN) {
        crate::error::CompileErrorCategory::Authentication
    } else {
        crate::error::CompileErrorCategory::Network
    }
}

pub(super) fn submit_registry_json_request(
    endpoint: &str,
    body: &serde_json::Value,
    success_label: &str,
    json_output: bool,
) -> Result<serde_json::Value> {
    let client = registry_http_client()?;
    let response = client.post(endpoint).json(body).send().map_err(|error| {
        crate::error::CompileError::without_span(format!("failed to submit registry request to '{}': {}", endpoint, error))
            .with_category(crate::error::CompileErrorCategory::Network)
            .with_source(error)
    })?;
    let status = response.status();
    let response_body = response.text().map_err(|error| {
        crate::error::CompileError::without_span(format!("failed to read registry response from '{}': {}", endpoint, error))
            .with_category(crate::error::CompileErrorCategory::Network)
            .with_source(error)
    })?;
    if !status.is_success() {
        return Err(crate::error::CompileError::without_span(format!(
            "registry request failed with HTTP {}: {}",
            status,
            response_body.trim()
        ))
        .with_category(registry_http_error_category(status)));
    }
    let parsed =
        serde_json::from_str::<serde_json::Value>(&response_body).unwrap_or_else(|_| serde_json::json!({ "response": response_body }));
    if json_output {
        print_json(&parsed)?;
    } else {
        println!("{}", success_label.green());
        if let Some(request_id) = parsed.get("request_id").and_then(serde_json::Value::as_str) {
            println!("  Request id: {}", request_id);
        }
    }
    Ok(parsed)
}

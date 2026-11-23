use super::super::output::{print_json, read_json_value, CommandOutcome};
use super::registry_keys::{
    generate_and_store_registry_capability_key, generate_registry_key_material, registry_capability_key_id,
    store_registry_private_key, write_new_reproducer_private_key,
};
use super::registry_requests::submit_registry_json_request;
use super::{
    current_utc_timestamp, read_capability_authorisation_payload, read_capability_revocation_payload, registry_auth_nonce,
    registry_origin_from_api_base, registry_revoke_nonce, resolve_capability_expires_at, resolve_registry_api_base,
    resolve_requested_scopes, utc_timestamp_after_seconds, AuthCapabilityArgs, AuthCapabilityRevokeArgs, AuthCapabilitySubmitArgs,
    AuthNamespaceClaimArgs, AuthReproducerCreateArgs, CommandExecutor, LoginArgs,
};
use crate::error::Result;
use colored::Colorize;

impl CommandExecutor {
    pub(super) fn login(args: LoginArgs) -> Result<()> {
        Self::auth_capability(AuthCapabilityArgs { registry_origin: args.registry, ..Default::default() })
    }

    pub(super) fn auth_capability(args: AuthCapabilityArgs) -> Result<()> {
        let registry_origin = args
            .registry_origin
            .or_else(|| std::env::var("CELLSCRIPT_REGISTRY_ORIGIN").ok())
            .unwrap_or_else(|| crate::package::registry::DEFAULT_PUBLIC_REGISTRY_ORIGIN.to_string());
        let principal_type =
            args.principal_type.or_else(|| std::env::var("CELLSCRIPT_PRINCIPAL_TYPE").ok()).unwrap_or_else(|| "joyid_ckb".to_string());
        if principal_type != "joyid_ckb" && principal_type != "ckb_secp256k1" {
            return Err(crate::error::CompileError::without_span("principal type must be joyid_ckb or ckb_secp256k1"));
        }
        let principal_id = args.principal_id.or_else(|| std::env::var("CELLSCRIPT_PRINCIPAL_ID").ok()).ok_or_else(|| {
            crate::error::CompileError::without_span(
                "principal id is required; pass --principal-id or set CELLSCRIPT_PRINCIPAL_ID to the normalized wallet identity binding",
            )
        })?;
        let explicit_capability_pubkey = args.capability_pubkey.or_else(|| std::env::var("CELLSCRIPT_CAPABILITY_PUBKEY").ok());
        let (capability_pubkey, generated_key_id) = match explicit_capability_pubkey {
            Some(capability_pubkey) => (capability_pubkey, None),
            None => {
                let generated = generate_and_store_registry_capability_key()?;
                (generated.capability_pubkey, Some(generated.key_id))
            }
        };
        let capability_key_id = registry_capability_key_id(&capability_pubkey);
        let requested_scopes = resolve_requested_scopes(args.scopes)?;
        let issued_at = current_utc_timestamp();
        let expires_at = utc_timestamp_after_seconds(10 * 60);
        let capability_expires_at = resolve_capability_expires_at(args.capability_expires_at, args.expires)?;
        let nonce =
            registry_auth_nonce(&registry_origin, &principal_type, &principal_id, &capability_pubkey, &requested_scopes, &issued_at);
        let payload = crate::package::registry::CapabilityAuthorisationPayload::new(
            registry_origin,
            principal_type,
            principal_id,
            capability_pubkey,
            requested_scopes,
            capability_expires_at,
            nonce,
            issued_at,
            expires_at,
            crate::VERSION.to_string(),
        );
        let machine = serde_json::to_value(&payload)?;
        let mut human_lines = vec![
            "Capability authorisation payload".green().to_string(),
            format!("  Protocol: {}", payload.protocol),
            format!("  Action: {}", payload.action),
            format!("  Registry: {}", payload.registry_origin),
            format!("  Principal: {}:{}", payload.principal_type, payload.principal_id),
            format!("  Capability pubkey: {}", payload.capability_pubkey),
            format!("  Capability key id: {}", capability_key_id),
        ];
        if generated_key_id.is_some() {
            human_lines.push("  Capability private key: stored in the OS keychain".to_string());
        }
        human_lines.extend([
            format!("  Scopes: {}", payload.requested_scopes.join(", ")),
            format!("  Capability expires: {}", payload.capability_expires_at),
            String::new(),
            "Sign this payload with the matching JoyID or CKB wallet, then submit the signed authorisation to the registry write API:"
                .to_string(),
            serde_json::to_string_pretty(&payload)?,
        ]);
        CommandOutcome { machine, human_lines }.emit(args.json)
    }

    pub(super) fn auth_reproducer_create(args: AuthReproducerCreateArgs) -> Result<()> {
        let builder_id = args.builder_id.trim().to_string();
        if builder_id.is_empty() || builder_id.len() > 200 {
            return Err(crate::error::CompileError::without_span("builder id must contain 1 to 200 characters"));
        }
        let trust_domain = args.trust_domain.trim().to_string();
        if trust_domain.is_empty() || trust_domain.len() > 200 {
            return Err(crate::error::CompileError::without_span("trust domain must contain 1 to 200 characters"));
        }

        let generated = generate_registry_key_material()?;
        let (private_key_storage, storage_line) = if let Some(path) = args.private_key_output {
            write_new_reproducer_private_key(&path, &generated.private_key_pkcs8)?;
            let path_display = path.display().to_string();
            (
                serde_json::json!({
                    "kind": "pkcs8_base64_file",
                    "path": &path_display,
                    "environment_variable": "CELLSCRIPT_REPRODUCER_PRIVATE_KEY_PKCS8_B64",
                }),
                format!("  Private key: restricted PKCS#8 base64 file at {path_display}"),
            )
        } else {
            store_registry_private_key(&generated.key_id, &generated.private_key_pkcs8)?;
            (
                serde_json::json!({
                    "kind": "os_keychain",
                    "service": "cellscript-registry",
                    "key_id": &generated.key_id,
                }),
                "  Private key: stored in the OS keychain".to_string(),
            )
        };
        let machine = serde_json::json!({
            "schema": "cellscript-reproducer-builder-enrollment-v1",
            "builder_id": &builder_id,
            "trust_domain": &trust_domain,
            "builder_key_id": &generated.key_id,
            "builder_public_key": &generated.public_key,
            "policy_builder": {
                "builder_id": &builder_id,
                "trust_domain": &trust_domain,
                "public_key": &generated.public_key,
            },
            "private_key_storage": private_key_storage,
        });
        let human_lines = vec![
            "Reproducer builder key created".green().to_string(),
            format!("  Builder: {builder_id}"),
            format!("  Trust domain: {trust_domain}"),
            format!("  Builder key id: {}", generated.key_id),
            format!("  Builder public key: {}", generated.public_key),
            storage_line,
            String::new(),
            "Send only policy_builder to the Registry operator. Keep the private key inside this builder's independent custody."
                .to_string(),
        ];
        CommandOutcome { machine, human_lines }.emit(args.json)
    }

    pub(super) fn auth_capability_submit(args: AuthCapabilitySubmitArgs) -> Result<()> {
        let api_base = resolve_registry_api_base(args.api_url)?;
        let registry_origin = registry_origin_from_api_base(&api_base)?;
        let payload = read_capability_authorisation_payload(&args.payload)?;
        if payload.registry_origin != registry_origin {
            return Err(crate::error::CompileError::without_span(format!(
                "capability payload registry_origin '{}' does not match API origin '{}'",
                payload.registry_origin, registry_origin
            )));
        }
        let wallet_signature = read_json_value(&args.wallet_signature)?;
        let body = serde_json::json!({
            "payload": payload,
            "wallet_signature": wallet_signature,
        });
        let endpoint = format!("{}/v1/capabilities", api_base.trim_end_matches('/'));
        let response = submit_registry_json_request(&endpoint, &body, "Submitted capability authorisation", args.json)?;
        if !args.json {
            if let Some(key_id) = response.get("key_id").and_then(serde_json::Value::as_str) {
                println!("  Capability key id: {}", key_id);
            }
            if let Some(status) = response.get("status").and_then(serde_json::Value::as_str) {
                println!("  Status: {}", status);
            }
        }
        Ok(())
    }

    pub(super) fn auth_namespace_claim(args: AuthNamespaceClaimArgs) -> Result<()> {
        let api_base = resolve_registry_api_base(args.api_url)?;
        let registry_origin = registry_origin_from_api_base(&api_base)?;
        let namespace = args.namespace.trim();
        if namespace.is_empty() {
            return Err(crate::error::CompileError::without_span("namespace is required for registry namespace claim"));
        }
        let payload = read_capability_authorisation_payload(&args.payload)?;
        if payload.registry_origin != registry_origin {
            return Err(crate::error::CompileError::without_span(format!(
                "namespace claim payload registry_origin '{}' does not match API origin '{}'",
                payload.registry_origin, registry_origin
            )));
        }
        let namespace_scope_prefix = format!("publish:{namespace}/");
        if !payload.requested_scopes.iter().any(|scope| scope.starts_with(&namespace_scope_prefix)) {
            return Err(crate::error::CompileError::without_span(format!(
                "namespace claim payload has no publish scope for namespace '{namespace}'"
            )));
        }
        let wallet_signature = read_json_value(&args.wallet_signature)?;
        let body = serde_json::json!({
            "namespace": namespace,
            "payload": payload,
            "wallet_signature": wallet_signature,
        });
        let endpoint = format!("{}/v1/namespaces/claim", api_base.trim_end_matches('/'));
        let response = submit_registry_json_request(&endpoint, &body, "Claimed registry namespace", args.json)?;
        if !args.json
            && let Some(status) = response.get("status").and_then(serde_json::Value::as_str)
        {
            println!("  Namespace: {namespace}");
            println!("  Status: {status}");
            if status != "active" {
                println!("  Publishing remains blocked until registry review activates the namespace.");
            }
        }
        Ok(())
    }

    pub(super) fn auth_capability_revoke(args: AuthCapabilityRevokeArgs) -> Result<()> {
        if args.payload.is_none() && args.wallet_signature.is_some() {
            return Err(crate::error::CompileError::without_span(
                "capability revocation with --wallet-signature must use --payload from a previously generated revoke challenge",
            ));
        }

        let payload = if let Some(payload_path) = args.payload.as_deref() {
            read_capability_revocation_payload(payload_path)?
        } else {
            let registry_origin = args
                .registry_origin
                .or_else(|| std::env::var("CELLSCRIPT_REGISTRY_ORIGIN").ok())
                .unwrap_or_else(|| crate::package::registry::DEFAULT_PUBLIC_REGISTRY_ORIGIN.to_string());
            let principal_type = args
                .principal_type
                .or_else(|| std::env::var("CELLSCRIPT_PRINCIPAL_TYPE").ok())
                .unwrap_or_else(|| "joyid_ckb".to_string());
            if principal_type != "joyid_ckb" && principal_type != "ckb_secp256k1" {
                return Err(crate::error::CompileError::without_span("principal type must be joyid_ckb or ckb_secp256k1"));
            }
            let principal_id = args.principal_id.or_else(|| std::env::var("CELLSCRIPT_PRINCIPAL_ID").ok()).ok_or_else(|| {
                crate::error::CompileError::without_span(
                    "principal id is required for capability revoke; pass --principal-id or set CELLSCRIPT_PRINCIPAL_ID to the normalized wallet identity binding",
                )
            })?;
            let capability_key_id =
                args.capability_key_id.or_else(|| std::env::var("CELLSCRIPT_CAPABILITY_KEY_ID").ok()).ok_or_else(|| {
                    crate::error::CompileError::without_span(
                        "capability key id is required for capability revoke; pass --capability-key-id or set CELLSCRIPT_CAPABILITY_KEY_ID",
                    )
                })?;
            let issued_at = current_utc_timestamp();
            let expires_at = utc_timestamp_after_seconds(10 * 60);
            let nonce = registry_revoke_nonce(&registry_origin, &principal_type, &principal_id, &capability_key_id, &issued_at);
            crate::package::registry::CapabilityRevocationPayload::new(
                registry_origin,
                principal_type,
                principal_id,
                capability_key_id,
                nonce,
                issued_at,
                expires_at,
                crate::VERSION.to_string(),
            )
        };

        let Some(signature_path) = args.wallet_signature.as_deref() else {
            if args.json {
                print_json(&serde_json::to_value(&payload)?)?;
            } else {
                println!("{}", "Capability revocation payload".green());
                println!("  Protocol: {}", payload.protocol);
                println!("  Action: {}", payload.action);
                println!("  Registry: {}", payload.registry_origin);
                println!("  Principal: {}:{}", payload.principal_type, payload.principal_id);
                println!("  Capability key id: {}", payload.capability_key_id);
                println!();
                println!("Sign this payload with the matching JoyID or CKB wallet, then submit it with:");
                println!("  cellc auth capability revoke --payload <payload.json> --wallet-signature <wallet-signature.json>");
                println!("{}", serde_json::to_string_pretty(&payload)?);
            }
            return Ok(());
        };

        let api_base = resolve_registry_api_base(args.api_url)?;
        let registry_origin = registry_origin_from_api_base(&api_base)?;
        if payload.registry_origin != registry_origin {
            return Err(crate::error::CompileError::without_span(format!(
                "capability revocation registry_origin '{}' does not match API origin '{}'",
                payload.registry_origin, registry_origin
            )));
        }
        let wallet_signature = read_json_value(signature_path)?;
        let mut body = serde_json::json!({
            "payload": payload,
            "wallet_signature": wallet_signature,
        });
        if let Some(reason) = args.reason.filter(|reason| !reason.trim().is_empty()) {
            body["reason"] = serde_json::Value::String(reason);
        }
        let endpoint = format!(
            "{}/v1/capabilities/{}/revoke",
            api_base.trim_end_matches('/'),
            body["payload"]["capability_key_id"].as_str().unwrap_or_default()
        );
        let response = submit_registry_json_request(&endpoint, &body, "Revoked capability", args.json)?;
        if !args.json {
            if let Some(key_id) = response.get("key_id").and_then(serde_json::Value::as_str) {
                println!("  Capability key id: {}", key_id);
            }
            if let Some(revoked_at) = response.get("revoked_at").and_then(serde_json::Value::as_str) {
                println!("  Revoked at: {}", revoked_at);
            }
        }
        Ok(())
    }
}

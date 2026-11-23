use super::{GeneratedRegistryCapabilityKey, GeneratedRegistryKeyMaterial, RegistryKeychainSecret, REGISTRY_KEYCHAIN_SECRET_SCHEMA};
use crate::error::Result;
use base64::Engine;
use ring::signature::KeyPair;
use std::io::Write;
use std::path::Path;

pub(super) fn generate_registry_key_material() -> Result<GeneratedRegistryKeyMaterial> {
    let rng = ring::rand::SystemRandom::new();
    let pkcs8 = ring::signature::EcdsaKeyPair::generate_pkcs8(&ring::signature::ECDSA_P256_SHA256_FIXED_SIGNING, &rng)
        .map_err(|error| crate::error::CompileError::without_span(format!("failed to generate P-256 registry key: {:?}", error)))?;
    let key_pair = ring::signature::EcdsaKeyPair::from_pkcs8(&ring::signature::ECDSA_P256_SHA256_FIXED_SIGNING, pkcs8.as_ref(), &rng)
        .map_err(|error| {
            crate::error::CompileError::without_span(format!("failed to load generated P-256 registry key: {:?}", error))
        })?;
    let spki = p256_spki_der_from_uncompressed_public_key(key_pair.public_key().as_ref())?;
    let public_key = format!("p256-spki:{}", base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(spki));
    let key_id = registry_capability_key_id(&public_key);
    Ok(GeneratedRegistryKeyMaterial { key_id, public_key, private_key_pkcs8: pkcs8.as_ref().to_vec() })
}

pub(super) fn generate_and_store_registry_capability_key() -> Result<GeneratedRegistryCapabilityKey> {
    let generated = generate_registry_key_material()?;
    store_registry_private_key(&generated.key_id, &generated.private_key_pkcs8)?;
    Ok(GeneratedRegistryCapabilityKey { key_id: generated.key_id, capability_pubkey: generated.public_key })
}

pub(super) fn registry_capability_key_id(capability_pubkey: &str) -> String {
    use sha2::Digest as _;
    let digest = sha2::Sha256::digest(capability_pubkey.as_bytes());
    format!("cap_{}", &hex::encode(digest)[..32])
}

fn p256_spki_der_from_uncompressed_public_key(public_key: &[u8]) -> Result<Vec<u8>> {
    const P256_SPKI_PREFIX: &[u8] = &[
        0x30, 0x59, 0x30, 0x13, 0x06, 0x07, 0x2a, 0x86, 0x48, 0xce, 0x3d, 0x02, 0x01, 0x06, 0x08, 0x2a, 0x86, 0x48, 0xce, 0x3d, 0x03,
        0x01, 0x07, 0x03, 0x42, 0x00,
    ];
    if public_key.len() != 65 || public_key.first() != Some(&0x04) {
        return Err(crate::error::CompileError::without_span(format!(
            "generated registry public key must be an uncompressed 65-byte P-256 point, got {} bytes",
            public_key.len()
        )));
    }
    let mut spki = Vec::with_capacity(P256_SPKI_PREFIX.len() + public_key.len());
    spki.extend_from_slice(P256_SPKI_PREFIX);
    spki.extend_from_slice(public_key);
    Ok(spki)
}

pub(super) fn store_registry_private_key(key_id: &str, pkcs8: &[u8]) -> Result<()> {
    store_registry_keychain_secret(
        key_id,
        &RegistryKeychainSecret {
            schema: REGISTRY_KEYCHAIN_SECRET_SCHEMA.to_string(),
            status: "active".to_string(),
            pkcs8_b64: base64::engine::general_purpose::STANDARD.encode(pkcs8),
            session_id: None,
            expires_at: None,
            pending_expires_at_unix_seconds: None,
        },
    )
}

pub(super) fn store_pending_registry_private_key(key_id: &str, pkcs8: &[u8], session_id: &str, expires_at: &str) -> Result<()> {
    let pending_expires_at_unix_seconds =
        std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap_or_default().as_secs().saturating_add(15 * 60);
    store_registry_keychain_secret(
        key_id,
        &RegistryKeychainSecret {
            schema: REGISTRY_KEYCHAIN_SECRET_SCHEMA.to_string(),
            status: "pending".to_string(),
            pkcs8_b64: base64::engine::general_purpose::STANDARD.encode(pkcs8),
            session_id: Some(session_id.to_string()),
            expires_at: Some(expires_at.to_string()),
            pending_expires_at_unix_seconds: Some(pending_expires_at_unix_seconds),
        },
    )
}

fn store_registry_keychain_secret(key_id: &str, secret: &RegistryKeychainSecret) -> Result<()> {
    let encoded = serde_json::to_string(secret).map_err(|error| {
        crate::error::CompileError::without_span(format!("failed to encode registry private-key state: {error}"))
            .with_category(crate::error::CompileErrorCategory::Authentication)
    })?;
    let entry = keyring::Entry::new("cellscript-registry", key_id).map_err(|error| {
        crate::error::CompileError::without_span(format!("failed to open OS keychain: {}", error))
            .with_category(crate::error::CompileErrorCategory::Authentication)
            .with_source(error)
    })?;
    entry.set_password(&encoded).map_err(|error| {
        crate::error::CompileError::without_span(format!(
            "failed to store registry P-256 private key '{}' in OS keychain: {}",
            key_id, error
        ))
        .with_category(crate::error::CompileErrorCategory::Authentication)
        .with_source(error)
    })
}

pub(super) fn remove_registry_private_key(key_id: &str) -> Result<()> {
    let entry = keyring::Entry::new("cellscript-registry", key_id).map_err(|error| {
        crate::error::CompileError::without_span(format!("failed to open OS keychain: {}", error))
            .with_category(crate::error::CompileErrorCategory::Authentication)
            .with_source(error)
    })?;
    match entry.delete_credential() {
        Ok(()) | Err(keyring::Error::NoEntry) => Ok(()),
        Err(error) => Err(crate::error::CompileError::without_span(format!(
            "failed to remove pending registry private key '{}' from OS keychain: {}",
            key_id, error
        ))
        .with_category(crate::error::CompileErrorCategory::Authentication)
        .with_source(error)),
    }
}

pub(super) fn write_new_reproducer_private_key(path: &Path, pkcs8: &[u8]) -> Result<()> {
    #[cfg(not(unix))]
    {
        let _ = (path, pkcs8);
        return Err(crate::error::CompileError::without_span(
            "--private-key-output requires Unix mode-0600 permission semantics; use the OS keychain on this platform",
        )
        .with_category(crate::error::CompileErrorCategory::Authentication));
    }

    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt as _;

        let mut options = std::fs::OpenOptions::new();
        options.write(true).create_new(true);
        options.mode(0o600);
        let mut file = options.open(path).map_err(|error| {
            crate::error::CompileError::without_span(format!(
                "failed to create reproducer private-key file '{}': {}",
                path.display(),
                error
            ))
            .with_category(crate::error::CompileErrorCategory::Authentication)
            .with_source(error)
        })?;
        let secret = base64::engine::general_purpose::STANDARD.encode(pkcs8);
        file.write_all(secret.as_bytes()).and_then(|_| file.write_all(b"\n")).map_err(|error| {
            crate::error::CompileError::without_span(format!(
                "failed to write reproducer private-key file '{}': {}",
                path.display(),
                error
            ))
            .with_category(crate::error::CompileErrorCategory::Authentication)
            .with_source(error)
        })?;
        file.sync_all().map_err(|error| {
            crate::error::CompileError::without_span(format!(
                "failed to sync reproducer private-key file '{}': {}",
                path.display(),
                error
            ))
            .with_category(crate::error::CompileErrorCategory::Authentication)
            .with_source(error)
        })
    }
}

pub(super) fn sign_registry_publish_payload(key_id: &str, canonical_payload: &str) -> Result<String> {
    sign_registry_capability_payload(key_id, canonical_payload)
}

pub(in crate::cli) fn sign_registry_capability_payload(key_id: &str, canonical_payload: &str) -> Result<String> {
    let Some(pkcs8) = load_registry_capability_private_key(key_id)? else {
        return Err(
            crate::error::CompileError::without_span(format!(
                "capability signature is required for public publish and no private key was found for '{}' in the OS keychain; pass --capability-signature, set CELLSCRIPT_CAPABILITY_SIGNATURE, or set CELLSCRIPT_CAPABILITY_PRIVATE_KEY_PKCS8_B64 for CI",
                key_id
            ))
            .with_category(crate::error::CompileErrorCategory::Authentication),
        );
    };
    sign_registry_publish_payload_with_pkcs8(&pkcs8, canonical_payload)
}

pub(in crate::cli) fn sign_registry_reproducer_payload(key_id: &str, canonical_payload: &str) -> Result<String> {
    let Some(pkcs8) = load_registry_reproducer_private_key(key_id)? else {
        return Err(
            crate::error::CompileError::without_span(format!(
                "reproducer signature key '{}' was not found in the OS keychain; set CELLSCRIPT_REPRODUCER_PRIVATE_KEY_PKCS8_B64 for an isolated builder",
                key_id
            ))
            .with_category(crate::error::CompileErrorCategory::Authentication),
        );
    };
    sign_registry_publish_payload_with_pkcs8(&pkcs8, canonical_payload)
}

fn load_registry_capability_private_key(key_id: &str) -> Result<Option<Vec<u8>>> {
    if let Ok(value) = std::env::var("CELLSCRIPT_CAPABILITY_PRIVATE_KEY_PKCS8_B64") {
        let trimmed = value.trim();
        if !trimmed.is_empty() {
            let decoded = base64::engine::general_purpose::STANDARD.decode(trimmed).map_err(|error| {
                crate::error::CompileError::without_span(format!(
                    "failed to decode CELLSCRIPT_CAPABILITY_PRIVATE_KEY_PKCS8_B64: {}",
                    error
                ))
            })?;
            return Ok(Some(decoded));
        }
    }

    load_registry_keychain_private_key(key_id)
}

fn load_registry_reproducer_private_key(key_id: &str) -> Result<Option<Vec<u8>>> {
    if let Ok(value) = std::env::var("CELLSCRIPT_REPRODUCER_PRIVATE_KEY_PKCS8_B64") {
        let trimmed = value.trim();
        if !trimmed.is_empty() {
            let decoded = base64::engine::general_purpose::STANDARD.decode(trimmed).map_err(|error| {
                crate::error::CompileError::without_span(format!(
                    "failed to decode CELLSCRIPT_REPRODUCER_PRIVATE_KEY_PKCS8_B64: {}",
                    error
                ))
            })?;
            return Ok(Some(decoded));
        }
    }

    load_registry_keychain_private_key(key_id)
}

fn load_registry_keychain_private_key(key_id: &str) -> Result<Option<Vec<u8>>> {
    let entry = keyring::Entry::new("cellscript-registry", key_id).map_err(|error| {
        crate::error::CompileError::without_span(format!("failed to open OS keychain: {}", error))
            .with_category(crate::error::CompileErrorCategory::Authentication)
            .with_source(error)
    })?;
    match entry.get_password() {
        Ok(secret) => {
            let trimmed = secret.trim();
            let encoded = if trimmed.starts_with('{') {
                let stored: RegistryKeychainSecret = serde_json::from_str(trimmed).map_err(|error| {
                    crate::error::CompileError::without_span(format!(
                        "failed to decode registry private-key state '{}' from OS keychain: {}",
                        key_id, error
                    ))
                    .with_category(crate::error::CompileErrorCategory::Authentication)
                })?;
                if stored.schema != REGISTRY_KEYCHAIN_SECRET_SCHEMA || !matches!(stored.status.as_str(), "pending" | "active") {
                    return Err(crate::error::CompileError::without_span(format!(
                        "registry private key '{}' has an unsupported OS keychain state",
                        key_id
                    ))
                    .with_category(crate::error::CompileErrorCategory::Authentication));
                }
                // A process may exit after the wallet commits authority but before the CLI
                // observes it. Only an observed terminal session state may remove a pending
                // key; local time alone cannot distinguish that case from an abandoned session.
                stored.pkcs8_b64
            } else {
                // Compatibility with keys written before the key lifecycle envelope was introduced.
                trimmed.to_string()
            };
            let decoded = base64::engine::general_purpose::STANDARD.decode(encoded.trim()).map_err(|error| {
                crate::error::CompileError::without_span(format!(
                    "failed to decode capability private key '{}' from OS keychain: {}",
                    key_id, error
                ))
            })?;
            Ok(Some(decoded))
        }
        Err(keyring::Error::NoEntry) => Ok(None),
        Err(error) => Err(crate::error::CompileError::without_span(format!(
            "failed to read capability private key '{}' from OS keychain: {}",
            key_id, error
        ))
        .with_category(crate::error::CompileErrorCategory::Authentication)
        .with_source(error)),
    }
}

fn sign_registry_publish_payload_with_pkcs8(pkcs8: &[u8], canonical_payload: &str) -> Result<String> {
    let rng = ring::rand::SystemRandom::new();
    let key_pair = ring::signature::EcdsaKeyPair::from_pkcs8(&ring::signature::ECDSA_P256_SHA256_FIXED_SIGNING, pkcs8, &rng)
        .map_err(|error| crate::error::CompileError::without_span(format!("failed to load capability private key: {:?}", error)))?;
    let signature = key_pair
        .sign(&rng, canonical_payload.as_bytes())
        .map_err(|error| crate::error::CompileError::without_span(format!("failed to sign publish payload: {:?}", error)))?;
    Ok(base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(signature.as_ref()))
}

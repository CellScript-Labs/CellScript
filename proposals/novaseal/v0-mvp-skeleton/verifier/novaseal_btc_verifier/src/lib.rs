pub use novaseal_btc_verifier_core::{
    IPC_BLOB_LEN, IPC_FLAGS_NONE, IPC_MAGIC, IPC_SCHEME_BIP340, IPC_VERSION, IpcEnvelopeError, IpcRequest, VerifyError,
    parse_ipc_request, verify_bip340_message32,
};
use thiserror::Error;

#[derive(Debug, Error, Eq, PartialEq)]
pub enum IpcError {
    #[error("{0}")]
    Envelope(IpcEnvelopeError),
    #[error("{0}")]
    Verification(VerifyError),
}

impl From<IpcEnvelopeError> for IpcError {
    fn from(error: IpcEnvelopeError) -> Self {
        Self::Envelope(error)
    }
}

impl From<VerifyError> for IpcError {
    fn from(error: VerifyError) -> Self {
        Self::Verification(error)
    }
}

/// Verifies a fixed `NovaSeal` verifier IPC request envelope.
///
/// # Errors
///
/// Returns [`IpcError`] when envelope parsing fails or when BIP340 verification
/// rejects the contained request.
pub fn verify_ipc_blob(blob: &[u8]) -> Result<(), IpcError> {
    let request = parse_ipc_request(blob)?;
    verify_bip340_message32(request.message32, request.xonly_pubkey, request.signature64)?;
    Ok(())
}

/// Decodes a fixed-width hexadecimal value with an optional `0x` prefix.
///
/// # Errors
///
/// Returns an error string when the value is not valid hex or when the decoded
/// byte length differs from `expected_len`.
pub fn decode_fixed_hex(value: &str, expected_len: usize) -> Result<Vec<u8>, String> {
    let bytes = decode_hex(value)?;
    if bytes.len() != expected_len {
        return Err(format!("expected {expected_len} bytes, got {}", bytes.len()));
    }
    Ok(bytes)
}

/// Decodes a hexadecimal value with an optional `0x` prefix.
///
/// # Errors
///
/// Returns an error string when the value is not valid hex.
pub fn decode_hex(value: &str) -> Result<Vec<u8>, String> {
    let raw = value.strip_prefix("0x").unwrap_or(value);
    hex::decode(raw).map_err(|err| format!("invalid hex: {err}"))
}

#[cfg(test)]
mod tests {
    use super::{
        IPC_BLOB_LEN, IPC_FLAGS_NONE, IPC_MAGIC, IPC_SCHEME_BIP340, IPC_VERSION, IpcEnvelopeError, VerifyError, decode_fixed_hex,
        parse_ipc_request, verify_bip340_message32, verify_ipc_blob,
    };

    const MESSAGE: &str = "0x47b7551bbdb7061ffdea17a9f3049503e676e8612b5af2a7103bfd4f23524d08";
    const WRONG_MESSAGE: &str = "0x46b7551bbdb7061ffdea17a9f3049503e676e8612b5af2a7103bfd4f23524d08";
    const PUBKEY: &str = "0xc89fe99d72fcfa969434ddd87bb186a48213e9df3ec4b8a77042cf9559fc5765";
    const SIGNATURE: &str = "0x07e8c447f2af09fcee87a6541a3bc6d9ca78579ec657e9e41b19e07db67c84d63d29193877d7489965cb716b9a39069e7482ca4adf3358ef59a7e503409a14a2";

    #[test]
    fn verifies_reference_positive_vector() {
        let message = decode_fixed_hex(MESSAGE, 32).unwrap();
        let pubkey = decode_fixed_hex(PUBKEY, 32).unwrap();
        let signature = decode_fixed_hex(SIGNATURE, 64).unwrap();
        assert_eq!(verify_bip340_message32(&message, &pubkey, &signature), Ok(()));
    }

    #[test]
    fn rejects_wrong_message_vector() {
        let message = decode_fixed_hex(WRONG_MESSAGE, 32).unwrap();
        let pubkey = decode_fixed_hex(PUBKEY, 32).unwrap();
        let signature = decode_fixed_hex(SIGNATURE, 64).unwrap();
        assert_eq!(verify_bip340_message32(&message, &pubkey, &signature), Err(VerifyError::VerificationFailed));
    }

    #[test]
    fn rejects_wrong_lengths_before_crypto() {
        assert_eq!(verify_bip340_message32(&[], &[0; 32], &[0; 64]), Err(VerifyError::MessageLength));
        assert_eq!(verify_bip340_message32(&[0; 32], &[], &[0; 64]), Err(VerifyError::PubkeyLength));
        assert_eq!(verify_bip340_message32(&[0; 32], &[0; 32], &[]), Err(VerifyError::SignatureLength));
    }

    #[test]
    fn verifies_reference_ipc_blob() {
        let message = decode_fixed_hex(MESSAGE, 32).unwrap();
        let pubkey = decode_fixed_hex(PUBKEY, 32).unwrap();
        let signature = decode_fixed_hex(SIGNATURE, 64).unwrap();
        let blob = ipc_blob(&message, &pubkey, &signature);
        assert_eq!(blob.len(), IPC_BLOB_LEN);
        assert_eq!(verify_ipc_blob(&blob), Ok(()));
    }

    #[test]
    fn rejects_malformed_ipc_envelope_before_crypto() {
        let message = decode_fixed_hex(MESSAGE, 32).unwrap();
        let pubkey = decode_fixed_hex(PUBKEY, 32).unwrap();
        let signature = decode_fixed_hex(SIGNATURE, 64).unwrap();
        let mut blob = ipc_blob(&message, &pubkey, &signature);

        assert_eq!(parse_ipc_request(&blob[..143]), Err(IpcEnvelopeError::BlobLength));

        blob[0] ^= 1;
        assert_eq!(parse_ipc_request(&blob), Err(IpcEnvelopeError::Magic));

        let mut blob = ipc_blob(&message, &pubkey, &signature);
        blob[8..10].copy_from_slice(&1u16.to_le_bytes());
        assert_eq!(parse_ipc_request(&blob), Err(IpcEnvelopeError::Version(1)));

        let mut blob = ipc_blob(&message, &pubkey, &signature);
        blob[10..12].copy_from_slice(&2u16.to_le_bytes());
        assert_eq!(parse_ipc_request(&blob), Err(IpcEnvelopeError::Scheme(2)));

        let mut blob = ipc_blob(&message, &pubkey, &signature);
        blob[12..16].copy_from_slice(&1u32.to_le_bytes());
        assert_eq!(parse_ipc_request(&blob), Err(IpcEnvelopeError::Flags(1)));
    }

    fn ipc_blob(message: &[u8], pubkey: &[u8], signature: &[u8]) -> Vec<u8> {
        let mut blob = Vec::with_capacity(IPC_BLOB_LEN);
        blob.extend_from_slice(IPC_MAGIC);
        blob.extend_from_slice(&IPC_VERSION.to_le_bytes());
        blob.extend_from_slice(&IPC_SCHEME_BIP340.to_le_bytes());
        blob.extend_from_slice(&IPC_FLAGS_NONE.to_le_bytes());
        blob.extend_from_slice(message);
        blob.extend_from_slice(pubkey);
        blob.extend_from_slice(signature);
        blob
    }
}

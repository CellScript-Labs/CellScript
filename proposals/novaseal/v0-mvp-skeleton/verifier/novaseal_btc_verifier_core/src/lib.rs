#![no_std]

use core::fmt;
use k256::schnorr::{Signature, VerifyingKey, signature::hazmat::PrehashVerifier};

pub const IPC_MAGIC: &[u8; 8] = b"NSBV0IPC";
pub const IPC_VERSION: u16 = 0;
pub const IPC_SCHEME_BIP340: u16 = 1;
pub const IPC_FLAGS_NONE: u32 = 0;
pub const IPC_BLOB_LEN: usize = 144;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum IpcEnvelopeError {
    BlobLength,
    Magic,
    Version(u16),
    Scheme(u16),
    Flags(u32),
}

impl fmt::Display for IpcEnvelopeError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::BlobLength => f.write_str("IPC blob must be exactly 144 bytes"),
            Self::Magic => f.write_str("IPC magic mismatch"),
            Self::Version(version) => write!(f, "unsupported IPC version {version}"),
            Self::Scheme(scheme) => write!(f, "unsupported IPC scheme {scheme}"),
            Self::Flags(flags) => write!(f, "unsupported IPC flags {flags}"),
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum VerifyError {
    MessageLength,
    PubkeyLength,
    SignatureLength,
    InvalidPubkey,
    InvalidSignature,
    VerificationFailed,
}

impl fmt::Display for VerifyError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::MessageLength => f.write_str("message must be exactly 32 bytes"),
            Self::PubkeyLength => f.write_str("x-only pubkey must be exactly 32 bytes"),
            Self::SignatureLength => f.write_str("signature must be exactly 64 bytes"),
            Self::InvalidPubkey => f.write_str("invalid x-only pubkey"),
            Self::InvalidSignature => f.write_str("invalid BIP340 signature encoding"),
            Self::VerificationFailed => f.write_str("signature verification failed"),
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum IpcVerificationError {
    Envelope(IpcEnvelopeError),
    Verification(VerifyError),
}

impl fmt::Display for IpcVerificationError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Envelope(error) => fmt::Display::fmt(error, f),
            Self::Verification(error) => fmt::Display::fmt(error, f),
        }
    }
}

impl From<IpcEnvelopeError> for IpcVerificationError {
    fn from(error: IpcEnvelopeError) -> Self {
        Self::Envelope(error)
    }
}

impl From<VerifyError> for IpcVerificationError {
    fn from(error: VerifyError) -> Self {
        Self::Verification(error)
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct IpcRequest<'a> {
    pub message32: &'a [u8; 32],
    pub xonly_pubkey: &'a [u8; 32],
    pub signature64: &'a [u8; 64],
}

/// Parses the fixed `NovaSeal` verifier IPC request envelope.
///
/// # Errors
///
/// Returns [`IpcEnvelopeError`] when the envelope has the wrong length, magic,
/// version, scheme, or flags.
pub fn parse_ipc_request(blob: &[u8]) -> Result<IpcRequest<'_>, IpcEnvelopeError> {
    if blob.len() != IPC_BLOB_LEN {
        return Err(IpcEnvelopeError::BlobLength);
    }
    if &blob[0..8] != IPC_MAGIC {
        return Err(IpcEnvelopeError::Magic);
    }

    let version = u16::from_le_bytes([blob[8], blob[9]]);
    if version != IPC_VERSION {
        return Err(IpcEnvelopeError::Version(version));
    }

    let scheme = u16::from_le_bytes([blob[10], blob[11]]);
    if scheme != IPC_SCHEME_BIP340 {
        return Err(IpcEnvelopeError::Scheme(scheme));
    }

    let flags = u32::from_le_bytes([blob[12], blob[13], blob[14], blob[15]]);
    if flags != IPC_FLAGS_NONE {
        return Err(IpcEnvelopeError::Flags(flags));
    }

    Ok(IpcRequest {
        message32: blob[16..48].try_into().map_err(|_| IpcEnvelopeError::BlobLength)?,
        xonly_pubkey: blob[48..80].try_into().map_err(|_| IpcEnvelopeError::BlobLength)?,
        signature64: blob[80..144].try_into().map_err(|_| IpcEnvelopeError::BlobLength)?,
    })
}

/// Verifies a 32-byte message against a BIP340 x-only public key and signature.
///
/// # Errors
///
/// Returns [`VerifyError`] when the message, key, or signature length is wrong,
/// when the key/signature encoding is invalid, or when signature verification
/// fails.
pub fn verify_bip340_message32(message: &[u8], xonly_pubkey: &[u8], signature: &[u8]) -> Result<(), VerifyError> {
    if message.len() != 32 {
        return Err(VerifyError::MessageLength);
    }
    if xonly_pubkey.len() != 32 {
        return Err(VerifyError::PubkeyLength);
    }
    if signature.len() != 64 {
        return Err(VerifyError::SignatureLength);
    }

    let key = VerifyingKey::from_bytes(xonly_pubkey).map_err(|_| VerifyError::InvalidPubkey)?;
    let sig = Signature::try_from(signature).map_err(|_| VerifyError::InvalidSignature)?;
    key.verify_prehash(message, &sig).map_err(|_| VerifyError::VerificationFailed)
}

/// Verifies a fixed `NovaSeal` verifier IPC request envelope.
///
/// # Errors
///
/// Returns [`IpcVerificationError`] when envelope parsing fails or when BIP340
/// verification rejects the contained request.
pub fn verify_ipc_blob(blob: &[u8]) -> Result<(), IpcVerificationError> {
    let request = parse_ipc_request(blob)?;
    verify_bip340_message32(request.message32, request.xonly_pubkey, request.signature64)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::{
        IPC_BLOB_LEN, IPC_FLAGS_NONE, IPC_MAGIC, IPC_SCHEME_BIP340, IPC_VERSION, IpcEnvelopeError, VerifyError, parse_ipc_request,
        verify_bip340_message32, verify_ipc_blob,
    };

    const MESSAGE: &str = "47b7551bbdb7061ffdea17a9f3049503e676e8612b5af2a7103bfd4f23524d08";
    const WRONG_MESSAGE: &str = "46b7551bbdb7061ffdea17a9f3049503e676e8612b5af2a7103bfd4f23524d08";
    const PUBKEY: &str = "c89fe99d72fcfa969434ddd87bb186a48213e9df3ec4b8a77042cf9559fc5765";
    const SIGNATURE: &str = "07e8c447f2af09fcee87a6541a3bc6d9ca78579ec657e9e41b19e07db67c84d63d29193877d7489965cb716b9a39069e7482ca4adf3358ef59a7e503409a14a2";

    #[test]
    fn parses_valid_ipc_envelope() {
        let blob = ipc_blob();
        let request = parse_ipc_request(&blob).unwrap();
        assert_eq!(request.message32, &[1; 32]);
        assert_eq!(request.xonly_pubkey, &[2; 32]);
        assert_eq!(request.signature64, &[3; 64]);
    }

    #[test]
    fn rejects_malformed_ipc_envelope_before_crypto() {
        let mut blob = ipc_blob();

        assert_eq!(parse_ipc_request(&blob[..143]), Err(IpcEnvelopeError::BlobLength));

        blob[0] ^= 1;
        assert_eq!(parse_ipc_request(&blob), Err(IpcEnvelopeError::Magic));

        let mut blob = ipc_blob();
        blob[8..10].copy_from_slice(&1u16.to_le_bytes());
        assert_eq!(parse_ipc_request(&blob), Err(IpcEnvelopeError::Version(1)));

        let mut blob = ipc_blob();
        blob[10..12].copy_from_slice(&2u16.to_le_bytes());
        assert_eq!(parse_ipc_request(&blob), Err(IpcEnvelopeError::Scheme(2)));

        let mut blob = ipc_blob();
        blob[12..16].copy_from_slice(&1u32.to_le_bytes());
        assert_eq!(parse_ipc_request(&blob), Err(IpcEnvelopeError::Flags(1)));
    }

    #[test]
    fn verifies_reference_bip340_vector() {
        let message = fixed_hex::<32>(MESSAGE);
        let pubkey = fixed_hex::<32>(PUBKEY);
        let signature = fixed_hex::<64>(SIGNATURE);

        assert_eq!(verify_bip340_message32(&message, &pubkey, &signature), Ok(()));
        assert_eq!(
            verify_bip340_message32(&fixed_hex::<32>(WRONG_MESSAGE), &pubkey, &signature),
            Err(VerifyError::VerificationFailed)
        );
    }

    #[test]
    fn verifies_reference_ipc_blob() {
        let mut blob = [0u8; IPC_BLOB_LEN];
        blob[0..8].copy_from_slice(IPC_MAGIC);
        blob[8..10].copy_from_slice(&IPC_VERSION.to_le_bytes());
        blob[10..12].copy_from_slice(&IPC_SCHEME_BIP340.to_le_bytes());
        blob[12..16].copy_from_slice(&IPC_FLAGS_NONE.to_le_bytes());
        blob[16..48].copy_from_slice(&fixed_hex::<32>(MESSAGE));
        blob[48..80].copy_from_slice(&fixed_hex::<32>(PUBKEY));
        blob[80..144].copy_from_slice(&fixed_hex::<64>(SIGNATURE));

        assert_eq!(verify_ipc_blob(&blob), Ok(()));

        blob[16] ^= 1;
        assert!(verify_ipc_blob(&blob).is_err());
    }

    fn ipc_blob() -> [u8; IPC_BLOB_LEN] {
        let mut blob = [0u8; IPC_BLOB_LEN];
        blob[0..8].copy_from_slice(IPC_MAGIC);
        blob[8..10].copy_from_slice(&IPC_VERSION.to_le_bytes());
        blob[10..12].copy_from_slice(&IPC_SCHEME_BIP340.to_le_bytes());
        blob[12..16].copy_from_slice(&IPC_FLAGS_NONE.to_le_bytes());
        blob[16..48].copy_from_slice(&[1; 32]);
        blob[48..80].copy_from_slice(&[2; 32]);
        blob[80..144].copy_from_slice(&[3; 64]);
        blob
    }

    fn fixed_hex<const N: usize>(value: &str) -> [u8; N] {
        assert_eq!(value.len(), N * 2);
        let mut out = [0u8; N];
        let mut index = 0usize;
        while index < N {
            out[index] = u8::from_str_radix(&value[index * 2..index * 2 + 2], 16).unwrap();
            index += 1;
        }
        out
    }
}

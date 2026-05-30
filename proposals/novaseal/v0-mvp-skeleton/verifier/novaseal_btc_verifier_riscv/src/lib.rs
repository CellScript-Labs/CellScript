#![no_std]

use novaseal_btc_verifier_core::{IPC_BLOB_LEN, IpcEnvelopeError, IpcVerificationError, VerifyError, verify_ipc_blob};

pub const EXIT_ACCEPT: u8 = 0;
pub const EXIT_REJECT_ENVELOPE: u8 = 10;
pub const EXIT_REJECT_SPAWN_IO: u8 = 11;
pub const EXIT_REJECT_CRYPTO: u8 = 12;
pub const IPC_WORD_COUNT: usize = IPC_BLOB_LEN / core::mem::size_of::<u64>();
pub const SPAWN_INPUT_FD_INDEX: u64 = 0;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum SpawnInputError {
    WordCount { actual: usize },
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ShellDecision {
    Accept,
    RejectSpawnInput(SpawnInputError),
    RejectEnvelope(IpcEnvelopeError),
    RejectCrypto(VerifyError),
}

impl ShellDecision {
    #[must_use]
    pub const fn exit_code(self) -> u8 {
        match self {
            Self::Accept => EXIT_ACCEPT,
            Self::RejectSpawnInput(_) => EXIT_REJECT_SPAWN_IO,
            Self::RejectEnvelope(_) => EXIT_REJECT_ENVELOPE,
            Self::RejectCrypto(_) => EXIT_REJECT_CRYPTO,
        }
    }

    #[must_use]
    pub const fn accepted(self) -> bool {
        matches!(self, Self::Accept)
    }
}

/// Applies the current RISC-V verifier shell policy.
///
/// # Errors
///
/// This function deliberately returns a [`ShellDecision`] instead of `Result`
/// so callers can map every failure class to a stable process exit code.
#[must_use]
pub fn decide(blob: &[u8]) -> ShellDecision {
    match verify_ipc_blob(blob) {
        Ok(()) => ShellDecision::Accept,
        Err(IpcVerificationError::Envelope(error)) => ShellDecision::RejectEnvelope(error),
        Err(IpcVerificationError::Verification(error)) => ShellDecision::RejectCrypto(error),
    }
}

/// Reconstructs the fixed IPC envelope from the u64 word stream used by the
/// current CKB Spawn/IPC helper surface.
///
/// # Errors
///
/// Returns [`SpawnInputError::WordCount`] unless the caller supplies exactly
/// 18 little-endian words, i.e. the 144-byte v0 IPC envelope.
pub fn ipc_blob_from_le_words(words: &[u64]) -> Result<[u8; IPC_BLOB_LEN], SpawnInputError> {
    if words.len() != IPC_WORD_COUNT {
        return Err(SpawnInputError::WordCount { actual: words.len() });
    }
    let mut blob = [0u8; IPC_BLOB_LEN];
    for (index, word) in words.iter().enumerate() {
        let offset = index * core::mem::size_of::<u64>();
        blob[offset..offset + core::mem::size_of::<u64>()].copy_from_slice(&word.to_le_bytes());
    }
    Ok(blob)
}

#[must_use]
pub fn decide_words(words: &[u64]) -> ShellDecision {
    match ipc_blob_from_le_words(words) {
        Ok(blob) => decide(&blob),
        Err(error) => ShellDecision::RejectSpawnInput(error),
    }
}

#[cfg(test)]
mod tests {
    use super::{
        EXIT_ACCEPT, EXIT_REJECT_CRYPTO, EXIT_REJECT_ENVELOPE, EXIT_REJECT_SPAWN_IO, IPC_WORD_COUNT, ShellDecision, SpawnInputError,
        decide, decide_words, ipc_blob_from_le_words,
    };
    use novaseal_btc_verifier_core::{
        IPC_BLOB_LEN, IPC_FLAGS_NONE, IPC_MAGIC, IPC_SCHEME_BIP340, IPC_VERSION, IpcEnvelopeError, VerifyError,
    };

    const MESSAGE: &str = "47b7551bbdb7061ffdea17a9f3049503e676e8612b5af2a7103bfd4f23524d08";
    const WRONG_MESSAGE: &str = "46b7551bbdb7061ffdea17a9f3049503e676e8612b5af2a7103bfd4f23524d08";
    const PUBKEY: &str = "c89fe99d72fcfa969434ddd87bb186a48213e9df3ec4b8a77042cf9559fc5765";
    const SIGNATURE: &str = "07e8c447f2af09fcee87a6541a3bc6d9ca78579ec657e9e41b19e07db67c84d63d29193877d7489965cb716b9a39069e7482ca4adf3358ef59a7e503409a14a2";

    #[test]
    fn accepts_valid_bip340_envelope() {
        let decision = decide(&valid_ipc_blob());
        assert_eq!(decision, ShellDecision::Accept);
        assert_eq!(decision.exit_code(), EXIT_ACCEPT);
        assert!(decision.accepted());
    }

    #[test]
    fn rejects_wrong_message_after_envelope_parse() {
        let mut blob = valid_ipc_blob();
        blob[16..48].copy_from_slice(&fixed_hex::<32>(WRONG_MESSAGE));
        let decision = decide(&blob);
        assert_eq!(decision, ShellDecision::RejectCrypto(VerifyError::VerificationFailed));
        assert_eq!(decision.exit_code(), EXIT_REJECT_CRYPTO);
        assert!(!decision.accepted());
    }

    #[test]
    fn rejects_malformed_envelope_before_crypto() {
        let mut blob = valid_ipc_blob();
        blob[0] ^= 1;
        let decision = decide(&blob);
        assert_eq!(decision, ShellDecision::RejectEnvelope(IpcEnvelopeError::Magic));
        assert_eq!(decision.exit_code(), EXIT_REJECT_ENVELOPE);
        assert!(!decision.accepted());
    }

    #[test]
    fn reconstructs_ipc_blob_from_spawn_word_stream() {
        let blob = valid_ipc_blob();
        let words = ipc_words(&blob);

        assert_eq!(words.len(), IPC_WORD_COUNT);
        assert_eq!(ipc_blob_from_le_words(&words), Ok(blob));
        assert_eq!(decide_words(&words), ShellDecision::Accept);
    }

    #[test]
    fn rejects_incomplete_spawn_word_stream_before_envelope_parse() {
        let words = [0u64; IPC_WORD_COUNT - 1];
        let decision = decide_words(&words);

        assert_eq!(decision, ShellDecision::RejectSpawnInput(SpawnInputError::WordCount { actual: IPC_WORD_COUNT - 1 }));
        assert_eq!(decision.exit_code(), EXIT_REJECT_SPAWN_IO);
        assert!(!decision.accepted());
    }

    fn valid_ipc_blob() -> [u8; IPC_BLOB_LEN] {
        let mut blob = [0u8; IPC_BLOB_LEN];
        blob[0..8].copy_from_slice(IPC_MAGIC);
        blob[8..10].copy_from_slice(&IPC_VERSION.to_le_bytes());
        blob[10..12].copy_from_slice(&IPC_SCHEME_BIP340.to_le_bytes());
        blob[12..16].copy_from_slice(&IPC_FLAGS_NONE.to_le_bytes());
        blob[16..48].copy_from_slice(&fixed_hex::<32>(MESSAGE));
        blob[48..80].copy_from_slice(&fixed_hex::<32>(PUBKEY));
        blob[80..144].copy_from_slice(&fixed_hex::<64>(SIGNATURE));
        blob
    }

    fn ipc_words(blob: &[u8; IPC_BLOB_LEN]) -> [u64; IPC_WORD_COUNT] {
        let mut words = [0u64; IPC_WORD_COUNT];
        for (index, chunk) in blob.chunks_exact(core::mem::size_of::<u64>()).enumerate() {
            let mut bytes = [0u8; core::mem::size_of::<u64>()];
            bytes.copy_from_slice(chunk);
            words[index] = u64::from_le_bytes(bytes);
        }
        words
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

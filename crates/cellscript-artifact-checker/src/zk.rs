//! Draft allocation-free wire contract for Groth16 transition research.
//! This does not admit a production ZK source type or verifier profile.
//! This module is also compiled by the no_std verifier child. Parsing is not
//! proof verification: only the exact child/VK and pairing check can admit it.

pub const PROFILE: &str = "cellscript-zk-transition-groth16-bn254-v2";
pub const PROFILE_PREIMAGE: &str = "cellscript-zk-transition-groth16-bn254-v2|arkworks-compressed-proof-128|hash-u128-limbs-le|domain,action,script,old-data,new-data,input-tx,index-u32,transaction-hash|molecule-request-464";
pub const PROFILE_ID: [u8; 32] = [
    0x43, 0x0e, 0x4c, 0xf7, 0xa6, 0x67, 0x7d, 0x9b, 0xfa, 0x16, 0x19, 0xd7, 0x24, 0x40, 0xd3, 0x92, 0xe9, 0xec, 0x77, 0x24, 0x86,
    0xc4, 0xe7, 0x85, 0x1a, 0xaf, 0x91, 0xdf, 0x58, 0x88, 0x11, 0x01,
];
pub const PROOF_BYTES: usize = 128;
pub const STATEMENT_BYTES: usize = 228;
pub const PUBLIC_INPUT_COUNT: usize = 15;
pub const PUBLIC_INPUT_BYTES: usize = 4 + PUBLIC_INPUT_COUNT * 32;
pub const VK_BYTES: usize = 232 + (PUBLIC_INPUT_COUNT + 1) * 32;
pub const REQUEST_BYTES: usize = 464;
pub const REQUEST_WORDS: usize = REQUEST_BYTES / 8;
pub const MAGIC: &[u8; 8] = b"CSZKIPC1";
// Molecule table: total size, then six canonical field offsets. Every field
// has fixed width; compatible extra fields and trailing bytes are forbidden.
pub const TABLE_WORDS: [u32; 7] = [464, 28, 36, 44, 76, 108, 236];

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DecodeError {
    Length,
    Offsets,
    Magic,
    VersionOrFlags,
    Profile,
    VerificationKey,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Statement {
    pub domain: [u8; 32],
    pub action: [u8; 32],
    pub script_hash: [u8; 32],
    pub old_data_hash: [u8; 32],
    pub new_data_hash: [u8; 32],
    pub input_transaction_hash: [u8; 32],
    pub input_output_index: u32,
    pub transaction_hash: [u8; 32],
}

impl Statement {
    pub fn encode(&self) -> [u8; STATEMENT_BYTES] {
        let mut bytes = [0; STATEMENT_BYTES];
        for (index, value) in
            [&self.domain, &self.action, &self.script_hash, &self.old_data_hash, &self.new_data_hash, &self.input_transaction_hash]
                .iter()
                .enumerate()
        {
            bytes[index * 32..(index + 1) * 32].copy_from_slice(*value);
        }
        bytes[192..196].copy_from_slice(&self.input_output_index.to_le_bytes());
        bytes[196..].copy_from_slice(&self.transaction_hash);
        bytes
    }

    pub fn decode(bytes: &[u8]) -> Result<Self, DecodeError> {
        if bytes.len() != STATEMENT_BYTES {
            return Err(DecodeError::Length);
        }
        let hash = |index: usize| {
            let mut hash = [0; 32];
            hash.copy_from_slice(&bytes[index * 32..(index + 1) * 32]);
            hash
        };
        Ok(Self {
            domain: hash(0),
            action: hash(1),
            script_hash: hash(2),
            old_data_hash: hash(3),
            new_data_hash: hash(4),
            input_transaction_hash: hash(5),
            input_output_index: u32::from_le_bytes([bytes[192], bytes[193], bytes[194], bytes[195]]),
            transaction_hash: bytes[196..228].try_into().map_err(|_| DecodeError::Length)?,
        })
    }

    /// Inject each hash as two unsigned little-endian 128-bit limbs. Every
    /// value is strictly below the BN254 scalar modulus; no modular reduction
    /// or truncated-hash binding is involved. Input 12 is the u32 index;
    /// inputs 13 and 14 bind the full raw transaction hash.
    pub fn public_inputs(&self) -> [u8; PUBLIC_INPUT_BYTES] {
        let statement = self.encode();
        let mut bytes = [0; PUBLIC_INPUT_BYTES];
        bytes[..4].copy_from_slice(&(PUBLIC_INPUT_COUNT as u32).to_le_bytes());
        for index in 0..12 {
            bytes[4 + index * 32..20 + index * 32].copy_from_slice(&statement[index * 16..(index + 1) * 16]);
        }
        bytes[4 + 12 * 32..8 + 12 * 32].copy_from_slice(&self.input_output_index.to_le_bytes());
        for i in 0..2 {
            bytes[420 + i * 32..436 + i * 32].copy_from_slice(&self.transaction_hash[i * 16..(i + 1) * 16]);
        }
        bytes
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Request {
    pub verification_key: [u8; 32],
    pub proof: [u8; PROOF_BYTES],
    pub statement: Statement,
}

impl Request {
    pub fn encode(&self) -> [u8; REQUEST_BYTES] {
        let mut bytes = [0; REQUEST_BYTES];
        for (index, value) in TABLE_WORDS.iter().enumerate() {
            bytes[index * 4..index * 4 + 4].copy_from_slice(&value.to_le_bytes());
        }
        bytes[28..36].copy_from_slice(MAGIC);
        bytes[36..40].copy_from_slice(&1u32.to_le_bytes());
        bytes[44..76].copy_from_slice(&PROFILE_ID);
        bytes[76..108].copy_from_slice(&self.verification_key);
        bytes[108..236].copy_from_slice(&self.proof);
        bytes[236..].copy_from_slice(&self.statement.encode());
        bytes
    }

    pub fn decode(bytes: &[u8], expected_key: &[u8; 32]) -> Result<Self, DecodeError> {
        if bytes.len() != REQUEST_BYTES {
            return Err(DecodeError::Length);
        }
        for (index, expected) in TABLE_WORDS.iter().enumerate() {
            if bytes[index * 4..index * 4 + 4] != expected.to_le_bytes() {
                return Err(DecodeError::Offsets);
            }
        }
        if &bytes[28..36] != MAGIC {
            return Err(DecodeError::Magic);
        }
        if bytes[36..44] != [1, 0, 0, 0, 0, 0, 0, 0] {
            return Err(DecodeError::VersionOrFlags);
        }
        if bytes[44..76] != PROFILE_ID {
            return Err(DecodeError::Profile);
        }
        if &bytes[76..108] != expected_key {
            return Err(DecodeError::VerificationKey);
        }
        let mut proof = [0; PROOF_BYTES];
        proof.copy_from_slice(&bytes[108..236]);
        Ok(Self { verification_key: *expected_key, proof, statement: Statement::decode(&bytes[236..])? })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn request() -> Request {
        Request {
            verification_key: [7; 32],
            proof: [9; PROOF_BYTES],
            statement: Statement {
                domain: [1; 32],
                action: [2; 32],
                script_hash: [3; 32],
                old_data_hash: [4; 32],
                new_data_hash: [5; 32],
                input_transaction_hash: [0xff; 32],
                input_output_index: u32::MAX,
                transaction_hash: [0x99; 32],
            },
        }
    }

    #[test]
    fn exact_molecule_offsets_and_all_header_mutations() {
        let request = request();
        let bytes = request.encode();
        assert_eq!(Request::decode(&bytes, &[7; 32]), Ok(request));
        for index in 0..108 {
            let mut mutated = bytes;
            mutated[index] ^= 1;
            assert!(Request::decode(&mutated, &[7; 32]).is_err(), "header byte {index}");
        }
        for len in 0..REQUEST_BYTES {
            assert_eq!(Request::decode(&bytes[..len], &[7; 32]), Err(DecodeError::Length));
        }
        let mut long = bytes.to_vec();
        long.push(0);
        assert_eq!(Request::decode(&long, &[7; 32]), Err(DecodeError::Length));
        assert_eq!(Request::decode(&bytes, &[8; 32]), Err(DecodeError::VerificationKey));
    }

    #[test]
    fn field_injection_preserves_every_bit_and_canonical_ranges() {
        let statement = request().statement;
        let encoded = statement.encode();
        let inputs = statement.public_inputs();
        assert_eq!(&inputs[..4], &15u32.to_le_bytes());
        for index in 0..12 {
            assert_eq!(&inputs[4 + index * 32..20 + index * 32], &encoded[index * 16..(index + 1) * 16]);
            assert!(inputs[20 + index * 32..36 + index * 32].iter().all(|byte| *byte == 0));
        }
        assert_eq!(&inputs[388..392], &u32::MAX.to_le_bytes());
        assert!(inputs[392..420].iter().all(|byte| *byte == 0));
        for index in 0..2 {
            assert_eq!(&inputs[420 + index * 32..436 + index * 32], &statement.transaction_hash[index * 16..(index + 1) * 16]);
            assert!(inputs[436 + index * 32..452 + index * 32].iter().all(|byte| *byte == 0));
        }
        for index in 0..STATEMENT_BYTES {
            let mut changed = encoded;
            changed[index] ^= 1;
            assert_ne!(Statement::decode(&changed).unwrap().public_inputs(), inputs);
        }
    }
}

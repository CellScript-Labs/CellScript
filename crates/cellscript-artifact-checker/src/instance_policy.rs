//! Versioned, bounded Script-instance policies layered over an unchanged v1
//! implementation selection. This module proves byte-level policy membership
//! only. The caller must authenticate the consumer args, independently check
//! the implementation receipt, and bind the selected Script to a real Cell.
//! It does not grant ownership, peer execution, liveness or chain evidence.

use crate::ckb_blake2b256;
use crate::open_handle_policy::{self, CodeHashType, Hash, PolicyError, PolicyMembership, ScriptRole};
use std::fmt;

pub const MAX_ARGS: usize = 256;
pub const MAX_CONFIG: usize = 128;
pub const MAX_CONSTRAINTS: usize = 16;
pub const CONSTRAINT_BYTES: usize = 48;
pub const POLICY_HEADER_BYTES: usize = 80;
pub const POLICY_BYTES: usize = POLICY_HEADER_BYTES + MAX_CONSTRAINTS * CONSTRAINT_BYTES;
pub const CONSUMER_ARGS_BYTES: usize = 80 + MAX_CONFIG;
pub const SELECTION_BYTES: usize = 8 + POLICY_BYTES + open_handle_policy::SELECTION_BYTES + MAX_ARGS;
pub const POLICY_MAGIC: &[u8; 8] = b"CSINPv1\0";
pub const CONSUMER_MAGIC: &[u8; 8] = b"CSINCv1\0";
pub const SELECTION_MAGIC: &[u8; 8] = b"CSINWv1\0";
pub const POLICY_DOMAIN: &[u8] = b"cellscript-instance-policy-v1\0";
pub const ARGS_SCHEMA_DOMAIN: &[u8] = b"cellscript-instance-args-schema-v1\0";
pub const IMPLEMENTATION_OFFSET: usize = 8 + POLICY_BYTES;
pub const ARGS_OFFSET: usize = IMPLEMENTATION_OFFSET + open_handle_policy::SELECTION_BYTES;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum InstanceMode {
    Exact,
    Constrained,
}

/// Constraints form an ordered, non-overlapping, complete partition of args.
/// Unconstrained bytes must be explicitly declared with `Any`; omitted bytes
/// never silently become a wildcard. All offsets are byte offsets.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ArgsConstraint {
    Bytes { offset: u16, value: Vec<u8> },
    Unsigned { offset: u16, width: u16, minimum: u64, maximum: u64 },
    Config { offset: u16, length: u16, config_offset: u16 },
    Any { offset: u16, length: u16 },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InstancePolicy {
    pub implementation_root: Hash,
    pub mode: InstanceMode,
    pub args_length: u16,
    pub config_length: u16,
    pub constraints: Vec<ArgsConstraint>,
}

/// New consumer layout, distinct from the existing exact 32-byte-root profile.
/// `subject` commits the concrete complete Script selected for this operation;
/// changing it requires the application's separately authorized transition.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ConsumerArgs {
    pub policy_root: Hash,
    pub subject: Hash,
    pub config: Vec<u8>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum InstanceError {
    Length,
    Magic,
    Reserved,
    Tag,
    Bounds,
    Partition,
    Mode,
    Schema,
    Root,
    Constraint,
    Subject,
    Implementation(PolicyError),
}
impl fmt::Display for InstanceError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "instance policy rejected at {self:?}")
    }
}
impl std::error::Error for InstanceError {}
impl From<PolicyError> for InstanceError {
    fn from(error: PolicyError) -> Self {
        Self::Implementation(error)
    }
}

impl ArgsConstraint {
    fn span(&self) -> Result<(usize, usize), InstanceError> {
        let (offset, length) = match self {
            Self::Bytes { offset, value } => (*offset, value.len()),
            Self::Unsigned { offset, width, .. } => (*offset, *width as usize),
            Self::Config { offset, length, .. } | Self::Any { offset, length } => (*offset, *length as usize),
        };
        if length == 0 || length > MAX_ARGS || offset as usize + length > MAX_ARGS {
            return Err(InstanceError::Bounds);
        }
        Ok((offset as usize, length))
    }

    fn encode(&self, config_length: usize) -> Result<[u8; CONSTRAINT_BYTES], InstanceError> {
        let (offset, length) = self.span()?;
        let mut out = [0; CONSTRAINT_BYTES];
        out[2..4].copy_from_slice(&(length as u16).to_le_bytes());
        out[4..6].copy_from_slice(&(offset as u16).to_le_bytes());
        match self {
            Self::Bytes { value, .. } => {
                if length > 32 {
                    return Err(InstanceError::Bounds);
                }
                out[16..16 + length].copy_from_slice(value);
            }
            Self::Unsigned { minimum, maximum, .. } => {
                if !matches!(length, 1 | 2 | 4 | 8) || minimum > maximum || (length < 8 && *maximum >= 1u64 << (length * 8)) {
                    return Err(InstanceError::Bounds);
                }
                out[0] = 1;
                out[16..24].copy_from_slice(&minimum.to_le_bytes());
                out[24..32].copy_from_slice(&maximum.to_le_bytes());
            }
            Self::Config { config_offset, .. } => {
                if *config_offset as usize + length > config_length {
                    return Err(InstanceError::Bounds);
                }
                out[0] = 2;
                out[6..8].copy_from_slice(&config_offset.to_le_bytes());
            }
            Self::Any { .. } => out[0] = 3,
        }
        Ok(out)
    }

    fn decode(bytes: &[u8], config_length: usize) -> Result<Self, InstanceError> {
        let length = u16_at(bytes, 2);
        let offset = u16_at(bytes, 4);
        let rule = match bytes[0] {
            0 if length <= 32 => Self::Bytes { offset, value: bytes[16..16 + length as usize].to_vec() },
            0 => return Err(InstanceError::Bounds),
            1 => Self::Unsigned { offset, width: length, minimum: u64_at(bytes, 16), maximum: u64_at(bytes, 24) },
            2 => Self::Config { offset, length, config_offset: u16_at(bytes, 6) },
            3 => Self::Any { offset, length },
            _ => return Err(InstanceError::Tag),
        };
        // Re-encoding checks every unused byte and rules out alternate forms.
        if rule.encode(config_length)?.as_slice() != bytes {
            return Err(InstanceError::Reserved);
        }
        Ok(rule)
    }

    fn check(&self, args: &[u8], config: &[u8]) -> Result<(), InstanceError> {
        let (offset, length) = self.span()?;
        let value = args.get(offset..offset + length).ok_or(InstanceError::Bounds)?;
        let matches = match self {
            Self::Bytes { value: expected, .. } => value == expected,
            Self::Unsigned { minimum, maximum, .. } => {
                let mut scalar = [0; 8];
                scalar[..length].copy_from_slice(value);
                (*minimum..=*maximum).contains(&u64::from_le_bytes(scalar))
            }
            Self::Config { config_offset, .. } => {
                let start = *config_offset as usize;
                value == config.get(start..start + length).ok_or(InstanceError::Bounds)?
            }
            Self::Any { .. } => true,
        };
        if matches {
            Ok(())
        } else {
            Err(InstanceError::Constraint)
        }
    }
}

impl InstancePolicy {
    /// Independently derive the flat args schema from the complete partition.
    /// A field is fixed bytes or an unsigned scalar; permission predicates and
    /// their values belong to the policy identity, not the layout identity.
    pub fn args_schema(&self) -> Result<Hash, InstanceError> {
        if self.constraints.len() > MAX_CONSTRAINTS || self.args_length as usize > MAX_ARGS || self.config_length as usize > MAX_CONFIG
        {
            return Err(InstanceError::Bounds);
        }
        let mut preimage = [0; ARGS_SCHEMA_DOMAIN.len() + 8 + MAX_CONSTRAINTS * 8];
        preimage[..ARGS_SCHEMA_DOMAIN.len()].copy_from_slice(ARGS_SCHEMA_DOMAIN);
        let layout = &mut preimage[ARGS_SCHEMA_DOMAIN.len()..];
        layout[..2].copy_from_slice(&self.args_length.to_le_bytes());
        layout[2] = self.constraints.len() as u8;
        let mut cursor = 0;
        for (rule, row) in self.constraints.iter().zip(layout[8..].chunks_exact_mut(8)) {
            let encoded = rule.encode(self.config_length as usize)?;
            let (offset, length) = rule.span()?;
            if offset != cursor {
                return Err(InstanceError::Partition);
            }
            row[..2].copy_from_slice(&encoded[4..6]);
            row[2..4].copy_from_slice(&encoded[2..4]);
            row[4] = u8::from(matches!(rule, ArgsConstraint::Unsigned { .. }));
            cursor += length;
        }
        if cursor != self.args_length as usize {
            return Err(InstanceError::Partition);
        }
        Ok(ckb_blake2b256(&preimage))
    }

    pub fn encode(&self) -> Result<[u8; POLICY_BYTES], InstanceError> {
        if self.args_length as usize > MAX_ARGS
            || self.config_length as usize > MAX_CONFIG
            || self.constraints.len() > MAX_CONSTRAINTS
            || self.implementation_root == [0; 32]
        {
            return Err(InstanceError::Bounds);
        }
        let mut out = [0; POLICY_BYTES];
        out[..8].copy_from_slice(POLICY_MAGIC);
        out[8] = match self.mode {
            InstanceMode::Exact => 0,
            InstanceMode::Constrained => 1,
        };
        out[9] = self.constraints.len() as u8;
        out[10..12].copy_from_slice(&self.args_length.to_le_bytes());
        out[12..14].copy_from_slice(&self.config_length.to_le_bytes());
        out[16..48].copy_from_slice(&self.implementation_root);
        out[48..80].copy_from_slice(&self.args_schema()?);
        let mut cursor = 0;
        for (rule, encoded) in self.constraints.iter().zip(out[POLICY_HEADER_BYTES..].chunks_exact_mut(CONSTRAINT_BYTES)) {
            let (offset, length) = rule.span()?;
            if offset != cursor {
                return Err(InstanceError::Partition);
            }
            if self.mode == InstanceMode::Exact && !matches!(rule, ArgsConstraint::Bytes { .. }) {
                return Err(InstanceError::Mode);
            }
            encoded.copy_from_slice(&rule.encode(self.config_length as usize)?);
            cursor += length;
        }
        if cursor != self.args_length as usize {
            return Err(InstanceError::Partition);
        }
        Ok(out)
    }

    pub fn decode(bytes: &[u8]) -> Result<Self, InstanceError> {
        if bytes.len() != POLICY_BYTES {
            return Err(InstanceError::Length);
        }
        if &bytes[..8] != POLICY_MAGIC {
            return Err(InstanceError::Magic);
        }
        let count = bytes[9] as usize;
        let config_length = u16_at(bytes, 12);
        if count > MAX_CONSTRAINTS || config_length as usize > MAX_CONFIG || u16_at(bytes, 10) as usize > MAX_ARGS {
            return Err(InstanceError::Bounds);
        }
        let mode = match bytes[8] {
            0 => InstanceMode::Exact,
            1 => InstanceMode::Constrained,
            _ => return Err(InstanceError::Tag),
        };
        let constraints = bytes[POLICY_HEADER_BYTES..POLICY_HEADER_BYTES + count * CONSTRAINT_BYTES]
            .chunks_exact(CONSTRAINT_BYTES)
            .map(|row| ArgsConstraint::decode(row, config_length as usize))
            .collect::<Result<Vec<_>, _>>()?;
        let policy =
            Self { implementation_root: hash_at(bytes, 16), mode, args_length: u16_at(bytes, 10), config_length, constraints };
        if hash_at(bytes, 48) != policy.args_schema()? {
            return Err(InstanceError::Schema);
        }
        if policy.encode()?.as_slice() != bytes {
            return Err(InstanceError::Reserved);
        }
        Ok(policy)
    }

    pub fn root(&self) -> Result<Hash, InstanceError> {
        let mut preimage = [0; POLICY_DOMAIN.len() + POLICY_BYTES];
        preimage[..POLICY_DOMAIN.len()].copy_from_slice(POLICY_DOMAIN);
        preimage[POLICY_DOMAIN.len()..].copy_from_slice(&self.encode()?);
        Ok(ckb_blake2b256(&preimage))
    }

    pub fn check_args(&self, args: &[u8], config: &[u8]) -> Result<(), InstanceError> {
        // Also validate policies constructed by Rust callers, not only decoders.
        self.encode()?;
        if args.len() != self.args_length as usize || config.len() != self.config_length as usize {
            return Err(InstanceError::Length);
        }
        for rule in &self.constraints {
            rule.check(args, config)?;
        }
        Ok(())
    }
}

impl ConsumerArgs {
    pub fn encode(&self) -> Result<[u8; CONSUMER_ARGS_BYTES], InstanceError> {
        if self.config.len() > MAX_CONFIG || self.policy_root == [0; 32] || self.subject == [0; 32] {
            return Err(InstanceError::Bounds);
        }
        let mut out = [0; CONSUMER_ARGS_BYTES];
        out[..8].copy_from_slice(CONSUMER_MAGIC);
        out[8..40].copy_from_slice(&self.policy_root);
        out[40..72].copy_from_slice(&self.subject);
        out[72..74].copy_from_slice(&(self.config.len() as u16).to_le_bytes());
        out[80..80 + self.config.len()].copy_from_slice(&self.config);
        Ok(out)
    }

    pub fn decode(bytes: &[u8]) -> Result<Self, InstanceError> {
        if bytes.len() != CONSUMER_ARGS_BYTES {
            return Err(InstanceError::Length);
        }
        if &bytes[..8] != CONSUMER_MAGIC {
            return Err(InstanceError::Magic);
        }
        let length = u16_at(bytes, 72) as usize;
        if length > MAX_CONFIG {
            return Err(InstanceError::Bounds);
        }
        let args = Self { policy_root: hash_at(bytes, 8), subject: hash_at(bytes, 40), config: bytes[80..80 + length].to_vec() };
        if args.encode()?.as_slice() != bytes {
            return Err(InstanceError::Reserved);
        }
        Ok(args)
    }
}

/// Construct the exact canonical Molecule Script. Byte length is bounded before
/// allocation; the hash binds the full code identity, hash type and actual args.
pub fn instance_script(code_hash: &Hash, hash_type: CodeHashType, args: &[u8]) -> Result<Vec<u8>, InstanceError> {
    if args.len() > MAX_ARGS {
        return Err(InstanceError::Bounds);
    }
    let mut script = Vec::with_capacity(53 + args.len());
    for word in [53 + args.len() as u32, 16, 48, 49] {
        script.extend_from_slice(&word.to_le_bytes());
    }
    script.extend_from_slice(code_hash);
    script.push(match hash_type {
        CodeHashType::Data => 0,
        CodeHashType::Type => 1,
        CodeHashType::Data1 => 2,
        CodeHashType::Data2 => 4,
    });
    script.extend_from_slice(&(args.len() as u32).to_le_bytes());
    script.extend_from_slice(args);
    Ok(script)
}

/// Encoding is not admission. Config-dependent constraints and concrete subject
/// binding are checked by `verify_selection` against separately supplied args.
pub fn encode_selection(policy: &InstancePolicy, implementation: &[u8], args: &[u8]) -> Result<[u8; SELECTION_BYTES], InstanceError> {
    let encoded_policy = policy.encode()?;
    if args.len() != policy.args_length as usize {
        return Err(InstanceError::Length);
    }
    open_handle_policy::verify_selection(&policy.implementation_root, implementation)?;
    let mut out = [0; SELECTION_BYTES];
    out[..8].copy_from_slice(SELECTION_MAGIC);
    out[8..IMPLEMENTATION_OFFSET].copy_from_slice(&encoded_policy);
    out[IMPLEMENTATION_OFFSET..ARGS_OFFSET].copy_from_slice(implementation);
    out[ARGS_OFFSET..ARGS_OFFSET + args.len()].copy_from_slice(args);
    Ok(out)
}

#[derive(Debug)]
pub struct InstanceMembership {
    policy: InstancePolicy,
    implementation: PolicyMembership,
    script: Vec<u8>,
    script_hash: Hash,
}
impl InstanceMembership {
    pub fn policy(&self) -> &InstancePolicy {
        &self.policy
    }
    pub fn implementation(&self) -> &PolicyMembership {
        &self.implementation
    }
    pub fn script(&self) -> &[u8] {
        &self.script
    }
    pub fn script_hash(&self) -> Hash {
        self.script_hash
    }
}

/// The consumer bytes must come from an authenticated transaction input Script,
/// not the selection witness. This low-level API cannot establish that origin.
pub fn verify_selection(consumer_args: &[u8], bytes: &[u8]) -> Result<InstanceMembership, InstanceError> {
    if bytes.len() != SELECTION_BYTES {
        return Err(InstanceError::Length);
    }
    if &bytes[..8] != SELECTION_MAGIC {
        return Err(InstanceError::Magic);
    }
    let consumer = ConsumerArgs::decode(consumer_args)?;
    let policy = InstancePolicy::decode(&bytes[8..IMPLEMENTATION_OFFSET])?;
    if policy.root()? != consumer.policy_root {
        return Err(InstanceError::Root);
    }
    let implementation =
        open_handle_policy::verify_selection(&policy.implementation_root, &bytes[IMPLEMENTATION_OFFSET..ARGS_OFFSET])?;
    let args = &bytes[ARGS_OFFSET..ARGS_OFFSET + policy.args_length as usize];
    if bytes[ARGS_OFFSET + args.len()..].iter().any(|byte| *byte != 0) {
        return Err(InstanceError::Reserved);
    }
    policy.check_args(args, &consumer.config)?;
    // A parameterless spawned child inherits its consumer context. It cannot
    // acquire a new Script-instance args context through this policy envelope.
    if policy.mode == InstanceMode::Constrained && implementation.header().role == ScriptRole::SpawnedVerifier {
        return Err(InstanceError::Mode);
    }
    let member = implementation.member();
    let script = instance_script(&member.code_hash, member.hash_type, args)?;
    let script_hash = ckb_blake2b256(&script);
    if script_hash != consumer.subject || (policy.mode == InstanceMode::Exact && script_hash != member.script) {
        return Err(InstanceError::Subject);
    }
    Ok(InstanceMembership { policy, implementation, script, script_hash })
}

fn u16_at(bytes: &[u8], start: usize) -> u16 {
    u16::from_le_bytes(bytes[start..start + 2].try_into().expect("bounded instance record"))
}
fn u64_at(bytes: &[u8], start: usize) -> u64 {
    u64::from_le_bytes(bytes[start..start + 8].try_into().expect("bounded instance record"))
}
fn hash_at(bytes: &[u8], start: usize) -> Hash {
    bytes[start..start + 32].try_into().expect("bounded instance record")
}

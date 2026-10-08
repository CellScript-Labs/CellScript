//! Bounded authorization-set commitments for the planned compatible-open handle.
//!
//! This is a wire/membership boundary, not artifact or compatibility admission.
//! A caller must obtain `expected_root` from its independently authorized policy;
//! taking it from the same witness would make the check self-authorizing. No
//! source syntax, CKB syscall enforcement, Registry freshness or peer execution
//! is established by this module. Fixed records follow `open_handle_policy.mol`.

use crate::ckb_blake2b256;
use std::fmt;

pub type Hash = [u8; 32];
pub const MAX_MEMBERS: usize = 32;
pub const TREE_DEPTH: usize = 5;
pub const HEADER_BYTES: usize = 188;
pub const MEMBER_BYTES: usize = 292;
pub const SELECTION_BYTES: usize = 656;
const _: () = assert!(MAX_MEMBERS == 1 << TREE_DEPTH);
const _: () = assert!(SELECTION_BYTES == 8 + HEADER_BYTES + MEMBER_BYTES + 8 + TREE_DEPTH * 32);
const HEADER_MAGIC: &[u8; 8] = b"CSOHPv1\0";
const MEMBER_MAGIC: &[u8; 8] = b"CSOHMv1\0";
const SELECTION_MAGIC: &[u8; 8] = b"CSOHWv1\0";
const ZERO: Hash = [0; 32];

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HandleClass {
    Script,
    Verifier,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ScriptRole {
    Lock,
    Type,
    SpawnedVerifier,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SelectionMode {
    Exact,
    Compatible,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MemberStatus {
    Active,
    Yanked,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CodeHashType {
    Data,
    Type,
    Data1,
    Data2,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PolicyHeader {
    pub class: HandleClass,
    pub role: ScriptRole,
    pub mode: SelectionMode,
    pub member_count: u8,
    /// Policy admission sequence, not a package SemVer or block height.
    pub sequence: u64,
    pub minimum_admission_sequence: u64,
    /// Required baseline interface contract identity, established by admission.
    pub required_interface: Hash,
    /// Nonzero only for exact mode; compatible mode uses required_interface.
    pub exact_receipt: Hash,
    pub network_genesis: Hash,
    pub target_profile: Hash,
    /// Required contract ABI, not equality of the candidate's entire metadata.
    pub runtime_abi: Hash,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PolicyMember {
    pub status: MemberStatus,
    pub hash_type: CodeHashType,
    pub admission_sequence: u64,
    pub deployment_sequence: u64,
    pub receipt: Hash,
    pub interface: Hash,
    pub artifact: Hash,
    /// Hash of the complete Script, including its concrete args.
    pub script: Hash,
    pub code_hash: Hash,
    pub code_tx_hash: Hash,
    pub deployment_line: Hash,
    pub history_tip: Hash,
    pub code_output_index: u32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PolicyError {
    Length,
    Magic,
    Tag,
    Reserved,
    Header,
    Member,
    Count,
    Duplicate,
    Conflict,
    Index,
    Root,
    Inactive,
    Sequence,
    ExactReceipt,
}
impl fmt::Display for PolicyError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "open-handle policy rejected at {self:?}")
    }
}
impl std::error::Error for PolicyError {}

impl PolicyHeader {
    fn validate(&self) -> Result<(), PolicyError> {
        if self.member_count == 0 || self.member_count as usize > MAX_MEMBERS {
            return Err(PolicyError::Count);
        }
        if self.minimum_admission_sequence > self.sequence
            || !matches!(
                (self.class, self.role),
                (HandleClass::Script, ScriptRole::Lock | ScriptRole::Type) | (HandleClass::Verifier, ScriptRole::SpawnedVerifier)
            )
            || [self.required_interface, self.network_genesis, self.target_profile, self.runtime_abi].contains(&ZERO)
            || (self.mode == SelectionMode::Exact) == (self.exact_receipt == ZERO)
        {
            return Err(PolicyError::Header);
        }
        Ok(())
    }

    pub fn encode(&self) -> Result<[u8; HEADER_BYTES], PolicyError> {
        self.validate()?;
        let mut out = [0; HEADER_BYTES];
        out[..8].copy_from_slice(HEADER_MAGIC);
        out[8] = match self.class {
            HandleClass::Script => 0,
            HandleClass::Verifier => 1,
        };
        out[9] = match self.role {
            ScriptRole::Lock => 0,
            ScriptRole::Type => 1,
            ScriptRole::SpawnedVerifier => 2,
        };
        out[10] = match self.mode {
            SelectionMode::Exact => 0,
            SelectionMode::Compatible => 1,
        };
        out[11] = self.member_count;
        out[12..20].copy_from_slice(&self.sequence.to_le_bytes());
        out[20..28].copy_from_slice(&self.minimum_admission_sequence.to_le_bytes());
        for (slot, hash) in out[28..].chunks_exact_mut(32).zip([
            self.required_interface,
            self.exact_receipt,
            self.network_genesis,
            self.target_profile,
            self.runtime_abi,
        ]) {
            slot.copy_from_slice(&hash);
        }
        Ok(out)
    }

    pub fn decode(bytes: &[u8]) -> Result<Self, PolicyError> {
        if bytes.len() != HEADER_BYTES {
            return Err(PolicyError::Length);
        }
        if &bytes[..8] != HEADER_MAGIC {
            return Err(PolicyError::Magic);
        }
        let header = Self {
            class: match bytes[8] {
                0 => HandleClass::Script,
                1 => HandleClass::Verifier,
                _ => return Err(PolicyError::Tag),
            },
            role: match bytes[9] {
                0 => ScriptRole::Lock,
                1 => ScriptRole::Type,
                2 => ScriptRole::SpawnedVerifier,
                _ => return Err(PolicyError::Tag),
            },
            mode: match bytes[10] {
                0 => SelectionMode::Exact,
                1 => SelectionMode::Compatible,
                _ => return Err(PolicyError::Tag),
            },
            member_count: bytes[11],
            sequence: u64_at(bytes, 12),
            minimum_admission_sequence: u64_at(bytes, 20),
            required_interface: hash_at(bytes, 28),
            exact_receipt: hash_at(bytes, 60),
            network_genesis: hash_at(bytes, 92),
            target_profile: hash_at(bytes, 124),
            runtime_abi: hash_at(bytes, 156),
        };
        header.validate()?;
        Ok(header)
    }
}

impl PolicyMember {
    fn validate(&self) -> Result<(), PolicyError> {
        if [self.receipt, self.interface, self.artifact, self.script, self.code_hash, self.code_tx_hash].contains(&ZERO) {
            return Err(PolicyError::Member);
        }
        match self.hash_type {
            CodeHashType::Type if self.deployment_line != ZERO && self.history_tip != ZERO => {}
            CodeHashType::Type => return Err(PolicyError::Member),
            _ if self.code_hash == self.artifact
                && self.deployment_line == ZERO
                && self.history_tip == ZERO
                && self.deployment_sequence == 0 => {}
            _ => return Err(PolicyError::Member),
        }
        Ok(())
    }

    pub fn encode(&self) -> Result<[u8; MEMBER_BYTES], PolicyError> {
        self.validate()?;
        let mut out = [0; MEMBER_BYTES];
        out[..8].copy_from_slice(MEMBER_MAGIC);
        out[8] = match self.status {
            MemberStatus::Active => 0,
            MemberStatus::Yanked => 1,
        };
        out[9] = match self.hash_type {
            CodeHashType::Data => 0,
            CodeHashType::Type => 1,
            CodeHashType::Data1 => 2,
            CodeHashType::Data2 => 4,
        };
        out[16..24].copy_from_slice(&self.admission_sequence.to_le_bytes());
        out[24..32].copy_from_slice(&self.deployment_sequence.to_le_bytes());
        for (slot, hash) in out[32..288].chunks_exact_mut(32).zip([
            self.receipt,
            self.interface,
            self.artifact,
            self.script,
            self.code_hash,
            self.code_tx_hash,
            self.deployment_line,
            self.history_tip,
        ]) {
            slot.copy_from_slice(&hash);
        }
        out[288..292].copy_from_slice(&self.code_output_index.to_le_bytes());
        Ok(out)
    }

    pub fn decode(bytes: &[u8]) -> Result<Self, PolicyError> {
        if bytes.len() != MEMBER_BYTES {
            return Err(PolicyError::Length);
        }
        if &bytes[..8] != MEMBER_MAGIC {
            return Err(PolicyError::Magic);
        }
        if bytes[10..16] != [0; 6] {
            return Err(PolicyError::Reserved);
        }
        let member = Self {
            status: match bytes[8] {
                0 => MemberStatus::Active,
                1 => MemberStatus::Yanked,
                _ => return Err(PolicyError::Tag),
            },
            hash_type: match bytes[9] {
                0 => CodeHashType::Data,
                1 => CodeHashType::Type,
                2 => CodeHashType::Data1,
                4 => CodeHashType::Data2,
                _ => return Err(PolicyError::Tag),
            },
            admission_sequence: u64_at(bytes, 16),
            deployment_sequence: u64_at(bytes, 24),
            receipt: hash_at(bytes, 32),
            interface: hash_at(bytes, 64),
            artifact: hash_at(bytes, 96),
            script: hash_at(bytes, 128),
            code_hash: hash_at(bytes, 160),
            code_tx_hash: hash_at(bytes, 192),
            deployment_line: hash_at(bytes, 224),
            history_tip: hash_at(bytes, 256),
            code_output_index: u32::from_le_bytes(bytes[288..292].try_into().expect("bounded member")),
        };
        member.validate()?;
        Ok(member)
    }
}

/// A canonical bounded tree. Constructing it commits supplied records; it does
/// not verify their artifact receipts, compatibility, history or authorization.
#[derive(Debug)]
pub struct AuthorizationSet {
    header: PolicyHeader,
    members: Vec<PolicyMember>,
    tree: [Hash; MAX_MEMBERS * 2],
    root: Hash,
}
impl AuthorizationSet {
    pub fn new(header: PolicyHeader, members: &[PolicyMember]) -> Result<Self, PolicyError> {
        let encoded_header = header.encode()?;
        if members.len() != header.member_count as usize {
            return Err(PolicyError::Count);
        }
        // Bound checked before allocation or member traversal.
        let mut members = members.to_vec();
        members.sort_by_key(|member| member.receipt);
        if members.windows(2).any(|pair| pair[0].receipt == pair[1].receipt) {
            return Err(PolicyError::Duplicate);
        }
        if header.mode == SelectionMode::Exact && !members.iter().any(|member| member.receipt == header.exact_receipt) {
            return Err(PolicyError::ExactReceipt);
        }
        let mut tree = [ZERO; MAX_MEMBERS * 2];
        for (index, member) in members.iter().enumerate() {
            member.validate()?;
            if member.admission_sequence > header.sequence {
                return Err(PolicyError::Sequence);
            }
            for other in &members[..index] {
                if (member.code_tx_hash == other.code_tx_hash
                    && member.code_output_index == other.code_output_index
                    && member.artifact != other.artifact)
                    || (member.deployment_line != ZERO
                        && member.deployment_line == other.deployment_line
                        && member.deployment_sequence == other.deployment_sequence)
                {
                    return Err(PolicyError::Conflict);
                }
            }
            tree[MAX_MEMBERS + index] = leaf(index as u8, &member.encode()?);
        }
        for index in members.len()..MAX_MEMBERS {
            tree[MAX_MEMBERS + index] = empty_leaf(index as u8);
        }
        for index in (1..MAX_MEMBERS).rev() {
            tree[index] = node(&tree[index * 2], &tree[index * 2 + 1]);
        }
        let root = policy_root(&encoded_header, &tree[1]);
        Ok(Self { header, members, tree, root })
    }
    pub fn root(&self) -> Hash {
        self.root
    }
    pub fn header(&self) -> &PolicyHeader {
        &self.header
    }
    pub fn members(&self) -> &[PolicyMember] {
        &self.members
    }

    /// Emit one fixed-width membership witness, including exactly five siblings.
    /// Inactive/history members may be represented; `verify_selection` rejects
    /// selecting them under the header's active/exact/minimum-sequence policy.
    pub fn selection(&self, receipt: &Hash) -> Result<[u8; SELECTION_BYTES], PolicyError> {
        let index = self.members.binary_search_by_key(receipt, |member| member.receipt).map_err(|_| PolicyError::Index)?;
        let mut out = [0; SELECTION_BYTES];
        out[..8].copy_from_slice(SELECTION_MAGIC);
        out[8..196].copy_from_slice(&self.header.encode()?);
        out[196..488].copy_from_slice(&self.members[index].encode()?);
        out[488] = index as u8;
        let mut position = MAX_MEMBERS + index;
        for sibling in out[496..].chunks_exact_mut(32) {
            sibling.copy_from_slice(&self.tree[position ^ 1]);
            position /= 2;
        }
        Ok(out)
    }
}

/// Proof of membership under a caller-supplied expected root only. It is not a
/// verified compatible artifact or a source-level ScriptHandle<I>.
#[derive(Debug)]
pub struct PolicyMembership {
    root: Hash,
    header: PolicyHeader,
    member: PolicyMember,
    index: u8,
}
impl PolicyMembership {
    pub fn root(&self) -> Hash {
        self.root
    }
    pub fn header(&self) -> &PolicyHeader {
        &self.header
    }
    pub fn member(&self) -> &PolicyMember {
        &self.member
    }
    pub fn index(&self) -> u8 {
        self.index
    }
}

pub fn verify_selection(expected_root: &Hash, bytes: &[u8]) -> Result<PolicyMembership, PolicyError> {
    if bytes.len() != SELECTION_BYTES {
        return Err(PolicyError::Length);
    }
    if &bytes[..8] != SELECTION_MAGIC {
        return Err(PolicyError::Magic);
    }
    if bytes[489..496] != [0; 7] {
        return Err(PolicyError::Reserved);
    }
    let header = PolicyHeader::decode(&bytes[8..196])?;
    let member = PolicyMember::decode(&bytes[196..488])?;
    let index = bytes[488];
    if index >= header.member_count {
        return Err(PolicyError::Index);
    }
    let mut root = leaf(index, &member.encode()?);
    for (level, sibling) in bytes[496..].chunks_exact(32).enumerate() {
        let sibling: Hash = sibling.try_into().expect("fixed sibling");
        root = if (index as usize >> level) & 1 == 0 { node(&root, &sibling) } else { node(&sibling, &root) };
    }
    if policy_root(&header.encode()?, &root) != *expected_root {
        return Err(PolicyError::Root);
    }
    if member.status != MemberStatus::Active {
        return Err(PolicyError::Inactive);
    }
    if member.admission_sequence < header.minimum_admission_sequence || member.admission_sequence > header.sequence {
        return Err(PolicyError::Sequence);
    }
    if header.mode == SelectionMode::Exact && member.receipt != header.exact_receipt {
        return Err(PolicyError::ExactReceipt);
    }
    Ok(PolicyMembership { root: *expected_root, header, member, index })
}

fn hash_at(bytes: &[u8], start: usize) -> Hash {
    bytes[start..start + 32].try_into().expect("bounded record")
}
fn u64_at(bytes: &[u8], start: usize) -> u64 {
    u64::from_le_bytes(bytes[start..start + 8].try_into().expect("bounded record"))
}
fn domain_hash(domain: &[u8], parts: &[&[u8]]) -> Hash {
    let mut bytes = Vec::with_capacity(domain.len() + parts.iter().map(|part| part.len()).sum::<usize>());
    bytes.extend_from_slice(domain);
    for part in parts {
        bytes.extend_from_slice(part);
    }
    ckb_blake2b256(&bytes)
}
fn leaf(index: u8, member: &[u8; MEMBER_BYTES]) -> Hash {
    domain_hash(b"cellscript-open-handle-member-v1\0", &[&[index], member])
}
fn empty_leaf(index: u8) -> Hash {
    domain_hash(b"cellscript-open-handle-empty-v1\0", &[&[index]])
}
fn node(left: &Hash, right: &Hash) -> Hash {
    domain_hash(b"cellscript-open-handle-node-v1\0", &[left, right])
}
fn policy_root(header: &[u8; HEADER_BYTES], root: &Hash) -> Hash {
    domain_hash(b"cellscript-open-handle-policy-v1\0", &[header, root])
}

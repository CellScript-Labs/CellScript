//! Fixed sibling comparison operands. This binds literal bytes or a typed
//! local slot; arbitrary scalar/fixed-value producer equivalence is separate.
use super::*;
use crate::{TypedSemanticConstant, TypedSemanticEntry, TypedSemanticOperand};

#[derive(PartialEq, Eq)]
pub(super) struct Expected {
    width: u32,
    scalar: bool,
    source: Option<u64>,
    inline_bytes: bool,
    constant: Vec<u8>,
}

impl Expected {
    pub(super) fn failure_code(&self) -> i32 {
        if self.scalar {
            3
        } else {
            32
        }
    }

    pub(super) fn new(entry: &TypedSemanticEntry, operand: &TypedSemanticOperand, ty: &str) -> Result<Self, CheckerError> {
        let ty = crate::checker::canonical_abi_type(ty);
        let width = match ty.as_str() {
            "u8" => 1,
            "u16" => 2,
            "u32" => 4,
            "u64" => 8,
            "u128" => 16,
            "hash" | "address" => 32,
            _ => ty
                .strip_prefix("[u8;")
                .and_then(|ty| ty.strip_suffix(']'))
                .and_then(|value| value.parse::<u32>().ok())
                .filter(|width| (1..=256).contains(width))
                .ok_or_else(|| invalid("unbounded fixed output field"))?,
        };
        let scalar = matches!(ty.as_str(), "u8" | "u16" | "u32" | "u64");
        let source = operand
            .local
            .map(|id| {
                entry
                    .locals
                    .iter()
                    .find(|local| local.id == id)
                    .map(|local| local.source_id)
                    .ok_or_else(|| invalid("fixed output has no expected local"))
            })
            .transpose()?;
        let constant = match &operand.constant {
            Some(
                TypedSemanticConstant::U8(value)
                | TypedSemanticConstant::U16(value)
                | TypedSemanticConstant::U32(value)
                | TypedSemanticConstant::U64(value)
                | TypedSemanticConstant::U128(value),
            ) => {
                let value = value.parse::<u128>().map_err(|_| invalid("invalid output integer"))?;
                if width > 16 || (width < 16 && value >> (width * 8) != 0) {
                    return Err(invalid("output integer exceeds its width"));
                }
                value.to_le_bytes()[..width as usize].to_vec()
            }
            Some(TypedSemanticConstant::Hash(value) | TypedSemanticConstant::Address(value)) => {
                // Typed constants use canonical unprefixed lowercase hex.
                let hex = value;
                if hex.len() != 64 || !hex.bytes().all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte)) {
                    return Err(invalid("invalid output fixed byte width"));
                }
                (0..32)
                    .map(|index| {
                        u8::from_str_radix(&hex[index * 2..index * 2 + 2], 16).map_err(|_| invalid("invalid output fixed byte digit"))
                    })
                    .collect::<Result<Vec<_>, _>>()?
            }
            Some(TypedSemanticConstant::Array(values)) => values
                .iter()
                .map(|value| match value {
                    TypedSemanticConstant::U8(value) => value.parse::<u8>().map_err(|_| invalid("invalid output array byte")),
                    _ => Err(invalid("output array has a non-byte element")),
                })
                .collect::<Result<Vec<_>, _>>()?,
            None if source.is_some() => Vec::new(),
            _ => return Err(invalid("unsupported fixed output operand")),
        };
        if operand.constant.is_some() && constant.len() != width as usize {
            return Err(invalid("output constant width differs"));
        }
        let inline_bytes = !scalar && width <= 8 && operand.local.is_some_and(|id| inline_parameter(entry, id));
        Ok(Self { width, scalar, source, inline_bytes, constant })
    }

    pub(super) fn check(&self, c: &mut Decoder<'_>, frame: u32, end: u64, failures: &mut Vec<u64>) -> Result<(), CheckerError> {
        c.constant(13, u64::from(self.width))?;
        failures.push(cells::branch(c, 1, 12, 13)?);
        c.add(10, 29, 30)?;
        if self.scalar {
            if let Some(source) = self.source {
                uses::expect_stack(c, 13, slot(source)?, false, frame)?;
            } else {
                let mut bytes = [0u8; 8];
                bytes[..self.constant.len()].copy_from_slice(&self.constant);
                c.constant(13, u64::from_le_bytes(bytes))?;
            }
            c.li(5, 0)?;
            for byte in 0..self.width {
                c.immediate(3, 4, 7, 10, byte as i32)?;
                if byte != 0 {
                    c.immediate(0x13, 1, 7, 7, byte as i32 * 8)?;
                }
                c.register(6, 0, 5, 5, 7)?;
            }
            failures.push(cells::branch(c, 1, 5, 13)?);
            if c.jump()? != end {
                return Err(invalid("fixed scalar comparison completion changed"));
            }
        } else if let Some(source) = self.source {
            if self.inline_bytes {
                cells::pointer(c, 11, slot(source)?)?;
            } else {
                uses::expect_stack(c, 11, slot(source)?, false, frame)?;
            }
            byte_loop(c, end, failures)?;
        } else {
            for (offset, byte) in self.constant.iter().enumerate() {
                c.immediate(3, 4, 5, 10, offset as i32)?;
                c.constant(6, u64::from(*byte))?;
                failures.push(cells::branch(c, 1, 5, 6)?);
            }
            if c.jump()? != end {
                return Err(invalid("fixed literal comparison completion changed"));
            }
        }
        Ok(())
    }
}

fn inline_parameter(entry: &TypedSemanticEntry, mut local: u32) -> bool {
    let mut visited = BTreeSet::new();
    while visited.insert(local) {
        if entry.params.iter().any(|param| param.binding_id == local) {
            return true;
        }
        let mut definitions =
            entry.blocks.iter().flat_map(|block| &block.operations).filter(|operation| operation.destinations.contains(&local));
        let Some(operation) = definitions.next() else { return false };
        if definitions.next().is_some() || operation.opcode != "move" {
            return false;
        }
        let [operand] = operation.operands.as_slice() else { return false };
        let Some(source) = operand.local else { return false };
        local = source;
    }
    false
}

fn slot(source: u64) -> Result<u32, CheckerError> {
    source.checked_mul(8).and_then(|slot| u32::try_from(slot).ok()).ok_or_else(|| invalid("fixed output source slot overflows"))
}

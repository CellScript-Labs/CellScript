//! Resolved call provenance is retained independently of assembly label text.
//! Only lowering of a known intrinsic may create a runtime target. Ordinary
//! source functions never acquire a runtime contract by spelling their name.
use cellscript_artifact_checker::runtime_contract::RuntimeHelper;
use std::fmt;
use std::hash::{Hash, Hasher};

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum IrCallTarget {
    Function(String),
    Runtime(RuntimeHelper),
    /// An internal lowering request named no registered contract. This is an
    /// explicit compilation failure, never an ordinary-function fallback.
    InvalidRuntime(String),
}

impl IrCallTarget {
    pub fn runtime_symbol(symbol: &str) -> Self {
        RuntimeHelper::from_symbol(symbol).map(Self::Runtime).unwrap_or_else(|| Self::InvalidRuntime(symbol.into()))
    }

    pub fn as_str(&self) -> &str {
        match self {
            Self::Function(symbol) | Self::InvalidRuntime(symbol) => symbol,
            Self::Runtime(helper) => helper.symbol(),
        }
    }

    pub const fn runtime_helper(&self) -> Option<RuntimeHelper> {
        match self {
            Self::Runtime(helper) => Some(*helper),
            Self::Function(_) | Self::InvalidRuntime(_) => None,
        }
    }

    pub fn runtime_dependencies(&self) -> &'static [RuntimeHelper] {
        match self {
            Self::Runtime(RuntimeHelper::Bip340RequireSignature | RuntimeHelper::Bip340RequireSignatureFromCellDep) => &[
                RuntimeHelper::CkbPipe,
                RuntimeHelper::CkbPipeWrite,
                RuntimeHelper::CkbClose,
                RuntimeHelper::CkbSpawnWithFd1,
                RuntimeHelper::CkbWait,
            ],
            Self::Runtime(RuntimeHelper::ZkRequireTransition) => {
                &[RuntimeHelper::CkbPipe, RuntimeHelper::CkbClose, RuntimeHelper::CkbSpawnWithFd1, RuntimeHelper::CkbWait]
            }
            Self::Function(_) | Self::Runtime(_) | Self::InvalidRuntime(_) => &[],
        }
    }

    pub fn function_symbol(&self) -> Option<&str> {
        match self {
            Self::Function(symbol) => Some(symbol),
            Self::Runtime(_) | Self::InvalidRuntime(_) => None,
        }
    }
}

impl From<String> for IrCallTarget {
    fn from(symbol: String) -> Self {
        Self::Function(symbol)
    }
}

impl From<RuntimeHelper> for IrCallTarget {
    fn from(helper: RuntimeHelper) -> Self {
        Self::Runtime(helper)
    }
}

impl fmt::Display for IrCallTarget {
    fn fmt(&self, output: &mut fmt::Formatter<'_>) -> fmt::Result {
        output.write_str(self.as_str())
    }
}

impl Ord for IrCallTarget {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        let class = |target: &Self| match target {
            Self::Function(_) => 0u8,
            Self::Runtime(_) => 1,
            Self::InvalidRuntime(_) => 2,
        };
        (class(self), self.as_str()).cmp(&(class(other), other.as_str()))
    }
}
impl PartialOrd for IrCallTarget {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        Some(self.cmp(other))
    }
}

impl Hash for IrCallTarget {
    fn hash<H: Hasher>(&self, state: &mut H) {
        std::mem::discriminant(self).hash(state);
        self.as_str().hash(state);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ordinary_labels_cannot_acquire_runtime_authority() {
        let label = RuntimeHelper::Bip340RequireSignature.symbol();
        let function = IrCallTarget::from(label.to_owned());
        let runtime = IrCallTarget::Runtime(RuntimeHelper::Bip340RequireSignature);
        assert_eq!(function.as_str(), runtime.as_str());
        assert_ne!(function, runtime);
        assert_eq!(function.runtime_helper(), None);
        assert_eq!(function.function_symbol(), Some(label));
        assert_eq!(runtime.function_symbol(), None);
        assert!(matches!(IrCallTarget::runtime_symbol("__missing_contract"), IrCallTarget::InvalidRuntime(_)));
    }
}

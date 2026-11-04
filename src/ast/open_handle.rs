//! Closed handle class and a retained module-interface designation. Source
//! checking resolves the module; this description grants no runtime authority.
use super::{Type, TypeArgument};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum OpenHandleClass {
    Script,
    Verifier,
    Instance,
}

impl OpenHandleClass {
    pub fn from_constructor(name: &str) -> Option<Self> {
        use crate::script_handle_contract::*;
        Some(match name {
            OPEN_SCRIPT_HANDLE_TYPE => Self::Script,
            OPEN_VERIFIER_HANDLE_TYPE => Self::Verifier,
            INSTANCE_SCRIPT_HANDLE_TYPE => Self::Instance,
            _ => return None,
        })
    }

    pub fn constructor(self) -> &'static str {
        use crate::script_handle_contract::*;
        match self {
            Self::Script => OPEN_SCRIPT_HANDLE_TYPE,
            Self::Verifier => OPEN_VERIFIER_HANDLE_TYPE,
            Self::Instance => INSTANCE_SCRIPT_HANDLE_TYPE,
        }
    }

    /// Current closed wire extent, independently checked at artifact admission.
    pub(crate) fn encoded_width(self) -> usize {
        match self {
            Self::Script | Self::Verifier => crate::script_handle_contract::OPEN_HANDLE_SELECTION_BYTES,
            Self::Instance => crate::script_handle_contract::INSTANCE_SCRIPT_HANDLE_BYTES,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct OpenHandleType {
    pub class: OpenHandleClass,
    pub interface: String,
}

impl OpenHandleType {
    pub(crate) fn has_bounded_module_path(&self) -> bool {
        self.interface.len() <= 512
            && self.interface.split("::").all(|part| {
                let mut characters = part.chars();
                characters.next().is_some_and(|first| first == '_' || first.is_alphabetic())
                    && characters.all(|character| character == '_' || character.is_alphanumeric())
            })
    }

    pub fn source_text(&self) -> String {
        format!("{}<{}>", self.class.constructor(), self.interface)
    }
}

impl Type {
    pub fn as_open_handle(&self) -> Option<OpenHandleType> {
        let Self::Applied(application) = self else { return None };
        let class = OpenHandleClass::from_constructor(&application.constructor)?;
        let [TypeArgument::Type(Self::Named(interface))] = application.arguments.as_slice() else { return None };
        Some(OpenHandleType { class, interface: interface.clone() })
    }
}

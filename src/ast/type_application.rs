//! Syntactic applications keep argument kinds until semantic admission. Numeric
//! arguments are never nominal declarations. AST and IR share the tree shape,
//! while their resolution and executable-family admission remain phase-owned.
use super::Type;

/// Source output for a nominal constructor expression or aggregate target.
/// Arguments remain nodes until specialization replaces them with a label.
pub(crate) fn render_type_designation(constructor: &str, arguments: &[Type]) -> String {
    if arguments.is_empty() {
        constructor.to_string()
    } else {
        format!("{constructor}<{}>", arguments.iter().map(Type::source_text).collect::<Vec<_>>().join(", "))
    }
}

impl Type {
    pub fn commitment(value: Type) -> Self {
        Self::applied("Commitment", vec![TypeArgument::Type(value)])
    }

    pub fn commitment_value_type(&self) -> Option<&Type> {
        self.application()?.unary_type("Commitment")
    }

    pub fn opening_value_type(&self) -> Option<&Type> {
        self.application()?.unary_type("Opening")
    }

    pub fn vector(element: Type) -> Self {
        Self::applied("Vec", vec![TypeArgument::Type(element)])
    }

    pub fn vector_element(&self) -> Option<&Type> {
        self.application()?.unary_type("Vec")
    }

    /// Source spelling for diagnostics, formatting and editor presentation.
    /// This is an output operation, not a nominal identity or an ABI reader.
    pub fn source_text(&self) -> String {
        match self {
            Self::U8 => "u8".into(),
            Self::U16 => "u16".into(),
            Self::U32 => "u32".into(),
            Self::I32 => "i32".into(),
            Self::U64 => "u64".into(),
            Self::U128 => "u128".into(),
            Self::Bool => "bool".into(),
            Self::Unit => "()".into(),
            Self::Address => "Address".into(),
            Self::Hash => "Hash".into(),
            Self::Named(name) => name.clone(),
            Self::Applied(application) => application.render(", ", Self::source_text),
            Self::Selection(selection) => selection.encode(),
            Self::Array(inner, length) => format!("[{}; {}]", inner.source_text(), length),
            Self::Tuple(items) => format!("({})", items.iter().map(Self::source_text).collect::<Vec<_>>().join(", ")),
            Self::Ref(inner) => format!("&{}", inner.source_text()),
            Self::MutRef(inner) => format!("&mut {}", inner.source_text()),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct TypeApplication<T> {
    pub constructor: String,
    pub arguments: Vec<TypeArgument<T>>,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum TypeArgument<T> {
    Type(T),
    UnsignedConstant(u128),
}

impl<T> TypeArgument<T> {
    pub fn as_type(&self) -> Option<&T> {
        match self {
            Self::Type(ty) => Some(ty),
            Self::UnsignedConstant(_) => None,
        }
    }

    pub fn as_unsigned(&self) -> Option<u128> {
        match self {
            Self::UnsignedConstant(value) => Some(*value),
            Self::Type(_) => None,
        }
    }
}

impl<T> TypeApplication<T> {
    pub fn unary_type(&self, constructor: &str) -> Option<&T> {
        match self.arguments.as_slice() {
            [TypeArgument::Type(ty)] if self.constructor == constructor => Some(ty),
            _ => None,
        }
    }

    pub fn new(constructor: impl Into<String>, arguments: Vec<TypeArgument<T>>) -> Self {
        Self { constructor: constructor.into(), arguments }
    }

    pub fn map_types<U>(&self, mut map: impl FnMut(&T) -> U) -> TypeApplication<U> {
        TypeApplication {
            constructor: self.constructor.clone(),
            arguments: self
                .arguments
                .iter()
                .map(|arg| match arg {
                    TypeArgument::Type(ty) => TypeArgument::Type(map(ty)),
                    TypeArgument::UnsignedConstant(value) => TypeArgument::UnsignedConstant(*value),
                })
                .collect(),
        }
    }

    pub fn try_map_types<U, E>(&self, mut map: impl FnMut(&T) -> Result<U, E>) -> Result<TypeApplication<U>, E> {
        Ok(TypeApplication {
            constructor: self.constructor.clone(),
            arguments: self
                .arguments
                .iter()
                .map(|arg| {
                    Ok(match arg {
                        TypeArgument::Type(ty) => TypeArgument::Type(map(ty)?),
                        TypeArgument::UnsignedConstant(value) => TypeArgument::UnsignedConstant(*value),
                    })
                })
                .collect::<Result<_, E>>()?,
        })
    }

    /// Rendering is an output boundary. Callers choose their existing source,
    /// identity or wire contract, including separators and child spellings.
    pub fn render(&self, separator: &str, mut render_type: impl FnMut(&T) -> String) -> String {
        format!(
            "{}<{}>",
            self.constructor,
            self.arguments
                .iter()
                .map(|arg| match arg {
                    TypeArgument::Type(ty) => render_type(ty),
                    TypeArgument::UnsignedConstant(value) => value.to_string(),
                })
                .collect::<Vec<_>>()
                .join(separator)
        )
    }
}

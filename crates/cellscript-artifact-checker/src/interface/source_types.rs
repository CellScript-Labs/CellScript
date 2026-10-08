//! Bounded source-type grammar shared by declaration qualification and receipts.
//! Parsing a spelling does not establish that a nominal declaration exists.
use crate::{CheckerError, CheckerRejectionCode, NominalDeclarationScope};
use std::collections::{BTreeMap, BTreeSet};

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum SourceType {
    Named(String, Vec<SourceArgument>),
    Array(Box<Self>, u64),
    Tuple(Vec<Self>),
    Reference { mutable: bool, inner: Box<Self> },
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum SourceArgument {
    Type(SourceType),
    Count(u64),
}

fn invalid() -> CheckerError {
    CheckerError::new(
        CheckerRejectionCode::V2419TypedSemanticsInvalid,
        "nominal declaration has invalid or unbounded source type syntax",
    )
}

struct Parser<'a> {
    text: &'a str,
    offset: usize,
    nodes: usize,
}

impl Parser<'_> {
    fn whitespace(&mut self) {
        while let Some(character) = self.text[self.offset..].chars().next().filter(|character| character.is_whitespace()) {
            self.offset += character.len_utf8();
        }
    }

    fn take(&mut self, token: &str) -> bool {
        self.whitespace();
        if self.text[self.offset..].starts_with(token) {
            self.offset += token.len();
            true
        } else {
            false
        }
    }

    fn identifier(&mut self) -> Result<&str, CheckerError> {
        self.whitespace();
        let start = self.offset;
        for (index, character) in self.text[start..].chars().enumerate() {
            if character != '_' && !character.is_alphabetic() && !(index > 0 && character.is_alphanumeric()) {
                break;
            }
            self.offset += character.len_utf8();
        }
        if start == self.offset {
            Err(invalid())
        } else {
            Ok(&self.text[start..self.offset])
        }
    }

    fn count(&mut self) -> Result<u64, CheckerError> {
        self.whitespace();
        let start = self.offset;
        while self.text.as_bytes().get(self.offset).is_some_and(u8::is_ascii_digit) {
            self.offset += 1;
        }
        self.text[start..self.offset].parse().map_err(|_| invalid())
    }

    fn ty(&mut self, depth: usize) -> Result<SourceType, CheckerError> {
        self.nodes += 1;
        if depth > 16 || self.nodes > 4096 {
            return Err(invalid());
        }
        if self.take("&") {
            self.whitespace();
            let suffix = &self.text[self.offset..];
            let mutable = suffix == "mut" || suffix.strip_prefix("mut").is_some_and(|rest| rest.starts_with(char::is_whitespace));
            if mutable {
                self.offset += 3;
            }
            return Ok(SourceType::Reference { mutable, inner: Box::new(self.ty(depth + 1)?) });
        }
        if self.take("[") {
            let inner = self.ty(depth + 1)?;
            if !self.take(";") {
                return Err(invalid());
            }
            let count = self.count()?;
            if !self.take("]") {
                return Err(invalid());
            }
            return Ok(SourceType::Array(Box::new(inner), count));
        }
        if self.take("(") {
            let mut fields = Vec::new();
            if !self.take(")") {
                loop {
                    fields.push(self.ty(depth + 1)?);
                    if fields.len() > 64 {
                        return Err(invalid());
                    }
                    if self.take(")") {
                        break;
                    }
                    if !self.take(",") {
                        return Err(invalid());
                    }
                }
            }
            return Ok(SourceType::Tuple(fields));
        }
        let mut name = self.identifier()?.to_string();
        while self.take("::") {
            name.push_str("::");
            name.push_str(self.identifier()?);
        }
        let mut arguments = Vec::new();
        if self.take("<") {
            loop {
                self.whitespace();
                arguments.push(if self.text.as_bytes().get(self.offset).is_some_and(u8::is_ascii_digit) {
                    SourceArgument::Count(self.count()?)
                } else {
                    SourceArgument::Type(self.ty(depth + 1)?)
                });
                if arguments.len() > 8 {
                    return Err(invalid());
                }
                if self.take(">") {
                    break;
                }
                if !self.take(",") {
                    return Err(invalid());
                }
            }
        }
        Ok(SourceType::Named(name, arguments))
    }
}

impl SourceType {
    pub(crate) fn parse(value: &str) -> Result<Self, CheckerError> {
        if value.is_empty() || value.len() > 512 {
            return Err(invalid());
        }
        let mut parser = Parser { text: value, offset: 0, nodes: 0 };
        let result = parser.ty(0)?;
        parser.whitespace();
        if parser.offset != value.len() {
            return Err(invalid());
        }
        Ok(result)
    }

    fn qualify(&mut self, bindings: &BTreeMap<&str, String>, parameters: &BTreeSet<&str>) {
        match self {
            Self::Named(name, arguments) => {
                if !parameters.contains(name.as_str()) {
                    if let Some(qualified) = bindings.get(name.as_str()) {
                        *name = qualified.clone();
                    } else {
                        *name = crate::generic_projection::canonical_type(name);
                    }
                }
                for argument in arguments {
                    if let SourceArgument::Type(ty) = argument {
                        ty.qualify(bindings, parameters);
                    }
                }
            }
            Self::Array(inner, _) | Self::Reference { inner, .. } => inner.qualify(bindings, parameters),
            Self::Tuple(fields) => fields.iter_mut().for_each(|field| field.qualify(bindings, parameters)),
        }
    }

    pub(crate) fn spelling(&self) -> String {
        match self {
            Self::Named(name, arguments) if arguments.is_empty() => name.clone(),
            Self::Named(name, arguments) => format!(
                "{}<{}>",
                name,
                arguments
                    .iter()
                    .map(|argument| match argument {
                        SourceArgument::Type(ty) => ty.spelling(),
                        SourceArgument::Count(count) => count.to_string(),
                    })
                    .collect::<Vec<_>>()
                    .join(",")
            ),
            Self::Array(inner, count) => format!("[{};{}]", inner.spelling(), count),
            Self::Tuple(fields) if fields.is_empty() => "unit".into(),
            Self::Tuple(fields) => format!("({})", fields.iter().map(Self::spelling).collect::<Vec<_>>().join(",")),
            Self::Reference { mutable, inner } => format!("&{}{}", if *mutable { "mut " } else { "" }, inner.spelling()),
        }
    }
}

pub(super) fn qualify(value: &str, scope: &NominalDeclarationScope, parameters: &[&str]) -> Result<String, CheckerError> {
    let mut ty = SourceType::parse(value)?;
    let bindings = scope
        .bindings
        .iter()
        .map(|binding| (binding.local_name.as_str(), format!("{}::{}", binding.owner_module, binding.source_name)))
        .collect();
    ty.qualify(&bindings, &parameters.iter().copied().collect());
    let result = ty.spelling();
    if result.len() > 512 {
        Err(invalid())
    } else {
        Ok(result)
    }
}

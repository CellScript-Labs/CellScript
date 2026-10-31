//! Compilation-scoped admission of nested descriptions. Source spelling is
//! retained for diagnostics/rendering; record equality uses its declaration.
use super::*;
use std::cell::RefCell;
use std::collections::HashMap;
use std::hash::{Hash, Hasher};
use std::sync::Arc;

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub(crate) enum FixedFieldType {
    U8,
    U16,
    U32,
    U64,
    U128,
    Hash,
    Address,
    Bytes(u16),
}

impl FixedFieldType {
    pub(super) fn from_source(ty: &Type) -> Option<Self> {
        Some(match ty {
            Type::U8 => Self::U8,
            Type::U16 => Self::U16,
            Type::U32 => Self::U32,
            Type::U64 => Self::U64,
            Type::U128 => Self::U128,
            Type::Hash => Self::Hash,
            Type::Address => Self::Address,
            Type::Array(inner, length) if **inner == Type::U8 && (1..=256).contains(length) => Self::Bytes(*length as u16),
            _ => return None,
        })
    }

    pub(crate) fn width(&self) -> u16 {
        match self {
            Self::U8 => 1,
            Self::U16 => 2,
            Self::U32 => 4,
            Self::U64 => 8,
            Self::U128 => 16,
            Self::Hash | Self::Address => 32,
            Self::Bytes(length) => *length,
        }
    }

    pub(crate) fn source_type(&self) -> Type {
        match self {
            Self::U8 => Type::U8,
            Self::U16 => Type::U16,
            Self::U32 => Type::U32,
            Self::U64 => Type::U64,
            Self::U128 => Type::U128,
            Self::Hash => Type::Hash,
            Self::Address => Type::Address,
            Self::Bytes(length) => Type::Array(Box::new(Type::U8), usize::from(*length)),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub(crate) struct RecordLayout {
    pub fixed_fields: Vec<(String, FixedFieldType)>,
    pub payload_field: String,
}

impl RecordLayout {
    pub(super) fn admit(fields: &[(String, Type)]) -> Result<Self> {
        let invalid = || {
            CompileError::new(
            "a nested record requires 1..=8 fixed identity fields (unsigned integers, Hash, Address or byte arrays of 1..=256 bytes) followed by one Vec<u8> payload; recursive, optional and Cell fields are not admitted",
            Span::default(),
        )
        };
        if !(2..=9).contains(&fields.len()) {
            return Err(invalid());
        }
        let (payload_field, payload_type) = fields.last().ok_or_else(invalid)?;
        if !matches!(payload_type, Type::Named(name) if is_bytes_element(name)) {
            return Err(invalid());
        }
        let fixed_fields = fields[..fields.len() - 1]
            .iter()
            .map(|(name, ty)| Ok((name.clone(), FixedFieldType::from_source(ty).ok_or_else(invalid)?)))
            .collect::<Result<Vec<_>>>()?;
        Ok(Self { fixed_fields, payload_field: payload_field.clone() })
    }

    pub(crate) fn shape(&self) -> ElementShape {
        ElementShape::Record { fixed_field_widths: self.fixed_fields.iter().map(|(_, ty)| ty.width()).collect() }
    }

    pub(crate) fn field_type(&self, name: &str, max_payload_bytes: u32) -> Option<Type> {
        if name == self.payload_field {
            Some(Type::Named(bytes_view_type(max_payload_bytes)))
        } else {
            self.fixed_fields.iter().find(|(field, _)| field == name).map(|(_, ty)| ty.source_type())
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
struct RecordDeclaration {
    owner: String,
    /// Concrete specialization name, including its existing generic identity.
    name: String,
    layout: RecordLayout,
}

#[derive(Debug, Clone)]
pub(crate) struct RecordRef {
    declaration: Arc<RecordDeclaration>,
    spelling: String,
}

impl PartialEq for RecordRef {
    fn eq(&self, other: &Self) -> bool {
        self.declaration == other.declaration
    }
}
impl Eq for RecordRef {}
impl Hash for RecordRef {
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.declaration.hash(state);
    }
}

impl RecordRef {
    pub(crate) fn layout(&self) -> &RecordLayout {
        &self.declaration.layout
    }

    pub(crate) fn spelling(&self) -> &str {
        &self.spelling
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub(crate) enum Element {
    Bytes,
    Record(RecordRef),
}

impl Element {
    pub(crate) fn shape(&self) -> ElementShape {
        match self {
            Self::Bytes => ElementShape::Bytes {},
            Self::Record(record) => record.layout().shape(),
        }
    }
}

impl std::fmt::Display for Element {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Bytes => formatter.write_str("Vec<u8>"),
            Self::Record(record) => formatter.write_str(record.spelling()),
        }
    }
}

/// One owner and one immutable resolver snapshot per instance. Never share this
/// cache between compilation/document revisions. Source, IR and runtime proof
/// retain separate responsibilities; admission here does not prove decoding.
#[derive(Default)]
pub(crate) struct Descriptions {
    parsed: RefCell<HashMap<String, Result<Option<Arc<NestedType>>>>>,
    records: RefCell<HashMap<String, Result<RecordRef>>>,
    resolved: RefCell<HashMap<NestedType, Arc<NestedType<Element>>>>,
}

impl Descriptions {
    #[cfg(test)]
    pub(crate) fn retained_counts(&self) -> (usize, usize, usize) {
        (self.parsed.borrow().len(), self.records.borrow().len(), self.resolved.borrow().len())
    }

    pub(crate) fn parse(&self, name: &str) -> Result<Option<Arc<NestedType>>> {
        if let Some(parsed) = self.parsed.borrow().get(name) {
            return parsed.clone();
        }
        let parsed = super::parse(name).map(|parsed| parsed.map(Arc::new));
        self.parsed.borrow_mut().insert(name.to_string(), parsed.clone());
        parsed
    }

    pub(crate) fn record(
        &self,
        spelling: &str,
        lookup: impl FnOnce() -> Result<(String, String, Vec<(String, Type)>)>,
    ) -> Result<RecordRef> {
        if let Some(record) = self.records.borrow().get(spelling) {
            return record.clone();
        }
        let record = lookup().and_then(|(owner, name, fields)| {
            Ok(RecordRef {
                declaration: Arc::new(RecordDeclaration { owner, name, layout: RecordLayout::admit(&fields)? }),
                spelling: spelling.to_string(),
            })
        });
        self.records.borrow_mut().insert(spelling.to_string(), record.clone());
        record
    }

    pub(crate) fn resolve(&self, nested: &NestedType, record: impl Fn(&str) -> Result<RecordRef>) -> Result<Arc<NestedType<Element>>> {
        if let Some(resolved) = self.resolved.borrow().get(nested) {
            return Ok(resolved.clone());
        }
        let element = |name: &str| {
            if is_bytes_element(name) {
                Ok(Element::Bytes)
            } else {
                record(name).map(Element::Record)
            }
        };
        let resolved = Arc::new(match nested {
            NestedType::Values(values) => NestedType::Values(ValuesType {
                element: element(&values.element)?,
                max_items: values.max_items,
                max_payload_bytes: values.max_payload_bytes,
                max_total_bytes: values.max_total_bytes,
            }),
            NestedType::BytesView { max_payload_bytes } => NestedType::BytesView { max_payload_bytes: *max_payload_bytes },
            NestedType::RecordView { element: name, max_payload_bytes } => {
                // A record view cannot denote a byte-list element.
                NestedType::RecordView { element: Element::Record(record(name)?), max_payload_bytes: *max_payload_bytes }
            }
        });
        self.resolved.borrow_mut().insert(nested.clone(), resolved.clone());
        Ok(resolved)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::cell::Cell;
    use std::collections::hash_map::DefaultHasher;

    fn record(cache: &Descriptions, spelling: &str, owner: &str, name: &str, key: Type) -> RecordRef {
        cache
            .record(spelling, || {
                Ok((owner.into(), name.into(), vec![("key".into(), key), ("data".into(), Type::Named("Vec<u8>".into()))]))
            })
            .unwrap()
    }

    #[test]
    fn scoped_descriptions_reuse_parsing_resolution_and_record_admission() {
        let cache = Descriptions::default();
        let text = "NestedValues<Entry, 4, 8, 128>";
        let parsed = cache.parse(text).unwrap().unwrap();
        let lookups = Cell::new(0);
        let resolve = |element: &str| {
            cache.record(element, || {
                lookups.set(lookups.get() + 1);
                Ok(("owner".into(), "Entry".into(), vec![("key".into(), Type::U64), ("data".into(), Type::Named("Vec<u8>".into()))]))
            })
        };
        let first = cache.resolve(&parsed, resolve).unwrap();
        for _ in 0..128 {
            assert!(Arc::ptr_eq(&parsed, &cache.parse(text).unwrap().unwrap()));
            assert!(Arc::ptr_eq(&first, &cache.resolve(&parsed, resolve).unwrap()));
        }
        let item = cache.parse("NestedRecordView<Entry, 8>").unwrap().unwrap();
        cache.resolve(&item, resolve).unwrap();
        assert_eq!(lookups.get(), 1);
        assert_eq!(cache.parsed.borrow().len(), 2);
        assert_eq!(cache.records.borrow().len(), 1);
        assert_eq!(cache.resolved.borrow().len(), 2);
        assert_eq!(first.canonical_text(), text);
        assert!(cache.parse("NestedValues<Entry, 4, 8, 4097>").is_err());
        assert!(cache.parse("NestedValues<Entry, 4, 8, 4097>").is_err());
        assert!(cache.parse("Ordinary").unwrap().is_none());
    }

    #[test]
    fn record_identity_ignores_import_spelling_but_retains_owner_specialization_and_layout() {
        let cache = Descriptions::default();
        let first = record(&cache, "Left", "left", "Entry", Type::U64);
        let alias = record(&cache, "Alias", "left", "Entry", Type::U64);
        assert_eq!(first, alias);
        let hash = |record: &RecordRef| {
            let mut state = DefaultHasher::new();
            record.hash(&mut state);
            state.finish()
        };
        assert_eq!(hash(&first), hash(&alias));
        assert_ne!(first.spelling(), alias.spelling());
        assert_ne!(first, record(&cache, "Right", "right", "Entry", Type::U64));
        assert_ne!(first, record(&cache, "Specialized", "left", "Entry__mono__753634", Type::U64));
        let revised = Descriptions::default();
        assert_ne!(first, record(&revised, "Left", "left", "Entry", Type::U8));
        assert_eq!(record(&revised, "Left", "left", "Entry", Type::U8).layout().fixed_fields[0].1.width(), 1);
    }

    #[test]
    fn canonical_nested_rendering_preserves_source_contracts_and_round_trips() {
        for element in ["Vec<u8>", "Entry", "Imported", "Entry__mono__753634", "模块::记录"] {
            for items in [0, 1, MAX_ITEMS] {
                for payload in [0, 1, MAX_PAYLOAD_BYTES] {
                    for total in [4, 128, MAX_TOTAL_BYTES] {
                        let expected = format!("NestedValues<{element}, {items}, {payload}, {total}>");
                        let spaced = format!("NestedValues< {element} , {items} , {payload} , {total} >");
                        let parsed = parse(&spaced).unwrap().unwrap();
                        assert_eq!(parsed.canonical_text(), expected);
                        assert_eq!(parse(&parsed.canonical_text()).unwrap().unwrap(), parsed);
                        assert_eq!(
                            ValuesType {
                                element: element.to_string(),
                                max_items: items,
                                max_payload_bytes: payload,
                                max_total_bytes: total
                            }
                            .source_type(),
                            Type::Named(expected)
                        );
                    }
                    for view in [format!("NestedBytesView<{payload}>"), format!("NestedRecordView<{element}, {payload}>")] {
                        let parsed = parse(&view).unwrap().unwrap();
                        assert_eq!(parsed.canonical_text(), view);
                        assert_eq!(parse(&parsed.canonical_text()).unwrap().unwrap(), parsed);
                    }
                }
            }
        }
    }

    #[test]
    fn admission_preserves_bitpattern_complete_field_types_and_rejects_malformed_records() {
        for key in
            [Type::U8, Type::U16, Type::U32, Type::U64, Type::U128, Type::Hash, Type::Address, Type::Array(Box::new(Type::U8), 256)]
        {
            let cache = Descriptions::default();
            let record = record(&cache, "Entry", "owner", "Entry", key.clone());
            assert_eq!(record.layout().field_type("key", 8), Some(key));
            assert_eq!(record.layout().field_type("data", 8), Some(Type::Named("NestedBytesView<8>".into())));
            assert!(record.layout().field_type("missing", 8).is_none());
        }
        for key in [
            Type::Bool,
            Type::I32,
            Type::Named("Enum".into()),
            Type::Array(Box::new(Type::U8), 0),
            Type::Array(Box::new(Type::U8), 257),
        ] {
            assert!(RecordLayout::admit(&[("key".into(), key), ("data".into(), Type::Named("Vec<u8>".into()))]).is_err());
        }
        assert!(RecordLayout::admit(&[]).is_err());
        assert!(RecordLayout::admit(&[("key".into(), Type::U64)]).is_err());
        assert!(RecordLayout::admit(&[("key".into(), Type::U64), ("data".into(), Type::U64)]).is_err());
    }
}

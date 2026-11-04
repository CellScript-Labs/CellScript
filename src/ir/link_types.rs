//! Link nominal labels by retained declaration identities. Equal local names
//! from different modules cannot share a schema/layout dictionary entry.
use super::*;

struct Bindings {
    names: BTreeMap<String, String>,
}

fn invalid(message: impl Into<String>) -> CompileError {
    CompileError::without_span(message).with_code(crate::error::DiagnosticCode::E2111)
}

impl Bindings {
    fn name(&self, name: &mut String) {
        if let Some(target) = self.names.get(name) {
            *name = target.clone();
        }
    }

    fn source(&self, ty: &mut Type) -> Result<()> {
        if let Some(inner) = ty.commitment_value_type().or_else(|| ty.opening_value_type()) {
            let mut projected = inner.clone();
            self.source(&mut projected)?;
            if projected != *inner {
                return Err(invalid("conflicting imported V0 packed type bindings require distinct or qualified source domain names"));
            }
            return Ok(());
        }
        match ty {
            Type::Named(name) => self.name(name),
            Type::Array(inner, _) | Type::Ref(inner) | Type::MutRef(inner) => self.source(inner)?,
            Type::Tuple(items) => {
                for ty in items {
                    self.source(ty)?;
                }
            }
            Type::Applied(application) => {
                for argument in &mut application.arguments {
                    if let TypeArgument::Type(ty) = argument {
                        self.source(ty)?;
                    }
                }
            }
            Type::Selection(selection) => {
                for name in &mut selection.schemas {
                    self.name(name);
                }
            }
            Type::U8
            | Type::U16
            | Type::U32
            | Type::I32
            | Type::U64
            | Type::U128
            | Type::Bool
            | Type::Unit
            | Type::Address
            | Type::Hash => {}
        }
        Ok(())
    }

    fn ty(&self, ty: &mut IrType) -> Result<()> {
        match ty {
            IrType::Named(name) => self.name(name),
            IrType::Array(inner, _) | IrType::Ref(inner) | IrType::MutRef(inner) => self.ty(inner)?,
            IrType::Tuple(items) => {
                for ty in items {
                    self.ty(ty)?;
                }
            }
            IrType::Vector(inner) => {
                if let Some(inner) = inner {
                    self.ty(inner)?;
                }
            }
            IrType::Context(view) => match view {
                ContextView::Input(inner) | ContextView::Output(inner) | ContextView::CellData(inner) => self.ty(inner)?,
                ContextView::CellDep | ContextView::HeaderDep | ContextView::WitnessArgs | ContextView::Script => {}
            },
            IrType::Bounded(collection) => self.ty(&mut collection.element_type)?,
            IrType::Applied(application) => {
                for argument in &mut application.arguments {
                    if let TypeArgument::Type(ty) = argument {
                        self.ty(ty)?;
                    }
                }
            }
            IrType::Selection(selection) => {
                for name in &mut selection.schemas {
                    self.name(name);
                }
            }
            IrType::Commitment(inner) | IrType::Opening(inner) => {
                let mut projected = inner.as_ref().clone();
                self.ty(&mut projected)?;
                if projected != **inner {
                    return Err(invalid(
                        "conflicting imported V0 packed type bindings require distinct or qualified source domain names",
                    ));
                }
            }
            IrType::Nested(view) => {
                *view = view.with_record_spelling(|name| self.names.get(name).cloned().unwrap_or_else(|| name.into()));
            }
            IrType::U8
            | IrType::U16
            | IrType::U32
            | IrType::I32
            | IrType::U64
            | IrType::U128
            | IrType::Bool
            | IrType::Unit
            | IrType::Address
            | IrType::Hash
            | IrType::OpenHandle(_)
            | IrType::WitnessBytes(_) => {}
        }
        Ok(())
    }
}

fn operand(value: &mut IrOperand, visit: &mut impl FnMut(&mut IrType) -> Result<()>) -> Result<()> {
    if let IrOperand::Var(value) = value {
        visit(&mut value.ty)?;
    }
    Ok(())
}

fn instruction(instruction: &mut IrInstruction, bindings: &Bindings) -> Result<()> {
    let mut visit = |ty: &mut IrType| bindings.ty(ty);
    use IrInstruction::*;
    match instruction {
        AllocatedValue(operation) => {
            if let Some(dest) = &mut operation.dest {
                visit(&mut dest.ty)?;
            }
            for value in operation.owner.iter_mut().chain(&mut operation.args) {
                operand(value, &mut visit)?;
            }
            for member in &mut operation.declaration.members {
                use crate::ast::ValueCollectionKind;
                match &mut member.kind {
                    ValueCollectionKind::Vec(ty) | ValueCollectionKind::OrderedSet(ty) => bindings.source(ty)?,
                    ValueCollectionKind::OrderedMap { key, value } => {
                        bindings.source(key)?;
                        bindings.source(value)?;
                    }
                }
            }
        }
        LoadConst { dest, .. } | LoadVar { dest, .. } => visit(&mut dest.ty)?,
        StoreVar { src, .. } => operand(src, &mut visit)?,
        Binary { dest, left, right, .. } => {
            visit(&mut dest.ty)?;
            operand(left, &mut visit)?;
            operand(right, &mut visit)?;
        }
        Unary { dest, operand: value, .. }
        | FieldAccess { dest, obj: value, .. }
        | Length { dest, operand: value }
        | TypeHash { dest, operand: value }
        | Move { dest, src: value }
        | Settle { dest, operand: value }
        | Claim { dest, receipt: value } => {
            visit(&mut dest.ty)?;
            operand(value, &mut visit)?;
        }
        Index { dest, arr, idx } => {
            visit(&mut dest.ty)?;
            operand(arr, &mut visit)?;
            operand(idx, &mut visit)?;
        }
        CollectionNew { dest, ty, capacity } => {
            visit(&mut dest.ty)?;
            visit(ty)?;
            if let Some(value) = capacity {
                operand(value, &mut visit)?;
            }
        }
        CollectionCapacity { dest, collection } | CollectionPop { dest, collection } => {
            visit(&mut dest.ty)?;
            operand(collection, &mut visit)?;
        }
        CollectionContains { dest, collection, value } => {
            visit(&mut dest.ty)?;
            operand(collection, &mut visit)?;
            operand(value, &mut visit)?;
        }
        CollectionRemove { dest, collection, index } => {
            visit(&mut dest.ty)?;
            operand(collection, &mut visit)?;
            operand(index, &mut visit)?;
        }
        CollectionPush { collection, value }
        | CollectionExtend { collection, slice: value }
        | CollectionTruncate { collection, len: value } => {
            operand(collection, &mut visit)?;
            operand(value, &mut visit)?;
        }
        CollectionClear { collection } | CollectionReverse { collection } => operand(collection, &mut visit)?,
        CollectionSwap { collection, left, right } => {
            operand(collection, &mut visit)?;
            operand(left, &mut visit)?;
            operand(right, &mut visit)?;
        }
        CollectionInsert { collection, index, value } | CollectionSet { collection, index, value } => {
            operand(collection, &mut visit)?;
            operand(index, &mut visit)?;
            operand(value, &mut visit)?;
        }
        BoundedCellLoad { dest, found, index, element_type, .. } => {
            visit(&mut dest.ty)?;
            visit(&mut found.ty)?;
            bindings.name(element_type);
            operand(index, &mut visit)?;
        }
        BoundedPlanLoad { dest, found, plan, index, element_type, .. } => {
            visit(&mut dest.ty)?;
            visit(&mut found.ty)?;
            bindings.name(element_type);
            operand(plan, &mut visit)?;
            operand(index, &mut visit)?;
        }
        BoundedOutputVerify { index, pattern, .. } => {
            operand(index, &mut visit)?;
            bindings.name(&mut pattern.ty);
            for (_, value) in &mut pattern.fields {
                operand(value, &mut visit)?;
            }
            if let Some(value) = &mut pattern.lock {
                operand(value, &mut visit)?;
            }
        }
        BoundedOutputEnd { index } => operand(index, &mut visit)?,
        VerifyExactCellRoles { dest, contract } => {
            if let Some(dest) = dest {
                visit(&mut dest.ty)?;
            }
            for role in &mut contract.roles {
                bindings.name(&mut role.type_name);
            }
        }
        ReadExactCellRole { dest, selection, index, .. } => {
            visit(&mut dest.ty)?;
            operand(selection, &mut visit)?;
            if let Some(value) = index {
                operand(value, &mut visit)?;
            }
        }
        Call { dest, args, .. } => {
            if let Some(dest) = dest {
                visit(&mut dest.ty)?;
            }
            for value in args {
                operand(value, &mut visit)?;
            }
        }
        ReadRef { dest, ty } => {
            visit(&mut dest.ty)?;
            bindings.name(ty);
        }
        Tuple { dest, fields } => {
            visit(&mut dest.ty)?;
            for value in fields {
                operand(value, &mut visit)?;
            }
        }
        EnumConstruct { dest, enum_name, fields, .. } => {
            visit(&mut dest.ty)?;
            bindings.name(enum_name);
            for value in fields {
                operand(value, &mut visit)?;
            }
        }
        EnumTag { dest, operand: value, enum_name } | EnumPayload { dest, operand: value, enum_name, .. } => {
            visit(&mut dest.ty)?;
            bindings.name(enum_name);
            operand(value, &mut visit)?;
        }
        Consume { operand: value } | Destroy { operand: value, .. } => operand(value, &mut visit)?,
        Transfer { dest, operand: value, to } => {
            visit(&mut dest.ty)?;
            operand(value, &mut visit)?;
            operand(to, &mut visit)?;
        }
        Create { dest, pattern } | CreateUnique { dest, pattern, .. } => {
            visit(&mut dest.ty)?;
            bindings.name(&mut pattern.ty);
            for (_, value) in &mut pattern.fields {
                operand(value, &mut visit)?;
            }
            if let Some(value) = &mut pattern.lock {
                operand(value, &mut visit)?;
            }
        }
        ReplaceUnique { dest, operand: value, pattern, .. } => {
            visit(&mut dest.ty)?;
            operand(value, &mut visit)?;
            bindings.name(&mut pattern.ty);
            for (_, value) in &mut pattern.fields {
                operand(value, &mut visit)?;
            }
            if let Some(value) = &mut pattern.lock {
                operand(value, &mut visit)?;
            }
        }
        CellMetadataEquality { left, right, .. } => {
            operand(left, &mut visit)?;
            operand(right, &mut visit)?;
        }
    }
    Ok(())
}

impl Bindings {
    fn params(&self, params: &mut [IrParam]) -> Result<()> {
        for param in params {
            self.ty(&mut param.ty)?;
            self.ty(&mut param.binding.ty)?;
        }
        Ok(())
    }

    fn definition(&self, definition: &mut IrTypeDef) -> Result<()> {
        self.name(&mut definition.name);
        for field in &mut definition.fields {
            self.ty(&mut field.ty)?;
        }
        if let Some(ty) = &mut definition.claim_output {
            self.ty(ty)?;
        }
        Ok(())
    }

    fn pattern(&self, pattern: &mut CreatePattern) -> Result<()> {
        self.name(&mut pattern.ty);
        for (_, value) in &mut pattern.fields {
            operand(value, &mut |ty| self.ty(ty))?;
        }
        if let Some(value) = &mut pattern.lock {
            operand(value, &mut |ty| self.ty(ty))?;
        }
        Ok(())
    }

    fn body(&self, body: &mut IrBody) -> Result<()> {
        for binding in &mut body.cell_bindings {
            self.name(&mut binding.ty);
        }
        for pattern in body.consume_set.iter_mut().chain(&mut body.read_refs) {
            for (_, value) in &mut pattern.fields {
                operand(value, &mut |ty| self.ty(ty))?;
            }
        }
        for pattern in &mut body.create_set {
            self.pattern(pattern)?;
        }
        for pattern in &mut body.mutate_set {
            self.name(&mut pattern.ty);
            for transition in &mut pattern.transitions {
                operand(&mut transition.operand, &mut |ty| self.ty(ty))?;
            }
        }
        for intent in &mut body.write_intents {
            self.name(&mut intent.ty);
        }
        for operation in &mut body.bounded_collection_ops {
            self.ty(&mut operation.collection_type)?;
            self.ty(&mut operation.element_type)?;
            if let Some(name) = &mut operation.output_type {
                self.name(name);
            }
            if let Some(template) = &mut operation.create_template {
                self.name(&mut template.output_type);
            }
        }
        for region in &mut body.borrow_regions {
            self.name(&mut region.root_type);
            self.name(&mut region.view_type);
            self.ty(&mut region.view_binding.ty)?;
        }
        for block in &mut body.blocks {
            for value in &mut block.instructions {
                instruction(value, self)?;
            }
            match &mut block.terminator {
                IrTerminator::Return(Some(value)) | IrTerminator::Branch { cond: value, .. } => operand(value, &mut |ty| self.ty(ty))?,
                IrTerminator::Return(None) | IrTerminator::Jump(_) => {}
            }
        }
        Ok(())
    }
}

fn rekey<V: PartialEq>(values: BTreeMap<String, V>, names: &Bindings) -> Result<BTreeMap<String, V>> {
    let mut result = BTreeMap::new();
    for (mut key, value) in values {
        names.name(&mut key);
        if result.get(&key).is_some_and(|previous| previous != &value) {
            return Err(invalid("linked nominal aliases have conflicting retained identities"));
        }
        result.insert(key, value);
    }
    Ok(result)
}

/// Source-local names are retained unless the linked artifact already binds
/// that exact label to another declaration. New labels encode owner coordinates;
/// the symbol's argument suffix remains the existing retained-argument encoder.
pub(super) fn prepare(existing: &IrModule, incoming: &mut IrModule) -> Result<()> {
    let mut names = BTreeMap::new();
    for (name, origin) in &incoming.source_type_origins {
        let Some(previous) = existing.source_type_origins.get(name) else { continue };
        if previous == origin {
            continue;
        }
        let Some(origin) = origin else {
            return Err(invalid("linked type has an ambiguous source declaration"));
        };
        let target = if let Some(contract) = incoming.generic_contracts.get(name) {
            let prefix = format!("__cellscript_type_{}_{}", hex::encode(contract.module.as_bytes()), contract.template);
            // Re-encode from retained arguments. Never recover them from a label.
            crate::generics::monomorph_name(&prefix, &contract.arguments, &contract.specialization_origins, Span::default())?
        } else {
            format!("{}::{}", origin.module, origin.name)
        };
        if existing.source_type_origins.get(&target).is_some_and(|previous| previous.as_ref() != Some(origin)) {
            return Err(invalid("linked nominal coordinate has conflicting owners"));
        }
        names.insert(name.clone(), target);
    }
    if names.is_empty() {
        return Ok(());
    }
    let bindings = Bindings { names };
    for item in &mut incoming.items {
        match item {
            IrItem::TypeDef(definition) => bindings.definition(definition)?,
            IrItem::Action(action) => {
                bindings.params(&mut action.params)?;
                if let Some(ty) = &mut action.return_type {
                    bindings.ty(ty)?;
                }
                for edge in &mut action.state_transition_edges {
                    bindings.name(&mut edge.type_name);
                }
                bindings.body(&mut action.body)?;
            }
            IrItem::Function(function) => {
                bindings.params(&mut function.params)?;
                if let Some(ty) = &mut function.return_type {
                    bindings.ty(ty)?;
                }
                bindings.body(&mut function.body)?;
            }
            IrItem::Lock(lock) => {
                bindings.params(&mut lock.params)?;
                bindings.body(&mut lock.body)?;
            }
            IrItem::Invariant(invariant) => {
                for target in &mut invariant.reads {
                    if let Some(name) = &mut target.type_name {
                        bindings.name(name);
                    }
                    for ty in &mut target.type_args {
                        bindings.source(ty)?;
                    }
                }
                for aggregate in &mut invariant.aggregates {
                    if let Some(name) = &mut aggregate.target.type_name {
                        bindings.name(name);
                    }
                    for ty in &mut aggregate.target.type_args {
                        bindings.source(ty)?;
                    }
                    if let Some(target) = &mut aggregate.rhs {
                        if let Some(name) = &mut target.type_name {
                            bindings.name(name);
                        }
                        for ty in &mut target.type_args {
                            bindings.source(ty)?;
                        }
                    }
                }
            }
        }
    }
    for definition in &mut incoming.external_type_defs {
        bindings.definition(definition)?;
    }
    for abi in &mut incoming.external_callable_abis {
        bindings.params(&mut abi.params)?;
    }
    incoming.source_type_origins = rekey(std::mem::take(&mut incoming.source_type_origins), &bindings)?;
    incoming.nominal_identities = rekey(std::mem::take(&mut incoming.nominal_identities), &bindings)?;
    for contract in incoming.generic_contracts.values_mut() {
        contract.type_dependencies =
            contract.type_dependencies.iter().map(|name| bindings.names.get(name).cloned().unwrap_or_else(|| name.clone())).collect();
    }
    let contracts = std::mem::take(&mut incoming.generic_contracts);
    incoming.generic_contracts = contracts
        .into_iter()
        .map(|(mut name, contract)| {
            bindings.name(&mut name);
            (name, contract)
        })
        .collect();
    incoming.enum_fixed_sizes = std::mem::take(&mut incoming.enum_fixed_sizes)
        .into_iter()
        .map(|(mut name, size)| {
            bindings.name(&mut name);
            (name, size)
        })
        .collect();
    incoming.enum_layouts = std::mem::take(&mut incoming.enum_layouts)
        .into_iter()
        .map(|(mut name, mut layout)| {
            bindings.name(&mut name);
            bindings.name(&mut layout.name);
            for field in layout.variants.iter_mut().flat_map(|variant| &mut variant.fields) {
                bindings.ty(&mut field.ty)?;
            }
            Ok((name, layout))
        })
        .collect::<Result<_>>()?;
    let mut retained = BTreeSet::new();
    incoming.external_type_defs.retain(|definition| retained.insert(definition.name.clone()));
    Ok(())
}

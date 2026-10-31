//! Resolved source and builtin call checking. Ownership, borrowing and
//! context rules remain owned by the type checker.
use super::*;

impl TypeChecker<'_> {
    pub(super) fn infer_call_type(
        &mut self,
        env: &mut TypeEnv,
        call: &CallExpr,
        arg_types: &[Type],
        source_signature: Option<FunctionSignature>,
    ) -> Result<Type> {
        if let Some(signature) = source_signature {
            let Expr::Identifier(name) = call.func.as_ref() else {
                return Err(CompileError::new("source callable binding requires a resolved identifier", call.span));
            };
            if !call.type_args.is_empty() {
                return Err(CompileError::new(format!("function '{}' does not accept type arguments", name), call.span));
            }
            self.validate_call_allowed(name, signature.kind, call.span)?;
            self.validate_borrow_call(name, signature.kind, &signature.params, arg_types, call)?;
            self.validate_call_args(name, &signature.params, arg_types, &call.args, call.span)?;
            return Ok(signature.return_type.unwrap_or(Type::Unit));
        }
        match call.func.as_ref() {
            Expr::Identifier(name) => {
                if name == "spawn" {
                    return Err(crate::error::bare_spawn_error(call.span));
                }
                if let Some(error) = crate::legacy_bytes::removed_call_error(name, call.span) {
                    return Err(error);
                }
                if let Some(view_type) = self.infer_transaction_view_constructor(name, call, arg_types)? {
                    return Ok(view_type);
                }
                if let Some(view_type) = Self::witness_bytes_view_type(name, call, arg_types)? {
                    return Ok(view_type);
                }
                if name == crate::commitment_contract::COMMIT_FUNCTION {
                    if !call.type_args.is_empty() {
                        return Err(CompileError::new("commitment::commit infers T and does not accept type arguments", call.span));
                    }
                    self.validate_builtin_arity(name, 1, arg_types, call.span)?;
                    self.validate_committed_value_type(&arg_types[0], call.span)?;
                    return Ok(Type::Named(crate::commitment_contract::commitment_type(&type_repr(&arg_types[0]))));
                }
                if !call.type_args.is_empty() {
                    return Err(CompileError::new(format!("function '{}' does not accept type arguments", name), call.span));
                }
                if let Some((prefix, suffix)) = name.rsplit_once("::") {
                    self.reject_borrow_view_in_builtin_call(name, call)?;
                    return Ok(match (prefix, suffix) {
                        ("env", "current_timepoint") => {
                            self.validate_builtin_arity(name, 0, arg_types, call.span)?;
                            Type::U64
                        }
                        ("env", "block_number") if self.current_validity => {
                            self.validate_builtin_arity(name, 0, arg_types, call.span)?;
                            Type::U64
                        }
                        ("script", "hash_type_data" | "hash_type_type" | "hash_type_data1" | "hash_type_data2") => {
                            self.validate_builtin_arity(name, 0, arg_types, call.span)?;
                            Type::U64
                        }
                        ("VerificationKeyCommitment", "from_bytes") => {
                            self.validate_builtin_arity(name, 1, arg_types, call.span)?;
                            if !matches!(&call.args[0], Expr::ByteString(bytes) if bytes.len() == 32) {
                                return Err(CompileError::new(
                                    "VerificationKeyCommitment::from_bytes requires a literal 32-byte VK data hash",
                                    call.span,
                                ));
                            }
                            Type::Named("VerificationKeyCommitment".into())
                        }
                        ("Hash", "from_bytes") => {
                            self.validate_builtin_arity(name, 1, arg_types, call.span)?;
                            if !Self::is_hash_bytes_type(&arg_types[0]) {
                                return Err(CompileError::new("Hash::from_bytes expects exactly 32 bytes ([u8; 32])", call.span));
                            }
                            Type::Hash
                        }
                        ("Hash", "from_sighash_all") => {
                            self.validate_builtin_arity(name, 1, arg_types, call.span)?;
                            if arg_types[0] != Type::Named(CKB_SIGHASH_ALL_DIGEST_TYPE.to_string()) {
                                return Err(CompileError::new("Hash::from_sighash_all expects a SighashAllDigest", call.span));
                            }
                            Type::Hash
                        }
                        ("script", "args_empty") => {
                            self.validate_builtin_arity(name, 0, arg_types, call.span)?;
                            Type::Named(CKB_SCRIPT_ARGS_TYPE.to_string())
                        }
                        ("script", "args") => {
                            self.validate_builtin_arity(name, 1, arg_types, call.span)?;
                            if !Self::is_script_args_payload_type(&arg_types[0]) {
                                return Err(CompileError::new("script::args expects fixed bytes ([u8; N]) or Hash input", call.span));
                            }
                            Type::Named(CKB_SCRIPT_ARGS_TYPE.to_string())
                        }
                        ("script", "new") => {
                            self.validate_builtin_arity(name, 3, arg_types, call.span)?;
                            if arg_types[0] != Type::Hash || arg_types[1] != Type::U64 || !Self::is_script_args_type(&arg_types[2]) {
                                return Err(CompileError::new(
                                    "script::new expects (code_hash: Hash, hash_type: u64, args: ScriptArgs)",
                                    call.span,
                                ));
                            }
                            if let Expr::Integer(value) = &call.args[1] {
                                Self::validate_script_hash_type_literal(*value, call.span)?;
                            }
                            Type::Named(CKB_SCRIPT_VALUE_TYPE.to_string())
                        }
                        ("script", "hash") => {
                            self.validate_builtin_arity(name, 1, arg_types, call.span)?;
                            if !Self::is_script_value_type(&arg_types[0]) {
                                return Err(CompileError::new(
                                    "script::hash expects a Script constructed with script::new",
                                    call.span,
                                ));
                            }
                            Type::Named(CKB_SCRIPT_HASH_TYPE.to_string())
                        }
                        ("script", "require_cell_lock_matches" | "require_cell_type_matches") => {
                            self.validate_builtin_arity(name, 2, arg_types, call.span)?;
                            if !Self::is_cell_view_type(&arg_types[0]) || !Self::is_script_value_type(&arg_types[1]) {
                                return Err(CompileError::new(
                                    format!("{} expects (source_view: u64, expected_script: Script)", name),
                                    call.span,
                                ));
                            }
                            Type::Unit
                        }
                        ("env", "sighash_all") => {
                            self.validate_builtin_arity(name, 1, arg_types, call.span)?;
                            if !Self::is_cell_view_type(&arg_types[0]) {
                                return Err(CompileError::new(
                                    "env::sighash_all expects a source view returned by source::*",
                                    call.span,
                                ));
                            }
                            Type::Hash
                        }
                        ("env", "sighash_all_zero_lock") => {
                            self.validate_builtin_arity(name, 4, arg_types, call.span)?;
                            if arg_types.iter().any(|ty| *ty != Type::U64) {
                                return Err(CompileError::new(
                                    "env::sighash_all_zero_lock expects four u64 integer literals: max_group_inputs, max_inputs, max_extra_witnesses, max_witness_bytes",
                                    call.span,
                                ));
                            }
                            let mut limits = [0u128; 4];
                            for (index, (arg, (label, maximum))) in call
                                .args
                                .iter()
                                .zip([
                                    ("max_group_inputs", 64u128),
                                    ("max_inputs", 256u128),
                                    ("max_extra_witnesses", 64u128),
                                    ("max_witness_bytes", u128::from(CKB_WITNESS_BYTES_MAX)),
                                ])
                                .enumerate()
                            {
                                let Expr::Integer(value) = arg else {
                                    return Err(CompileError::new(
                                        format!("env::sighash_all_zero_lock {label} must be an integer literal"),
                                        call.span,
                                    ));
                                };
                                let minimum = if index == 2 { 0 } else { 1 };
                                if *value < minimum || *value > maximum {
                                    return Err(CompileError::new(
                                        format!("env::sighash_all_zero_lock {label} must be in {minimum}..={maximum}"),
                                        call.span,
                                    ));
                                }
                                limits[index] = *value;
                            }
                            if limits[0] > limits[1] {
                                return Err(CompileError::new(
                                    "env::sighash_all_zero_lock max_group_inputs must not exceed max_inputs",
                                    call.span,
                                ));
                            }
                            Type::Named(CKB_SIGHASH_ALL_DIGEST_TYPE.to_string())
                        }
                        (
                            "ckb",
                            "header_epoch_number"
                            | "header_epoch_start_block_number"
                            | "header_epoch_length"
                            | "input_since"
                            | "input_since_raw"
                            | "current_role",
                        ) => {
                            self.validate_builtin_arity(name, 0, arg_types, call.span)?;
                            Type::U64
                        }
                        ("ckb", "exec_cell_dep_u8_args") => {
                            self.validate_builtin_arity(name, 6, arg_types, call.span)?;
                            if arg_types.iter().any(|ty| *ty != Type::U64) {
                                return Err(CompileError::new(
                                    "ckb::exec_cell_dep_u8_args expects six u64 values: CellDep index, argc, and four byte arguments",
                                    call.span,
                                ));
                            }
                            Type::Unit
                        }
                        ("ckb", "trusted_exec_cell_dep_u8_args") => {
                            self.validate_builtin_arity(name, 7, arg_types, call.span)?;
                            if arg_types[0] != Type::U64
                                || arg_types[1] != Type::Hash
                                || arg_types[2..].iter().any(|ty| *ty != Type::U64)
                            {
                                return Err(CompileError::new(
                                    "ckb::trusted_exec_cell_dep_u8_args expects (dep: u64, code_hash: Hash, argc: u64, arg0: u64, arg1: u64, arg2: u64, arg3: u64)",
                                    call.span,
                                ));
                            }
                            Type::Unit
                        }
                        ("ckb", "exec_cell_dep_hex4" | "spawn_wait_cell_dep_hex4") => {
                            self.validate_builtin_arity(name, 6, arg_types, call.span)?;
                            if arg_types[0] != Type::U64
                                || arg_types[1] != Type::Named("Vec<u8>".to_string())
                                || arg_types[2..].iter().any(|ty| *ty != Type::U64)
                            {
                                return Err(CompileError::new(
                                    format!("{name} expects (dep: u64, bytes: Vec<u8>, len0: u64, len1: u64, len2: u64, len3: u64)"),
                                    call.span,
                                ));
                            }
                            Type::Unit
                        }
                        ("ckb", "trusted_exec_cell_dep_hex4" | "trusted_spawn_wait_cell_dep_hex4") => {
                            self.validate_builtin_arity(name, 7, arg_types, call.span)?;
                            if arg_types[0] != Type::U64
                                || arg_types[1] != Type::Hash
                                || arg_types[2] != Type::Named("Vec<u8>".to_string())
                                || arg_types[3..].iter().any(|ty| *ty != Type::U64)
                            {
                                return Err(CompileError::new(
                                    format!("{name} expects (dep: u64, code_hash: Hash, bytes: Vec<u8>, len0: u64, len1: u64, len2: u64, len3: u64)"),
                                    call.span,
                                ));
                            }
                            Type::Unit
                        }
                        ("ckb", "current_script_hash" | "transaction_hash" | "raw_transaction_hash_without_cell_deps") => {
                            self.validate_builtin_arity(name, 0, arg_types, call.span)?;
                            Type::Hash
                        }
                        ("ckb", "script_hash") => {
                            self.validate_builtin_arity(name, 1, arg_types, call.span)?;
                            if arg_types[0] != Type::Hash {
                                return Err(CompileError::new(
                                    "ckb::script_hash expects a Hash and explicitly treats it as a complete CKB Script hash",
                                    call.span,
                                ));
                            }
                            Type::Named(CKB_SCRIPT_HASH_TYPE.to_string())
                        }
                        (
                            "ckb",
                            "require_cell_lock_exact_handle"
                            | "require_cell_type_exact_handle"
                            | "require_cell_dep_exact_verifier_handle",
                        ) => {
                            self.validate_builtin_arity(name, 3, arg_types, call.span)?;
                            let valid_view = if suffix == "require_cell_dep_exact_verifier_handle" {
                                matches!(&arg_types[0], Type::Named(view) if view == CKB_CELL_DEP_VIEW_TYPE)
                            } else {
                                matches!(
                                    &arg_types[0],
                                    Type::Named(view)
                                        if matches!(
                                            view.split('<').next().unwrap_or(view.as_str()),
                                            CKB_INPUT_VIEW_TYPE | CKB_OUTPUT_VIEW_TYPE | CKB_CELL_DEP_VIEW_TYPE
                                        )
                                )
                            };
                            if !valid_view
                                || arg_types[1] != Type::Named(CKB_EXACT_SCRIPT_HANDLE_TYPE.to_string())
                                || arg_types[2] != Type::Hash
                            {
                                return Err(CompileError::new(
                                    if suffix == "require_cell_dep_exact_verifier_handle" {
                                        format!("{name} expects (dep: CellDepView, handle: ExactScriptHandle, handle_hash: Hash)")
                                    } else {
                                        format!("{name} expects (source_view, handle: ExactScriptHandle, handle_hash: Hash)")
                                    },
                                    call.span,
                                ));
                            }
                            Type::Unit
                        }
                        ("ckb", "require_instance_script_participant_from_args") => {
                            self.validate_builtin_arity(name, 3, arg_types, call.span)?;
                            if !matches!(&arg_types[0], Type::Named(view) if view == CKB_CELL_DEP_VIEW_TYPE)
                                || !matches!(&arg_types[1], Type::Named(view) if view.starts_with("InstanceScriptHandle<"))
                                || !matches!(&arg_types[2], Type::Named(view) if matches!(view.split('<').next(), Some(CKB_INPUT_VIEW_TYPE | CKB_OUTPUT_VIEW_TYPE)))
                            {
                                return Err(CompileError::new(format!("{name} expects (dep: CellDepView, handle: InstanceScriptHandle<module>, subject: InputView|OutputView); policy and configuration come from the current Script's versioned args"), call.span));
                            }
                            Type::Unit
                        }
                        (
                            "ckb",
                            "require_cell_dep_open_script_handle_from_args"
                            | "require_cell_dep_open_verifier_handle_from_args"
                            | "require_open_script_participant_from_args",
                        ) => {
                            let participant = suffix == "require_open_script_participant_from_args";
                            self.validate_builtin_arity(name, if participant { 3 } else { 2 }, arg_types, call.span)?;
                            let valid_dep = matches!(&arg_types[0], Type::Named(view) if view == CKB_CELL_DEP_VIEW_TYPE);
                            let expected_class = if participant || suffix == "require_cell_dep_open_script_handle_from_args" {
                                crate::script_handle_contract::OPEN_SCRIPT_HANDLE_TYPE
                            } else {
                                crate::script_handle_contract::OPEN_VERIFIER_HANDLE_TYPE
                            };
                            let valid_handle = matches!(&arg_types[1], Type::Named(view) if view.split('<').next().unwrap_or(view.as_str()) == expected_class);
                            let valid_subject = !participant
                                || matches!(&arg_types[2], Type::Named(view) if matches!(view.split('<').next().unwrap_or(view.as_str()), CKB_INPUT_VIEW_TYPE | CKB_OUTPUT_VIEW_TYPE));
                            if !valid_dep || !valid_handle || !valid_subject {
                                return Err(CompileError::new(
                                    format!("{name} expects (dep: CellDepView, handle: {expected_class}<module>{}); the root comes from the current Script's committed 32-byte args", if participant { ", subject: InputView|OutputView" } else { "" }),
                                    call.span,
                                ));
                            }
                            Type::Unit
                        }
                        ("ckb", "require_cell_dep_open_script_handle" | "require_cell_dep_open_verifier_handle") => {
                            self.validate_builtin_arity(name, 3, arg_types, call.span)?;
                            let valid_dep = matches!(&arg_types[0], Type::Named(view) if view == CKB_CELL_DEP_VIEW_TYPE);
                            let expected_class = if suffix == "require_cell_dep_open_script_handle" {
                                crate::script_handle_contract::OPEN_SCRIPT_HANDLE_TYPE
                            } else {
                                crate::script_handle_contract::OPEN_VERIFIER_HANDLE_TYPE
                            };
                            let valid_handle = matches!(&arg_types[1], Type::Named(view)
                                if view.split('<').next().unwrap_or(view.as_str()) == expected_class);
                            if !valid_dep || !valid_handle || arg_types[2] != Type::Hash {
                                return Err(CompileError::new(
                                    format!(
                                        "{name} expects (dep: CellDepView, handle: {expected_class}<module>, expected_root: Hash)"
                                    ),
                                    call.span,
                                ));
                            }
                            Type::Unit
                        }
                        ("ckb", "require_cell_lock_deployment_line_handle" | "require_cell_type_deployment_line_handle") => {
                            self.validate_builtin_arity(name, 5, arg_types, call.span)?;
                            let valid_source = matches!(
                                &arg_types[0],
                                Type::Named(view)
                                    if matches!(
                                        view.split('<').next().unwrap_or(view.as_str()),
                                        CKB_INPUT_VIEW_TYPE | CKB_OUTPUT_VIEW_TYPE | CKB_CELL_DEP_VIEW_TYPE
                                    )
                            );
                            let valid_dep = |ty: &Type| matches!(ty, Type::Named(view) if view == CKB_CELL_DEP_VIEW_TYPE);
                            if !valid_source
                                || !valid_dep(&arg_types[1])
                                || !valid_dep(&arg_types[2])
                                || arg_types[3] != Type::Named(CKB_DEPLOYMENT_LINE_HANDLE_TYPE.to_string())
                                || arg_types[4] != Type::Hash
                            {
                                return Err(CompileError::new(
                                    format!(
                                        "{name} expects (source_view, admission: CellDepView, code: CellDepView, handle: DeploymentLineHandle, handle_hash: Hash)"
                                    ),
                                    call.span,
                                ));
                            }
                            Type::Unit
                        }
                        ("ckb", "require_cell_dep_deployment_line_verifier_handle") => {
                            self.validate_builtin_arity(name, 4, arg_types, call.span)?;
                            let valid_dep = |ty: &Type| matches!(ty, Type::Named(view) if view == CKB_CELL_DEP_VIEW_TYPE);
                            if !valid_dep(&arg_types[0])
                                || !valid_dep(&arg_types[1])
                                || arg_types[2] != Type::Named(CKB_DEPLOYMENT_LINE_HANDLE_TYPE.to_string())
                                || arg_types[3] != Type::Hash
                            {
                                return Err(CompileError::new(
                                    format!(
                                        "{name} expects (admission: CellDepView, code: CellDepView, handle: DeploymentLineHandle, handle_hash: Hash)"
                                    ),
                                    call.span,
                                ));
                            }
                            Type::Unit
                        }
                        ("ckb", "transaction_u32_le") => {
                            self.validate_builtin_arity(name, 1, arg_types, call.span)?;
                            if arg_types[0] != Type::U64 {
                                return Err(CompileError::new("transaction_u32_le expects a u64 offset", call.span));
                            }
                            Type::U64
                        }
                        ("ckb", "transaction_blake2b_gather") => {
                            self.validate_builtin_arity(name, 4, arg_types, call.span)?;
                            for (ty, expected) in arg_types.iter().zip(["Vec<u64>", "Vec<u64>", "Vec<u8>", "Vec<u8>"]) {
                                if *ty != Type::Named(expected.to_string()) {
                                    return Err(CompileError::new("transaction_blake2b_gather expects (offsets: Vec<u64>, lengths: Vec<u64>, prefix: Vec<u8>, suffix: Vec<u8>)", call.span));
                                }
                            }
                            Type::Hash
                        }
                        ("witness", "blake2b_select_chunks") => {
                            self.validate_builtin_arity(name, 6, arg_types, call.span)?;
                            if !Self::is_witness_args_view_type(&arg_types[0])
                                || arg_types[1..3].iter().any(|ty| *ty != Type::U64)
                                || arg_types[3..].iter().any(|ty| *ty != Type::Named("Vec<u8>".into()))
                            {
                                return Err(CompileError::new("witness::blake2b_select_chunks expects (view, start: u64, stride: u64, selection: Vec<u8>, prefix: Vec<u8>, suffix: Vec<u8>)", call.span));
                            }
                            Type::Hash
                        }
                        ("ckb", "since_epoch_absolute" | "since_epoch_relative") => {
                            self.validate_builtin_arity(name, 3, arg_types, call.span)?;
                            if arg_types.iter().any(|ty| *ty != Type::U64) {
                                return Err(CompileError::new(
                                    format!("{} expects (number: u64, index: u64, length: u64)", name),
                                    call.span,
                                ));
                            }
                            Type::U64
                        }
                        ("ckb", "since_absolute_epoch" | "since_relative_epoch") => {
                            self.validate_builtin_arity(name, 3, arg_types, call.span)?;
                            if arg_types.iter().any(|ty| *ty != Type::U64) {
                                return Err(CompileError::new(
                                    format!("{} expects (number: u64, index: u64, length: u64)", name),
                                    call.span,
                                ));
                            }
                            Type::Named(
                                if suffix == "since_absolute_epoch" {
                                    CKB_ABSOLUTE_EPOCH_SINCE_TYPE
                                } else {
                                    CKB_RELATIVE_EPOCH_SINCE_TYPE
                                }
                                .to_string(),
                            )
                        }
                        (
                            "ckb",
                            "since_absolute_block" | "since_relative_block" | "since_absolute_timestamp" | "since_relative_timestamp",
                        ) => {
                            self.validate_builtin_arity(name, 1, arg_types, call.span)?;
                            if arg_types[0] != Type::U64 {
                                return Err(CompileError::new(format!("{} expects a u64 metric value", name), call.span));
                            }
                            let result = match suffix {
                                "since_absolute_block" => CKB_ABSOLUTE_BLOCK_SINCE_TYPE,
                                "since_relative_block" => CKB_RELATIVE_BLOCK_SINCE_TYPE,
                                "since_absolute_timestamp" => CKB_ABSOLUTE_TIMESTAMP_SINCE_TYPE,
                                _ => CKB_RELATIVE_TIMESTAMP_SINCE_TYPE,
                            };
                            Type::Named(result.to_string())
                        }
                        ("ckb", "since_decode") => {
                            self.validate_builtin_arity(name, 1, arg_types, call.span)?;
                            if arg_types[0] != Type::Named(CKB_ENCODED_SINCE_TYPE.to_string()) {
                                return Err(CompileError::new("ckb::since_decode expects an EncodedSince value", call.span));
                            }
                            Type::Named(CKB_DECODED_SINCE_TYPE.to_string())
                        }
                        ("ckb", "since_from_raw_checked") => {
                            self.validate_builtin_arity(name, 1, arg_types, call.span)?;
                            if arg_types[0] != Type::U64 {
                                return Err(CompileError::new("ckb::since_from_raw_checked expects a u64 value", call.span));
                            }
                            Type::Named(CKB_DECODED_SINCE_TYPE.to_string())
                        }
                        (
                            "ckb",
                            "since_as_absolute_block"
                            | "since_as_relative_block"
                            | "since_as_absolute_epoch"
                            | "since_as_relative_epoch"
                            | "since_as_absolute_timestamp"
                            | "since_as_relative_timestamp",
                        ) => {
                            self.validate_builtin_arity(name, 1, arg_types, call.span)?;
                            if arg_types[0] != Type::Named(CKB_DECODED_SINCE_TYPE.to_string()) {
                                return Err(CompileError::new(format!("{} expects a DecodedSince value", name), call.span));
                            }
                            let result = match suffix {
                                "since_as_absolute_block" => CKB_ABSOLUTE_BLOCK_SINCE_TYPE,
                                "since_as_relative_block" => CKB_RELATIVE_BLOCK_SINCE_TYPE,
                                "since_as_absolute_epoch" => CKB_ABSOLUTE_EPOCH_SINCE_TYPE,
                                "since_as_relative_epoch" => CKB_RELATIVE_EPOCH_SINCE_TYPE,
                                "since_as_absolute_timestamp" => CKB_ABSOLUTE_TIMESTAMP_SINCE_TYPE,
                                _ => CKB_RELATIVE_TIMESTAMP_SINCE_TYPE,
                            };
                            Type::Named(result.to_string())
                        }
                        ("ckb", "since_is_relative" | "since_is_disabled") => {
                            self.validate_builtin_arity(name, 1, arg_types, call.span)?;
                            if arg_types[0] != Type::Named(CKB_DECODED_SINCE_TYPE.to_string()) {
                                return Err(CompileError::new(format!("{} expects a DecodedSince value", name), call.span));
                            }
                            Type::Bool
                        }
                        ("ckb", "since_metric" | "since_value") => {
                            self.validate_builtin_arity(name, 1, arg_types, call.span)?;
                            if arg_types[0] != Type::Named(CKB_DECODED_SINCE_TYPE.to_string()) {
                                return Err(CompileError::new(format!("{} expects a DecodedSince value", name), call.span));
                            }
                            Type::U64
                        }
                        ("ckb", "epoch_duration") => {
                            self.validate_builtin_arity(name, 1, arg_types, call.span)?;
                            if arg_types[0] != Type::U64 {
                                return Err(CompileError::new("ckb::epoch_duration expects a u64 value", call.span));
                            }
                            Type::Named(CKB_EPOCH_DURATION_TYPE.to_string())
                        }
                        ("ckb", "epoch_add" | "epoch_sub") => {
                            self.validate_builtin_arity(name, 2, arg_types, call.span)?;
                            if arg_types[0] != Type::Named(CKB_EPOCH_NUMBER_TYPE.to_string())
                                || arg_types[1] != Type::Named(CKB_EPOCH_DURATION_TYPE.to_string())
                            {
                                return Err(CompileError::new(
                                    format!("{} expects (epoch: EpochNumber, duration: EpochDuration)", name),
                                    call.span,
                                ));
                            }
                            Type::Named(CKB_EPOCH_NUMBER_TYPE.to_string())
                        }
                        ("ckb", "since_to_raw") => {
                            self.validate_builtin_arity(name, 1, arg_types, call.span)?;
                            if !Self::is_ckb_since_type(&arg_types[0]) {
                                return Err(CompileError::new(
                                    "ckb::since_to_raw expects EncodedSince or a typed Since<Mode, Metric> value",
                                    call.span,
                                ));
                            }
                            Type::U64
                        }
                        (
                            "ckb",
                            "epoch_number_to_u64"
                            | "epoch_duration_to_u64"
                            | "block_number_to_u64"
                            | "epoch_length_to_u64"
                            | "timestamp_millis_to_u64",
                        ) => {
                            self.validate_builtin_arity(name, 1, arg_types, call.span)?;
                            let expected = match suffix {
                                "epoch_number_to_u64" => CKB_EPOCH_NUMBER_TYPE,
                                "epoch_duration_to_u64" => CKB_EPOCH_DURATION_TYPE,
                                "block_number_to_u64" => CKB_BLOCK_NUMBER_TYPE,
                                "epoch_length_to_u64" => CKB_EPOCH_LENGTH_TYPE,
                                _ => CKB_TIMESTAMP_MILLIS_TYPE,
                            };
                            if arg_types[0] != Type::Named(expected.to_string()) {
                                return Err(CompileError::new(format!("{} expects a {} value", name, expected), call.span));
                            }
                            Type::U64
                        }
                        (
                            "ckb",
                            "cell_capacity"
                            | "cell_occupied_capacity"
                            | "cell_unoccupied_capacity"
                            | "cell_output_index"
                            | "input_out_point_index"
                            | "input_out_point_tx_hash_low"
                            | "input_since_at"
                            | "cell_lock_hash_low"
                            | "cell_type_hash_low"
                            | "cell_lock_hash_type"
                            | "cell_type_hash_type"
                            | "cell_count"
                            | "cell_lock_size"
                            | "cell_type_size"
                            | "cell_data_size",
                        ) => {
                            self.validate_builtin_arity(name, 1, arg_types, call.span)?;
                            let source_ok =
                                if matches!(suffix, "input_out_point_index" | "input_out_point_tx_hash_low" | "input_since_at") {
                                    Self::is_input_view_type(&arg_types[0])
                                } else {
                                    Self::is_cell_view_type(&arg_types[0])
                                };
                            if !source_ok {
                                return Err(CompileError::new(
                                    format!("{} expects a source view returned by source::*", name),
                                    call.span,
                                ));
                            }
                            if suffix == "input_out_point_index" {
                                Type::U32
                            } else {
                                Type::U64
                            }
                        }
                        ("ckb", "input_out_point_tx_hash") => {
                            self.validate_builtin_arity(name, 1, arg_types, call.span)?;
                            if !Self::is_input_view_type(&arg_types[0]) {
                                return Err(CompileError::new(
                                    format!("{} expects a source view returned by source::*", name),
                                    call.span,
                                ));
                            }
                            Type::Hash
                        }
                        ("ckb", "hash_data_packed") => {
                            self.validate_builtin_arity(name, 1, arg_types, call.span)?;
                            if matches!(arg_types[0], Type::Unit) {
                                return Err(CompileError::new("ckb::hash_data_packed expects packed data", call.span));
                            }
                            Type::Hash
                        }
                        ("ckb", "hash_sha256" | "hash_sha256d") => {
                            self.validate_builtin_arity(name, 1, arg_types, call.span)?;
                            if arg_types[0] != Type::Hash {
                                return Err(CompileError::new(format!("{} expects Hash input", name), call.span));
                            }
                            Type::Hash
                        }
                        ("ckb", "hash_sha256_pair" | "hash_sha256d_pair") => {
                            self.validate_builtin_arity(name, 2, arg_types, call.span)?;
                            if arg_types.iter().any(|ty| *ty != Type::Hash) {
                                return Err(CompileError::new(format!("{} expects (Hash, Hash)", name), call.span));
                            }
                            Type::Hash
                        }
                        ("ckb", "require_sha256d_merkle_root") => {
                            self.validate_builtin_arity(name, 5, arg_types, call.span)?;
                            let siblings_ty = Type::Array(Box::new(Type::Hash), 16);
                            if arg_types[0] != Type::Hash
                                || arg_types[1] != siblings_ty
                                || arg_types[2] != Type::U64
                                || arg_types[3] != Type::U64
                                || arg_types[4] != Type::Hash
                            {
                                return Err(CompileError::new(
                                    "ckb::require_sha256d_merkle_root expects (leaf: Hash, siblings: [Hash; 16], depth: u64, leaf_index: u64, expected_root: Hash)",
                                    call.span,
                                ));
                            }
                            match &call.args[2] {
                                Expr::Integer(0..=16) => {}
                                Expr::Integer(_) => {
                                    return Err(CompileError::new(
                                        "ckb::require_sha256d_merkle_root depth must be in 0..=16",
                                        call.span,
                                    ));
                                }
                                _ => {
                                    return Err(CompileError::new(
                                        "ckb::require_sha256d_merkle_root depth must be an integer literal",
                                        call.span,
                                    ));
                                }
                            }
                            Type::Unit
                        }
                        ("ckb", "cell_data_blake2b_span") | ("witness", "blake2b_span") => {
                            self.validate_builtin_arity(name, 3, arg_types, call.span)?;
                            let valid_view = if prefix == "ckb" {
                                Self::is_cell_view_type(&arg_types[0])
                            } else {
                                Self::is_witness_args_view_type(&arg_types[0])
                            };
                            if !valid_view || arg_types[1] != Type::U64 || arg_types[2] != Type::U64 {
                                return Err(CompileError::new(
                                    format!("{} expects (source_view, offset: u64, length: u64)", name),
                                    call.span,
                                ));
                            }
                            Type::Hash
                        }
                        ("ckb", "cell_data_bytes32") => {
                            self.validate_builtin_arity(name, 2, arg_types, call.span)?;
                            if !Self::is_cell_view_type(&arg_types[0]) || arg_types[1] != Type::U64 {
                                return Err(CompileError::new(format!("{} expects (Cell source view, offset: u64)", name), call.span));
                            }
                            Type::Array(Box::new(Type::U8), 32)
                        }
                        ("ckb", "cell_data_u8" | "cell_data_u32_le" | "cell_data_u64_le" | "cell_lock_u8" | "cell_type_u8") => {
                            self.validate_builtin_arity(name, 2, arg_types, call.span)?;
                            if !Self::is_cell_view_type(&arg_types[0]) || arg_types[1] != Type::U64 {
                                return Err(CompileError::new(format!("{} expects (source_view: u64, offset: u64)", name), call.span));
                            }
                            Type::U64
                        }
                        (
                            "ckb",
                            "cell_lock_hash"
                            | "cell_type_hash"
                            | "cell_data_hash_field"
                            | "cell_lock_code_hash"
                            | "cell_type_code_hash",
                        ) => {
                            self.validate_builtin_arity(name, 1, arg_types, call.span)?;
                            if !Self::is_cell_view_type(&arg_types[0]) {
                                return Err(CompileError::new(
                                    format!("{} expects a source view returned by source::*", name),
                                    call.span,
                                ));
                            }
                            Type::Hash
                        }
                        ("ckb", "cell_lock_args32" | "cell_type_args32") => {
                            self.validate_builtin_arity(name, 1, arg_types, call.span)?;
                            if !Self::is_cell_view_type(&arg_types[0]) {
                                return Err(CompileError::new(format!("{name} expects a Cell source view"), call.span));
                            }
                            Type::Array(Box::new(Type::U8), 32)
                        }
                        ("ckb", "cell_lock_args_empty" | "cell_type_args_empty" | "cell_has_type") => {
                            self.validate_builtin_arity(name, 1, arg_types, call.span)?;
                            if !Self::is_cell_view_type(&arg_types[0]) {
                                return Err(CompileError::new(
                                    format!("{} expects a source view returned by source::*", name),
                                    call.span,
                                ));
                            }
                            Type::Bool
                        }
                        ("ckb", "require_cell_lock_hash" | "require_cell_type_hash" | "require_cell_data_hash") => {
                            self.validate_builtin_arity(name, 2, arg_types, call.span)?;
                            if !Self::is_cell_view_type(&arg_types[0]) || arg_types[1] != Type::Hash {
                                return Err(CompileError::new(
                                    format!("{} expects (source_view: u64, expected_hash: Hash)", name),
                                    call.span,
                                ));
                            }
                            Type::Unit
                        }
                        ("ckb", "require_bounded_cell_dep_data_hash") => {
                            self.validate_builtin_arity(name, 2, arg_types, call.span)?;
                            if arg_types[0] != Type::U64 || arg_types[1] != Type::Hash {
                                return Err(CompileError::new(
                                    "ckb::require_bounded_cell_dep_data_hash expects (max_deps: u64, expected_data_hash: Hash)",
                                    call.span,
                                ));
                            }
                            match &call.args[0] {
                                Expr::Integer(1..=64) => {}
                                Expr::Integer(_) => {
                                    return Err(CompileError::new(
                                        "ckb::require_bounded_cell_dep_data_hash max_deps must be in 1..=64",
                                        call.span,
                                    ));
                                }
                                _ => {
                                    return Err(CompileError::new(
                                        "ckb::require_bounded_cell_dep_data_hash max_deps must be an integer literal",
                                        call.span,
                                    ));
                                }
                            }
                            Type::Unit
                        }
                        ("ckb", "require_current_script_args_empty") => {
                            self.validate_builtin_arity(name, 0, arg_types, call.span)?;
                            Type::Unit
                        }
                        ("ckb", "require_cell_lock_args_empty" | "require_cell_type_args_empty") => {
                            self.validate_builtin_arity(name, 1, arg_types, call.span)?;
                            if !Self::is_cell_view_type(&arg_types[0]) {
                                return Err(CompileError::new(
                                    format!("{} expects a source view returned by source::*", name),
                                    call.span,
                                ));
                            }
                            Type::Unit
                        }
                        (
                            "ckb",
                            "require_cell_lock_args32"
                            | "require_cell_type_args32"
                            | "require_cell_lock_args_prefix32"
                            | "require_cell_type_args_prefix32"
                            | "require_cell_lock_args_suffix32"
                            | "require_cell_type_args_suffix32",
                        ) => {
                            self.validate_builtin_arity(name, 2, arg_types, call.span)?;
                            if !Self::is_cell_view_type(&arg_types[0]) || arg_types[1] != Type::Array(Box::new(Type::U8), 32) {
                                return Err(CompileError::new(
                                    format!("{} expects (source_view: u64, expected_bytes: [u8; 32])", name),
                                    call.span,
                                ));
                            }
                            Type::Unit
                        }
                        ("ckb", "require_cell_lock_script_hash_type" | "require_cell_type_script_hash_type") => {
                            self.validate_builtin_arity(name, 3, arg_types, call.span)?;
                            if !Self::is_cell_view_type(&arg_types[0]) || arg_types[1] != Type::Hash || arg_types[2] != Type::U64 {
                                return Err(CompileError::new(
                                    format!("{} expects (source_view: u64, expected_code_hash: Hash, expected_hash_type: u64)", name),
                                    call.span,
                                ));
                            }
                            Type::Unit
                        }
                        ("ckb", "require_input_out_point_tx_hash" | "require_input_out_point") => {
                            let expects_index = suffix == "require_input_out_point";
                            let expected_arity = if expects_index { 3 } else { 2 };
                            self.validate_builtin_arity(name, expected_arity, arg_types, call.span)?;
                            let expected_index_ok = !expects_index || arg_types[2] == Type::U64;
                            if !Self::is_input_view_type(&arg_types[0]) || arg_types[1] != Type::Hash || !expected_index_ok {
                                return Err(CompileError::new(
                                    format!(
                                        "{} expects {}",
                                        name,
                                        if expects_index {
                                            "(source_view: u64, expected_hash: Hash, expected_index: u64)"
                                        } else {
                                            "(source_view: u64, expected_hash: Hash)"
                                        }
                                    ),
                                    call.span,
                                ));
                            }
                            Type::Unit
                        }
                        ("zk", "require_valid") => {
                            self.validate_builtin_arity(name, 8, arg_types, call.span)?;
                            if arg_types[1] != Type::Named(crate::zk_contract::PROOF_TYPE.to_string())
                                || arg_types[2] != Type::Named(CKB_CELL_DEP_VIEW_TYPE.to_string())
                                || arg_types[3] != Type::Named(CKB_EXACT_SCRIPT_HANDLE_TYPE.to_string())
                                || arg_types[4] != Type::Hash
                                || arg_types[5] != Type::Named("VerificationKeyCommitment".into())
                                || arg_types[6..].iter().any(|ty| *ty != Type::Hash)
                                || !matches!(&call.args[0], Expr::String(policy) if !policy.is_empty() && policy.len() <= 64 && policy.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'_'))
                            {
                                return Err(CompileError::new("zk::require_valid expects (literal_policy_name, ZkTransitionProof, CellDepView, ExactScriptHandle, literal_handle_hash, literal_vk_hash, literal_domain, literal_action); hashes use Hash", call.span));
                            }
                            Type::Unit
                        }
                        ("verifier::btc::bip340", "require_signature" | "require_signature_from_cell_dep") => {
                            let explicit_dep = suffix == "require_signature_from_cell_dep";
                            self.validate_builtin_arity(name, if explicit_dep { 4 } else { 3 }, arg_types, call.span)?;
                            let value_offset = usize::from(explicit_dep);
                            let pubkey_ty = Type::Array(Box::new(Type::U8), 32);
                            let signature_ty = Type::Array(Box::new(Type::U8), 64);
                            let message_type_ok = arg_types[value_offset] == Type::Hash
                                || arg_types[value_offset] == Type::Named(CKB_SIGHASH_ALL_DIGEST_TYPE.to_string());
                            if (explicit_dep && arg_types[0] != Type::U64)
                                || !message_type_ok
                                || arg_types[value_offset + 1] != pubkey_ty
                                || arg_types[value_offset + 2] != signature_ty
                            {
                                return Err(CompileError::new(
                                    if explicit_dep {
                                        "verifier::btc::bip340::require_signature_from_cell_dep expects (dep_index: u64, message_hash: Hash, pubkey: [u8; 32], signature: [u8; 64])"
                                    } else {
                                        "verifier::btc::bip340::require_signature expects (message_hash: Hash, pubkey: [u8; 32], signature: [u8; 64])"
                                    },
                                    call.span,
                                ));
                            }
                            if explicit_dep {
                                match &call.args[0] {
                                    Expr::Integer(0..=63) => {}
                                    Expr::Integer(_) => {
                                        return Err(CompileError::new(
                                            "verifier::btc::bip340::require_signature_from_cell_dep dep_index must be in 0..=63",
                                            call.span,
                                        ));
                                    }
                                    _ => {
                                        return Err(CompileError::new(
                                            "verifier::btc::bip340::require_signature_from_cell_dep dep_index must be an integer literal",
                                            call.span,
                                        ));
                                    }
                                }
                            }
                            Type::Unit
                        }
                        ("ckb", "require_metapoint_relative") => {
                            self.validate_builtin_arity(name, 3, arg_types, call.span)?;
                            if !Self::is_cell_view_type(&arg_types[0])
                                || !Self::is_cell_view_type(&arg_types[1])
                                || arg_types[2] != Type::I32
                            {
                                return Err(CompileError::new(
                                    "ckb::require_metapoint_relative expects (base_view: u64, related_view: u64, relative_distance: i32)",
                                    call.span,
                                ));
                            }
                            Type::Unit
                        }
                        ("ckb", "require_lock_type_metapoint_pairs" | "require_type_lock_metapoint_pairs") => {
                            self.validate_builtin_arity(name, 2, arg_types, call.span)?;
                            if !Self::is_cell_view_type(&arg_types[0]) || arg_types[1] != Type::I32 {
                                return Err(CompileError::new(
                                    format!("{} expects (source_view: u64, relative_distance: i32)", name),
                                    call.span,
                                ));
                            }
                            Type::Unit
                        }
                        (
                            "ckb",
                            "require_lock_type_metapoint_pairs_from_i32_data" | "require_type_lock_metapoint_pairs_from_i32_data",
                        ) => {
                            self.validate_builtin_arity(name, 2, arg_types, call.span)?;
                            if !Self::is_cell_view_type(&arg_types[0]) || arg_types[1] != Type::U64 {
                                return Err(CompileError::new(
                                    format!("{} expects (source_view: u64, distance_offset: u64)", name),
                                    call.span,
                                ));
                            }
                            Type::Unit
                        }
                        (
                            "ckb",
                            "require_lock_type_metapoint_pairs_from_i32_data_filtered"
                            | "require_type_lock_metapoint_pairs_from_i32_data_filtered",
                        ) => {
                            self.validate_builtin_arity(name, 4, arg_types, call.span)?;
                            if !Self::is_cell_view_type(&arg_types[0])
                                || arg_types[1] != Type::U64
                                || arg_types[2] != Type::Hash
                                || arg_types[3] != Type::U64
                            {
                                return Err(CompileError::new(
                                    format!(
                                        "{} expects (source_view: u64, distance_offset: u64, expected_related_type_hash: Hash, related_data_rule: u64)",
                                        name
                                    ),
                                    call.span,
                                ));
                            }
                            Type::Unit
                        }
                        ("ckb", "require_lock_match_master_out_point_pairs_from_data") => {
                            self.validate_builtin_arity(name, 5, arg_types, call.span)?;
                            if !Self::is_cell_view_type(&arg_types[0])
                                || !Self::is_cell_view_type(&arg_types[1])
                                || arg_types[2..].iter().any(|ty| *ty != Type::U64)
                            {
                                return Err(CompileError::new(
                                    "ckb::require_lock_match_master_out_point_pairs_from_data expects (input_source_view: u64, output_source_view: u64, action_offset: u64, tx_hash_offset: u64, index_offset: u64)",
                                    call.span,
                                ));
                            }
                            Type::Unit
                        }
                        ("source", "input" | "output" | "cell_dep" | "header_dep" | "group_input" | "group_output") => {
                            self.validate_builtin_arity(name, 1, arg_types, call.span)?;
                            if arg_types[0] != Type::U64 {
                                return Err(CompileError::new(format!("{} expects a u64 index", name), call.span));
                            }
                            Type::U64
                        }
                        ("dao", "accumulated_rate") => {
                            self.validate_builtin_arity(name, 1, arg_types, call.span)?;
                            if !Self::is_header_dep_view_type(&arg_types[0]) {
                                return Err(CompileError::new(
                                    "dao::accumulated_rate expects a HeaderDep source view returned by source::header_dep",
                                    call.span,
                                ));
                            }
                            Type::U64
                        }
                        ("dao", "input_accumulated_rate") => {
                            self.validate_builtin_arity(name, 1, arg_types, call.span)?;
                            if !Self::is_input_view_type(&arg_types[0]) {
                                return Err(CompileError::new(
                                    "dao::input_accumulated_rate expects an Input or GroupInput source view returned by source::input/source::group_input",
                                    call.span,
                                ));
                            }
                            Type::U64
                        }
                        ("dao", "is_deposit_data" | "is_withdrawal_request_data" | "has_dao_type") => {
                            self.validate_builtin_arity(name, 1, arg_types, call.span)?;
                            if !Self::is_cell_view_type(&arg_types[0]) {
                                return Err(CompileError::new(
                                    format!("{} expects a source view returned by source::*", name),
                                    call.span,
                                ));
                            }
                            Type::Bool
                        }
                        ("dao", "require_header_dep_for_input") => {
                            self.validate_builtin_arity(name, 2, arg_types, call.span)?;
                            if !Self::is_input_view_type(&arg_types[0]) || !Self::is_header_dep_view_type(&arg_types[1]) {
                                return Err(CompileError::new(
                                    "dao::require_header_dep_for_input expects (input_view: u64, header_view: u64)",
                                    call.span,
                                ));
                            }
                            Type::Unit
                        }
                        ("dao", "require_input_since_at_least") => {
                            self.validate_builtin_arity(name, 2, arg_types, call.span)?;
                            if !Self::is_input_view_type(&arg_types[0])
                                || (arg_types[1] != Type::U64 && !Self::is_ckb_since_type(&arg_types[1]))
                            {
                                return Err(CompileError::new(
                                    "dao::require_input_since_at_least expects (input_view, required_since: u64 or typed Since)",
                                    call.span,
                                ));
                            }
                            Type::Unit
                        }
                        ("dao", "require_input_relative_epoch_since_at_least") => {
                            self.validate_builtin_arity(name, 4, arg_types, call.span)?;
                            if !Self::is_input_view_type(&arg_types[0]) || arg_types[1..].iter().any(|ty| *ty != Type::U64) {
                                return Err(CompileError::new(
                                    "dao::require_input_relative_epoch_since_at_least expects (input_view: u64, number: u64, index: u64, length: u64)",
                                    call.span,
                                ));
                            }
                            Type::Unit
                        }
                        ("xudt", "owner_mode_input_type_hash" | "amount_low" | "amount_high") => {
                            self.validate_builtin_arity(name, 1, arg_types, call.span)?;
                            if !Self::is_cell_view_type(&arg_types[0]) {
                                return Err(CompileError::new(
                                    format!("{} expects a source view returned by source::*", name),
                                    call.span,
                                ));
                            }
                            Type::U64
                        }
                        ("xudt", "require_owner_mode_input_type") => {
                            self.validate_builtin_arity(name, 2, arg_types, call.span)?;
                            if !Self::is_cell_view_type(&arg_types[0]) || arg_types[1] != Type::Hash {
                                return Err(CompileError::new(
                                    "xudt::require_owner_mode_input_type expects (source_view: u64, expected_hash: Hash)",
                                    call.span,
                                ));
                            }
                            Type::Unit
                        }
                        ("xudt", "require_owner_mode_type_args") => {
                            self.validate_builtin_arity(name, 3, arg_types, call.span)?;
                            if !Self::is_cell_view_type(&arg_types[0]) || arg_types[1] != Type::Hash || arg_types[2] != Type::U64 {
                                return Err(CompileError::new(
                                    "xudt::require_owner_mode_type_args expects (source_view: u64, owner_hash: Hash, flags: u64)",
                                    call.span,
                                ));
                            }
                            Type::Unit
                        }
                        ("xudt", "require_owner_mode_type_args_current_script") => {
                            self.validate_builtin_arity(name, 2, arg_types, call.span)?;
                            if !Self::is_cell_view_type(&arg_types[0]) || arg_types[1] != Type::U64 {
                                return Err(CompileError::new(
                                    "xudt::require_owner_mode_type_args_current_script expects (source_view: u64, flags: u64)",
                                    call.span,
                                ));
                            }
                            Type::Unit
                        }
                        ("xudt", "require_group_amount_conserved") => {
                            self.validate_builtin_arity(name, 0, arg_types, call.span)?;
                            Type::Unit
                        }
                        ("xudt", "require_group_amount_minted" | "require_group_amount_burned") => {
                            self.validate_builtin_arity(name, 1, arg_types, call.span)?;
                            if arg_types[0] != Type::U128 {
                                return Err(CompileError::new(format!("{} expects a u128 delta amount", name), call.span));
                            }
                            Type::Unit
                        }
                        ("c256", "require_product_lte" | "require_product_eq") => {
                            self.validate_builtin_arity(name, 4, arg_types, call.span)?;
                            if arg_types.iter().any(|ty| *ty != Type::U128) {
                                return Err(CompileError::new(
                                    format!("{} expects four u128 operands: (left_amount, left_multiplier, right_amount, right_multiplier)", name),
                                    call.span,
                                ));
                            }
                            Type::Unit
                        }
                        ("c256", "require_sum2_products_lte" | "require_sum2_products_eq") => {
                            self.validate_builtin_arity(name, 8, arg_types, call.span)?;
                            if arg_types.iter().any(|ty| *ty != Type::U128) {
                                return Err(CompileError::new(
                                    format!(
                                        "{} expects eight u128 operands: left pair 1, left pair 2, right pair 1, right pair 2",
                                        name
                                    ),
                                    call.span,
                                ));
                            }
                            Type::Unit
                        }
                        ("witness", "count") => {
                            self.validate_builtin_arity(name, 0, arg_types, call.span)?;
                            Type::U64
                        }
                        ("witness", "byte" | "u32_le" | "u64_le" | "bytes32") => {
                            self.validate_builtin_arity(name, 2, arg_types, call.span)?;
                            let bounded = Self::witness_bytes_view_parts(&arg_types[0]).is_some();
                            if (!Self::is_witness_args_view_type(&arg_types[0]) && !bounded)
                                || (suffix == "bytes32" && bounded)
                                || arg_types[1] != Type::U64
                            {
                                return Err(CompileError::new(format!("{} expects (witness_view, offset: u64)", name), call.span));
                            }
                            if suffix == "bytes32" {
                                Type::Array(Box::new(Type::U8), 32)
                            } else {
                                Type::U64
                            }
                        }
                        ("witness", "lock_exact32" | "input_type_exact32" | "output_type_exact32") => {
                            self.validate_builtin_arity(name, 1, arg_types, call.span)?;
                            if !Self::is_witness_args_view_type(&arg_types[0]) {
                                return Err(CompileError::new(format!("{name} expects a witness source view"), call.span));
                            }
                            Type::Array(Box::new(Type::U8), 32)
                        }
                        ("witness", "blake2b") => {
                            self.validate_builtin_arity(name, 1, arg_types, call.span)?;
                            if Self::witness_bytes_view_parts(&arg_types[0]).is_none() {
                                return Err(CompileError::new("witness::blake2b expects a bounded witness byte view", call.span));
                            }
                            Type::Hash
                        }
                        ("witness", "size") => {
                            self.validate_builtin_arity(name, 1, arg_types, call.span)?;
                            if !Self::is_witness_args_view_type(&arg_types[0]) {
                                return Err(CompileError::new(
                                    format!("{} expects a source view returned by source::*", name),
                                    call.span,
                                ));
                            }
                            Type::U64
                        }
                        ("ckb", "require_witness_size_at_least") => {
                            self.validate_builtin_arity(name, 2, arg_types, call.span)?;
                            if !Self::is_witness_args_view_type(&arg_types[0]) || arg_types[1] != Type::U64 {
                                return Err(CompileError::new(
                                    format!("{} expects (source_view: u64, min_size: u64)", name),
                                    call.span,
                                ));
                            }
                            Type::Unit
                        }
                        ("Address", "zero") => {
                            self.validate_builtin_arity(name, 0, arg_types, call.span)?;
                            Type::Address
                        }
                        ("Hash", "zero") => {
                            self.validate_builtin_arity(name, 0, arg_types, call.span)?;
                            Type::Hash
                        }
                        ("Vec", "with_capacity") => {
                            self.validate_builtin_arity(name, 1, arg_types, call.span)?;
                            if arg_types[0] != Type::U64 {
                                return Err(CompileError::new("Vec::with_capacity expects a u64 capacity", call.span));
                            }
                            Type::Named("Vec".to_string())
                        }
                        (_, "new") => {
                            self.validate_builtin_arity(name, 0, arg_types, call.span)?;
                            self.validate_namespaced_type_constructor(prefix, suffix, call.span)?;
                            Type::Named(prefix.to_string())
                        }
                        (_, "zero") => {
                            self.validate_builtin_arity(name, 0, arg_types, call.span)?;
                            self.validate_namespaced_type_constructor(prefix, suffix, call.span)?;
                            Type::Named(prefix.to_string())
                        }
                        _ => return Err(CompileError::new(format!("unknown namespaced function '{}'", name), call.span)),
                    });
                }
                self.reject_borrow_view_in_builtin_call(name, call)?;
                if name == "min" || name == "max" || name == "isqrt" {
                    self.validate_numeric_builtin_call(name, arg_types, call.span)?;
                    return Ok(Type::U64);
                }
                match name.as_str() {
                    "wait" | "process_id" | "pipe_read" | "inherited_fd" | "close" => {
                        let expected = if matches!(name.as_str(), "wait" | "process_id") { 0 } else { 1 };
                        self.validate_builtin_arity(name, expected, arg_types, call.span)?;
                        if expected == 1 && arg_types[0] != Type::U64 {
                            return Err(CompileError::new(format!("{} expects a u64 file descriptor or index", name), call.span));
                        }
                        return Ok(Type::U64);
                    }
                    "pipe" => {
                        self.validate_builtin_arity(name, 0, arg_types, call.span)?;
                        return Ok(Type::Tuple(vec![Type::U64, Type::U64]));
                    }
                    "pipe_write" => {
                        self.validate_builtin_arity(name, 2, arg_types, call.span)?;
                        if arg_types[0] != Type::U64 || arg_types[1] != Type::U64 {
                            return Err(CompileError::new("pipe_write expects (fd: u64, value: u64)", call.span));
                        }
                        return Ok(Type::U64);
                    }
                    "require_maturity" | "require_time" => {
                        self.validate_builtin_arity(name, 1, arg_types, call.span)?;
                        if arg_types[0] != Type::U64 {
                            return Err(CompileError::new(format!("{} expects a u64 CKB since/time value", name), call.span));
                        }
                        return Ok(Type::Unit);
                    }
                    "require_epoch_after" | "require_epoch_relative" => {
                        self.validate_builtin_arity(name, 3, arg_types, call.span)?;
                        if arg_types.iter().any(|ty| *ty != Type::U64) {
                            return Err(CompileError::new(
                                format!("{} expects (number: u64, index: u64, length: u64)", name),
                                call.span,
                            ));
                        }
                        return Ok(Type::Unit);
                    }
                    "occupied_capacity" => {
                        self.validate_builtin_arity(name, 1, arg_types, call.span)?;
                        if !matches!(arg_types[0], Type::Named(ref ty) if ty == "String") {
                            return Err(CompileError::new("occupied_capacity expects a type name string literal", call.span));
                        }
                        return Ok(Type::U64);
                    }
                    "hash_chain" => {
                        self.validate_builtin_arity(name, 1, arg_types, call.span)?;
                        if arg_types[0] != Type::Hash {
                            return Err(CompileError::new("hash_chain expects Hash input", call.span));
                        }
                        return Ok(Type::Hash);
                    }
                    "hash_pair" => {
                        self.validate_builtin_arity(name, 2, arg_types, call.span)?;
                        if arg_types[0] != Type::Hash || arg_types[1] != Type::Hash {
                            return Err(CompileError::new("hash_pair expects (Hash, Hash)", call.span));
                        }
                        return Ok(Type::Hash);
                    }
                    "hash_blake2b" => {
                        self.validate_builtin_arity(name, 1, arg_types, call.span)?;
                        if arg_types[0] != Type::Hash {
                            return Err(CompileError::new("hash_blake2b expects Hash input", call.span));
                        }
                        return Ok(Type::Hash);
                    }
                    "hash_blake2b_packed" => {
                        self.validate_builtin_arity(name, 1, arg_types, call.span)?;
                        if matches!(arg_types[0], Type::Unit) {
                            return Err(CompileError::new("hash_blake2b_packed expects packed data", call.span));
                        }
                        return Ok(Type::Hash);
                    }
                    _ => {}
                }
                Err(CompileError::new(format!("unknown function '{}'", name), call.span))
            }
            Expr::FieldAccess(field) => {
                let receiver_ty = self.infer_expr(env, &field.expr)?;
                if let Some(result) = self.infer_named_role_method(&receiver_ty, call, arg_types)? {
                    return Ok(result);
                }
                if nested::contains_nested_type(&receiver_ty) && !matches!(field.field.as_str(), "len" | "is_empty") {
                    return Err(CompileError::new(
                        "nested values and derived views are read-only; only len/is_empty and indexed or field reads are supported",
                        call.span,
                    ));
                }
                match field.field.as_str() {
                    "type_hash" => {
                        self.validate_builtin_arity(&field.field, 0, arg_types, call.span)?;
                        Ok(Type::Hash)
                    }
                    "len" => {
                        self.validate_builtin_arity(&field.field, 0, arg_types, call.span)?;
                        if self.supports_collection_len(&receiver_ty) {
                            Ok(Type::U64)
                        } else {
                            Err(CompileError::new("len is only supported on array or Vec values", call.span))
                        }
                    }
                    "is_empty" => {
                        self.validate_builtin_arity(&field.field, 0, arg_types, call.span)?;
                        if self.supports_collection_len(&receiver_ty) {
                            Ok(Type::Bool)
                        } else {
                            Err(CompileError::new("is_empty is only supported on array or Vec values", call.span))
                        }
                    }
                    "capacity" => {
                        self.validate_builtin_arity("Vec.capacity", 0, arg_types, call.span)?;
                        match &receiver_ty {
                            Type::Named(name) if self.parse_named_collection_item_type(name).is_some() => Ok(Type::U64),
                            Type::Named(name) if name == "Vec" => Err(CompileError::new(
                                "Vec.capacity requires a typed Vec<T>; push or annotate the Vec before reading capacity",
                                call.span,
                            )),
                            _ => Err(CompileError::new("capacity is only supported on Vec values", call.span)),
                        }
                    }
                    "first" | "last" => {
                        self.validate_builtin_arity(&format!("Vec.{}", field.field), 0, arg_types, call.span)?;
                        match &receiver_ty {
                            Type::Named(name) => {
                                let Some(item_ty) = self.parse_named_collection_item_type(name) else {
                                    return Err(CompileError::new(
                                        format!(
                                            "Vec.{} requires a typed Vec<T>; push or annotate the Vec before reading",
                                            field.field
                                        ),
                                        call.span,
                                    ));
                                };
                                Ok(item_ty)
                            }
                            _ => Err(CompileError::new(format!("{} is only supported on Vec values", field.field), call.span)),
                        }
                    }
                    "push" => {
                        self.validate_builtin_arity("Vec.push", 1, arg_types, call.span)?;
                        let arg_ty = &arg_types[0];
                        if self.type_contains_reference(arg_ty) {
                            return Err(CompileError::new(
                                format!(
                                    "Vec.push cannot store reference type {}; Vec<T> values must use owned non-reference items",
                                    type_repr(arg_ty)
                                ),
                                call.span,
                            ));
                        }
                        if let Type::Named(name) = &receiver_ty {
                            if name == "Vec" {
                                if let Expr::Identifier(receiver_name) = field.expr.as_ref() {
                                    env.update_type(receiver_name, Type::Named(format!("Vec<{}>", type_repr(arg_ty))));
                                }
                                return Ok(Type::Unit);
                            }
                            if let Some(item_ty) = self.parse_named_collection_item_type(name) {
                                if !self.expr_type_compatible_with_expected(&call.args[0], arg_ty, &item_ty, call.span)? {
                                    return Err(CompileError::new(
                                        format!("Vec.push type mismatch: expected {:?}, found {:?}", item_ty, arg_ty),
                                        call.span,
                                    ));
                                }
                                return Ok(Type::Unit);
                            }
                        }
                        Err(CompileError::new("push is only supported on Vec values", call.span))
                    }
                    "clear" => {
                        self.validate_builtin_arity("Vec.clear", 0, arg_types, call.span)?;
                        match &receiver_ty {
                            Type::Named(name) if name == "Vec" || self.parse_named_collection_item_type(name).is_some() => {
                                Ok(Type::Unit)
                            }
                            _ => Err(CompileError::new("clear is only supported on Vec values", call.span)),
                        }
                    }
                    "reverse" => {
                        self.validate_builtin_arity("Vec.reverse", 0, arg_types, call.span)?;
                        match &receiver_ty {
                            Type::Named(name) if name == "Vec" || self.parse_named_collection_item_type(name).is_some() => {
                                Ok(Type::Unit)
                            }
                            _ => Err(CompileError::new("reverse is only supported on Vec values", call.span)),
                        }
                    }
                    "truncate" => {
                        self.validate_builtin_arity("Vec.truncate", 1, arg_types, call.span)?;
                        if arg_types[0] != Type::U64 {
                            return Err(CompileError::new("Vec.truncate expects a u64 length", call.span));
                        }
                        match &receiver_ty {
                            Type::Named(name) if name == "Vec" || self.parse_named_collection_item_type(name).is_some() => {
                                Ok(Type::Unit)
                            }
                            _ => Err(CompileError::new("truncate is only supported on Vec values", call.span)),
                        }
                    }
                    "swap" => {
                        self.validate_builtin_arity("Vec.swap", 2, arg_types, call.span)?;
                        if arg_types[0] != Type::U64 || arg_types[1] != Type::U64 {
                            return Err(CompileError::new("Vec.swap expects u64 indices", call.span));
                        }
                        match &receiver_ty {
                            Type::Named(name) if name == "Vec" || self.parse_named_collection_item_type(name).is_some() => {
                                Ok(Type::Unit)
                            }
                            _ => Err(CompileError::new("swap is only supported on Vec values", call.span)),
                        }
                    }
                    "contains" => {
                        self.validate_builtin_arity("Vec.contains", 1, arg_types, call.span)?;
                        let arg_ty = &arg_types[0];
                        match &receiver_ty {
                            Type::Named(name) if name == "Vec" => {
                                if let Expr::Identifier(receiver_name) = field.expr.as_ref() {
                                    env.update_type(receiver_name, Type::Named(format!("Vec<{}>", type_repr(arg_ty))));
                                }
                                Ok(Type::Bool)
                            }
                            Type::Named(name) => {
                                let Some(item_ty) = self.parse_named_collection_item_type(name) else {
                                    return Err(CompileError::new("contains is only supported on Vec values", call.span));
                                };
                                if !self.expr_type_compatible_with_expected(&call.args[0], arg_ty, &item_ty, call.span)? {
                                    return Err(CompileError::new(
                                        format!("Vec.contains type mismatch: expected {:?}, found {:?}", item_ty, arg_ty),
                                        call.span,
                                    ));
                                }
                                Ok(Type::Bool)
                            }
                            _ => Err(CompileError::new("contains is only supported on Vec values", call.span)),
                        }
                    }
                    "remove" => {
                        self.validate_builtin_arity("Vec.remove", 1, arg_types, call.span)?;
                        if arg_types[0] != Type::U64 {
                            return Err(CompileError::new("Vec.remove expects a u64 index", call.span));
                        }
                        match &receiver_ty {
                            Type::Named(name) => {
                                let Some(item_ty) = self.parse_named_collection_item_type(name) else {
                                    return Err(CompileError::new(
                                        "Vec.remove requires a typed Vec<T>; push or annotate the Vec before removing",
                                        call.span,
                                    ));
                                };
                                Ok(item_ty)
                            }
                            _ => Err(CompileError::new("remove is only supported on Vec values", call.span)),
                        }
                    }
                    "pop" => {
                        self.validate_builtin_arity("Vec.pop", 0, arg_types, call.span)?;
                        match &receiver_ty {
                            Type::Named(name) => {
                                let Some(item_ty) = self.parse_named_collection_item_type(name) else {
                                    return Err(CompileError::new(
                                        "Vec.pop requires a typed Vec<T>; push or annotate the Vec before popping",
                                        call.span,
                                    ));
                                };
                                Ok(item_ty)
                            }
                            _ => Err(CompileError::new("pop is only supported on Vec values", call.span)),
                        }
                    }
                    "insert" => {
                        self.validate_builtin_arity("Vec.insert", 2, arg_types, call.span)?;
                        if arg_types[0] != Type::U64 {
                            return Err(CompileError::new("Vec.insert expects a u64 index", call.span));
                        }
                        let arg_ty = &arg_types[1];
                        if self.type_contains_reference(arg_ty) {
                            return Err(CompileError::new(
                                format!(
                                    "Vec.insert cannot store reference type {}; Vec<T> values must use owned non-reference items",
                                    type_repr(arg_ty)
                                ),
                                call.span,
                            ));
                        }
                        match &receiver_ty {
                            Type::Named(name) if name == "Vec" => {
                                if let Expr::Identifier(receiver_name) = field.expr.as_ref() {
                                    env.update_type(receiver_name, Type::Named(format!("Vec<{}>", type_repr(arg_ty))));
                                }
                                Ok(Type::Unit)
                            }
                            Type::Named(name) => {
                                let Some(item_ty) = self.parse_named_collection_item_type(name) else {
                                    return Err(CompileError::new("insert is only supported on Vec values", call.span));
                                };
                                if !self.expr_type_compatible_with_expected(&call.args[1], arg_ty, &item_ty, call.span)? {
                                    return Err(CompileError::new(
                                        format!("Vec.insert type mismatch: expected {:?}, found {:?}", item_ty, arg_ty),
                                        call.span,
                                    ));
                                }
                                Ok(Type::Unit)
                            }
                            _ => Err(CompileError::new("insert is only supported on Vec values", call.span)),
                        }
                    }
                    "set" => {
                        self.validate_builtin_arity("Vec.set", 2, arg_types, call.span)?;
                        if arg_types[0] != Type::U64 {
                            return Err(CompileError::new("Vec.set expects a u64 index", call.span));
                        }
                        let arg_ty = &arg_types[1];
                        if self.type_contains_reference(arg_ty) {
                            return Err(CompileError::new(
                                format!(
                                    "Vec.set cannot store reference type {}; Vec<T> values must use owned non-reference items",
                                    type_repr(arg_ty)
                                ),
                                call.span,
                            ));
                        }
                        match &receiver_ty {
                            Type::Named(name) if name == "Vec" => {
                                if let Expr::Identifier(receiver_name) = field.expr.as_ref() {
                                    env.update_type(receiver_name, Type::Named(format!("Vec<{}>", type_repr(arg_ty))));
                                }
                                Ok(Type::Unit)
                            }
                            Type::Named(name) => {
                                let Some(item_ty) = self.parse_named_collection_item_type(name) else {
                                    return Err(CompileError::new("set is only supported on Vec values", call.span));
                                };
                                if !self.expr_type_compatible_with_expected(&call.args[1], arg_ty, &item_ty, call.span)? {
                                    return Err(CompileError::new(
                                        format!("Vec.set type mismatch: expected {:?}, found {:?}", item_ty, arg_ty),
                                        call.span,
                                    ));
                                }
                                Ok(Type::Unit)
                            }
                            _ => Err(CompileError::new("set is only supported on Vec values", call.span)),
                        }
                    }
                    "extend_from_slice" => {
                        self.validate_builtin_arity("Vec.extend_from_slice", 1, arg_types, call.span)?;
                        let Some(slice_item_ty) = Self::slice_item_type(&arg_types[0]) else {
                            return Err(CompileError::new("Vec.extend_from_slice expects an array or byte-slice source", call.span));
                        };
                        if self.type_contains_reference(&slice_item_ty) {
                            return Err(CompileError::new(
                                format!(
                                    "Vec.extend_from_slice cannot store reference type {}; Vec<T> values must use owned non-reference items",
                                    type_repr(&slice_item_ty)
                                ),
                                call.span,
                            ));
                        }
                        match &receiver_ty {
                            Type::Named(name) if name == "Vec" => {
                                if let Expr::Identifier(receiver_name) = field.expr.as_ref() {
                                    env.update_type(receiver_name, Type::Named(format!("Vec<{}>", type_repr(&slice_item_ty))));
                                }
                                Ok(Type::Unit)
                            }
                            Type::Named(name) => {
                                let Some(item_ty) = self.parse_named_collection_item_type(name) else {
                                    return Err(CompileError::new("extend_from_slice is only supported on Vec values", call.span));
                                };
                                if !self.types_equal(&item_ty, &slice_item_ty) {
                                    return Err(CompileError::new(
                                        format!(
                                            "Vec.extend_from_slice type mismatch: expected {:?}, found {:?}",
                                            item_ty, slice_item_ty
                                        ),
                                        call.span,
                                    ));
                                }
                                Ok(Type::Unit)
                            }
                            _ => Err(CompileError::new("extend_from_slice is only supported on Vec values", call.span)),
                        }
                    }
                    _ => self.lookup_field_type(&receiver_ty, &field.field, field.span),
                }
            }
            _ => Err(CompileError::new("unsupported call target", call.span)),
        }
    }
}

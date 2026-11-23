use super::super::{
    CellScriptRuntimeError, CodeGenerator, CKB_EPOCH_FRACTION_BOUND, CKB_EPOCH_NUMBER_BOUND,
    CKB_SINCE_EPOCH_NUMBER_WITH_FRACTION_FLAG, CKB_SINCE_METRIC_TYPE_FLAG_MASK, CKB_SINCE_RELATIVE_FLAG, CKB_SINCE_TIMESTAMP_BOUND,
    CKB_SINCE_VALUE_MASK,
};
use super::super::{CKB_EPOCH_FRACTION_MASK, CKB_SINCE_REMAIN_FLAGS_BITS, CKB_SINCE_TIMESTAMP_FLAG};

impl CodeGenerator {
    pub(super) fn emit_runtime_ckb_since_epoch_helper(&mut self, symbol: &str, relative: bool, detail: &str, enabled: bool) {
        self.emit_global(symbol);
        self.emit_label(symbol);
        self.emit(format!("# cellscript abi: {}", detail));
        self.emit("# cellscript abi: args a0=number(<2^24), a1=index(<2^16), a2=length(<2^16); requires length>0 and index<length");
        self.emit("# cellscript abi: encodes CKB RFC0017 EpochNumberWithFraction as number | index<<24 | length<<40");
        if !enabled {
            self.emit("li a0, 0");
            self.emit(format!("li a1, {}", CellScriptRuntimeError::SyscallFailed.code()));
            self.emit("ret");
            return;
        }

        let malformed = self.fresh_label("ckb_since_epoch_malformed");
        let done = self.fresh_label("ckb_since_epoch_done");
        self.emit(format!("li t0, {}", CKB_EPOCH_NUMBER_BOUND));
        self.emit("sltu t1, a0, t0");
        self.emit(format!("beqz t1, {}", malformed));
        self.emit(format!("li t0, {}", CKB_EPOCH_FRACTION_BOUND));
        self.emit("sltu t1, a1, t0");
        self.emit(format!("beqz t1, {}", malformed));
        self.emit("sltu t1, a2, t0");
        self.emit(format!("beqz t1, {}", malformed));
        self.emit(format!("beqz a2, {}", malformed));
        self.emit("sltu t1, a1, a2");
        self.emit(format!("beqz t1, {}", malformed));
        self.emit_add_immediate("t2", "a0", 0);
        self.emit("slli t0, a1, 24");
        self.emit("or t2, t2, t0");
        self.emit("slli t0, a2, 40");
        self.emit("or t2, t2, t0");
        self.emit(format!("li t0, {}", CKB_SINCE_EPOCH_NUMBER_WITH_FRACTION_FLAG));
        self.emit("or t2, t2, t0");
        if relative {
            self.emit("li t0, 1");
            self.emit("slli t0, t0, 63");
            self.emit("or t2, t2, t0");
        }
        self.emit_add_immediate("a0", "t2", 0);
        self.emit("li a1, 0");
        self.emit(format!("j {}", done));

        self.emit_label(&malformed);
        self.emit("li a0, 0");
        self.emit(format!("li a1, {}", CellScriptRuntimeError::CkbSinceMalformed.code()));
        self.emit_label(&done);
        self.emit("ret");
    }

    pub(super) fn emit_runtime_ckb_since_scalar_helper(
        &mut self,
        symbol: &str,
        relative: bool,
        metric_flag: u64,
        timestamp: bool,
        detail: &str,
        enabled: bool,
    ) {
        self.emit_global(symbol);
        self.emit_label(symbol);
        self.emit(format!("# cellscript abi: {detail}; arg a0 is the low-bit metric value"));
        if !enabled {
            self.emit("li a0, 0");
            self.emit(format!("li a1, {}", CellScriptRuntimeError::SyscallFailed.code()));
            self.emit("ret");
            return;
        }

        let malformed = self.fresh_label("ckb_since_scalar_malformed");
        let done = self.fresh_label("ckb_since_scalar_done");
        let bound = if timestamp { CKB_SINCE_TIMESTAMP_BOUND } else { CKB_SINCE_VALUE_MASK + 1 };
        self.emit(format!("li t0, {bound}"));
        self.emit("sltu t1, a0, t0");
        self.emit(format!("beqz t1, {}", malformed));
        self.emit(format!("li t0, {metric_flag}"));
        self.emit("or a0, a0, t0");
        if relative {
            self.emit(format!("li t0, {CKB_SINCE_RELATIVE_FLAG}"));
            self.emit("or a0, a0, t0");
        }
        self.emit("li a1, 0");
        self.emit(format!("j {}", done));
        self.emit_label(&malformed);
        self.emit("li a0, 0");
        self.emit(format!("li a1, {}", CellScriptRuntimeError::CkbSinceMalformed.code()));
        self.emit_label(&done);
        self.emit("ret");
    }

    fn emit_runtime_ckb_since_validation(&mut self, malformed: &str) {
        let non_epoch = self.fresh_label("ckb_since_decode_non_epoch");
        let epoch_nonzero_length = self.fresh_label("ckb_since_decode_epoch_nonzero_length");
        let valid = self.fresh_label("ckb_since_decode_valid");

        self.emit(format!("li t0, {CKB_SINCE_REMAIN_FLAGS_BITS}"));
        self.emit("and t1, a0, t0");
        self.emit(format!("bnez t1, {malformed}"));
        self.emit(format!("li t0, {CKB_SINCE_METRIC_TYPE_FLAG_MASK}"));
        self.emit("and t1, a0, t0");
        self.emit(format!("beq t1, t0, {malformed}"));
        self.emit(format!("li t0, {CKB_SINCE_EPOCH_NUMBER_WITH_FRACTION_FLAG}"));
        self.emit(format!("bne t1, t0, {}", non_epoch));

        self.emit(format!("li t0, {CKB_EPOCH_FRACTION_MASK}"));
        self.emit("srli t2, a0, 24");
        self.emit("and t2, t2, t0");
        self.emit("srli t3, a0, 40");
        self.emit("and t3, t3, t0");
        self.emit(format!("bnez t3, {}", epoch_nonzero_length));
        self.emit(format!("beqz t2, {}", valid));
        self.emit(format!("j {malformed}"));
        self.emit_label(&epoch_nonzero_length);
        self.emit("sltu t4, t2, t3");
        self.emit(format!("bnez t4, {}", valid));
        self.emit(format!("j {malformed}"));

        self.emit_label(&non_epoch);
        self.emit(format!("li t0, {CKB_SINCE_TIMESTAMP_FLAG}"));
        self.emit(format!("bne t1, t0, {}", valid));
        self.emit(format!("li t0, {CKB_SINCE_VALUE_MASK}"));
        self.emit("and t2, a0, t0");
        self.emit(format!("li t0, {CKB_SINCE_TIMESTAMP_BOUND}"));
        self.emit("sltu t3, t2, t0");
        self.emit(format!("beqz t3, {malformed}"));
        self.emit_label(&valid);
    }

    pub(super) fn emit_runtime_ckb_since_decode_helper(&mut self, symbol: &str, enabled: bool) {
        self.emit_global(symbol);
        self.emit_label(symbol);
        self.emit("# cellscript abi: validates RFC0017 reserved flags, metric, epoch fraction, and timestamp multiplication bound");
        if !enabled {
            self.emit("li a0, 0");
            self.emit(format!("li a1, {}", CellScriptRuntimeError::SyscallFailed.code()));
            self.emit("ret");
            return;
        }
        let malformed = self.fresh_label("ckb_since_decode_malformed");
        let done = self.fresh_label("ckb_since_decode_done");
        self.emit_runtime_ckb_since_validation(&malformed);
        self.emit("li a1, 0");
        self.emit(format!("j {}", done));
        self.emit_label(&malformed);
        self.emit("li a0, 0");
        self.emit(format!("li a1, {}", CellScriptRuntimeError::CkbSinceMalformed.code()));
        self.emit_label(&done);
        self.emit("ret");
    }

    pub(super) fn emit_runtime_ckb_since_narrow_helper(
        &mut self,
        symbol: &str,
        relative: bool,
        metric_flag: u64,
        detail: &str,
        enabled: bool,
    ) {
        self.emit_global(symbol);
        self.emit_label(symbol);
        self.emit(format!("# cellscript abi: narrows a validated DecodedSince to {detail}"));
        if !enabled {
            self.emit("li a0, 0");
            self.emit(format!("li a1, {}", CellScriptRuntimeError::SyscallFailed.code()));
            self.emit("ret");
            return;
        }
        let malformed = self.fresh_label("ckb_since_narrow_malformed");
        let done = self.fresh_label("ckb_since_narrow_done");
        self.emit_runtime_ckb_since_validation(&malformed);
        let expected = metric_flag | if relative { CKB_SINCE_RELATIVE_FLAG } else { 0 };
        self.emit(format!("li t0, {}", CKB_SINCE_RELATIVE_FLAG | CKB_SINCE_METRIC_TYPE_FLAG_MASK));
        self.emit("and t1, a0, t0");
        self.emit(format!("li t0, {expected}"));
        self.emit(format!("bne t1, t0, {}", malformed));
        self.emit("li a1, 0");
        self.emit(format!("j {}", done));
        self.emit_label(&malformed);
        self.emit("li a0, 0");
        self.emit(format!("li a1, {}", CellScriptRuntimeError::CkbSinceMalformed.code()));
        self.emit_label(&done);
        self.emit("ret");
    }

    pub(super) fn emit_runtime_ckb_since_projection_helper(&mut self, symbol: &str, operation: &str, detail: &str, enabled: bool) {
        self.emit_global(symbol);
        self.emit_label(symbol);
        self.emit(format!("# cellscript abi: {detail}"));
        if !enabled {
            self.emit("li a0, 0");
            self.emit(format!("li a1, {}", CellScriptRuntimeError::SyscallFailed.code()));
            self.emit("ret");
            return;
        }
        let malformed = self.fresh_label("ckb_since_projection_malformed");
        let done = self.fresh_label("ckb_since_projection_done");
        self.emit_runtime_ckb_since_validation(&malformed);
        match operation {
            "relative" => self.emit("srli a0, a0, 63"),
            "disabled" => self.emit("seqz a0, a0"),
            "metric" => {
                self.emit("srli a0, a0, 61");
                self.emit("andi a0, a0, 3");
            }
            "value" => {
                self.emit(format!("li t0, {CKB_SINCE_VALUE_MASK}"));
                self.emit("and a0, a0, t0");
            }
            _ => unreachable!("known CKB Since projection"),
        }
        self.emit("li a1, 0");
        self.emit(format!("j {}", done));
        self.emit_label(&malformed);
        self.emit("li a0, 0");
        self.emit(format!("li a1, {}", CellScriptRuntimeError::CkbSinceMalformed.code()));
        self.emit_label(&done);
        self.emit("ret");
    }

    pub(super) fn emit_runtime_ckb_epoch_arithmetic_helper(&mut self, symbol: &str, operation: &str, detail: &str, enabled: bool) {
        self.emit_global(symbol);
        self.emit_label(symbol);
        self.emit(format!("# cellscript abi: {detail}; values must remain below 2^24"));
        if !enabled {
            self.emit("li a0, 0");
            self.emit(format!("li a1, {}", CellScriptRuntimeError::SyscallFailed.code()));
            self.emit("ret");
            return;
        }

        let invalid = self.fresh_label("ckb_epoch_arithmetic_invalid");
        let done = self.fresh_label("ckb_epoch_arithmetic_done");
        self.emit(format!("li t0, {CKB_EPOCH_NUMBER_BOUND}"));
        self.emit("sltu t1, a0, t0");
        self.emit(format!("beqz t1, {invalid}"));

        match operation {
            "duration" => {}
            "add" | "sub" => {
                self.emit("sltu t1, a1, t0");
                self.emit(format!("beqz t1, {invalid}"));
                if operation == "add" {
                    self.emit("add t2, a0, a1");
                    self.emit("sltu t1, t2, a0");
                    self.emit(format!("bnez t1, {invalid}"));
                    self.emit("sltu t1, t2, t0");
                    self.emit(format!("beqz t1, {invalid}"));
                    self.emit_add_immediate("a0", "t2", 0);
                } else {
                    self.emit("sltu t1, a0, a1");
                    self.emit(format!("bnez t1, {invalid}"));
                    self.emit("sub a0, a0, a1");
                }
            }
            _ => unreachable!("known CKB epoch arithmetic operation"),
        }

        self.emit("li a1, 0");
        self.emit(format!("j {done}"));
        self.emit_label(&invalid);
        self.emit("li a0, 0");
        self.emit(format!("li a1, {}", CellScriptRuntimeError::NumericOrDiscriminantInvalid.code()));
        self.emit_label(&done);
        self.emit("ret");
    }

    pub(super) fn emit_runtime_ckb_temporal_to_raw(&mut self, symbol: &str, detail: &str) {
        self.emit_global(symbol);
        self.emit_label(symbol);
        self.emit(format!("# cellscript abi: {detail}; preserves the exact 64-bit representation"));
        self.emit("li a1, 0");
        self.emit("ret");
    }
}

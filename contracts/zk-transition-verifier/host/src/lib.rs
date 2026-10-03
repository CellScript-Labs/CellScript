//! Research harness only. The deterministic setup and identity circuit below
//! test the cryptographic transport. They are not an authorization circuit,
//! trusted setup ceremony, production VK, or stateful transaction evidence.

#[path = "../../../../crates/cellscript-artifact-checker/src/zk.rs"]
pub mod wire;

#[cfg(test)]
mod tests {
    use super::wire;
    use ark_bn254::{Bn254, Fr};
    use ark_groth16::Groth16;
    use ark_relations::{
        lc,
        r1cs::{ConstraintSynthesizer, ConstraintSystemRef, SynthesisError, Variable},
    };
    use ark_serialize::{CanonicalDeserialize, CanonicalSerialize};
    use ark_snark::SNARK;
    use ark_std::rand::{rngs::StdRng, SeedableRng};
    use ckb_vm::{
        cost_model::estimate_cycles,
        machine::VERSION2,
        registers::{A0, A1, A2, A3, A4, A5, A7},
        CoreMachine, DefaultCoreMachine, DefaultMachineBuilder, DefaultMachineRunner, Error, Memory, SparseMemory, SupportMachine,
        Syscalls, TraceMachine, WXorXMemory, ISA_B, ISA_IMC, ISA_MOP,
    };

    #[derive(Clone)]
    struct BindingCircuit(Vec<Fr>);
    impl ConstraintSynthesizer<Fr> for BindingCircuit {
        fn generate_constraints(self, cs: ConstraintSystemRef<Fr>) -> Result<(), SynthesisError> {
            for value in self.0 {
                let public = cs.new_input_variable(|| Ok(value))?;
                let private = cs.new_witness_variable(|| Ok(value))?;
                cs.enforce_constraint(lc!() + private, lc!() + Variable::One, lc!() + public)?;
            }
            Ok(())
        }
    }
    fn serialize(value: &impl CanonicalSerialize) -> Vec<u8> {
        let mut bytes = Vec::new();
        value.serialize_compressed(&mut bytes).unwrap();
        bytes
    }
    fn hash(bytes: &[u8]) -> [u8; 32] {
        let mut out = [0; 32];
        let mut h = blake2b_ref::Blake2bBuilder::new(32).personal(b"ckb-default-hash").build();
        h.update(bytes);
        h.finalize(&mut out);
        out
    }
    fn fixture() -> (Vec<u8>, wire::Request) {
        let statement = wire::Statement {
            domain: [1; 32],
            action: [2; 32],
            script_hash: [3; 32],
            old_data_hash: [4; 32],
            new_data_hash: [5; 32],
            input_transaction_hash: [6; 32],
            input_output_index: 7,
        };
        let pi = statement.public_inputs();
        let inputs = pi[4..].chunks_exact(32).map(|bytes| Fr::deserialize_compressed(bytes).unwrap()).collect();
        let circuit = BindingCircuit(inputs);
        // Public seed is deliberately unsuitable for production. Test fixture only.
        let mut rng = StdRng::seed_from_u64(0x325a4b);
        let (pk, vk) = Groth16::<Bn254>::circuit_specific_setup(circuit.clone(), &mut rng).unwrap();
        let proof = Groth16::<Bn254>::prove(&pk, circuit, &mut rng).unwrap();
        let key = serialize(&vk);
        assert_eq!(key.len(), wire::VK_BYTES);
        let request = wire::Request { verification_key: hash(&key), proof: serialize(&proof).try_into().unwrap(), statement };
        (key, request)
    }

    // Executes the exact child ELF with explicitly modeled transport/CellDep
    // syscalls. This isolates child cryptography; it is not a CKB scheduler or
    // consensus transaction test, and does not include scheduler syscall fees.
    type Core = DefaultCoreMachine<u64, WXorXMemory<SparseMemory<u64>>>;
    struct Transport {
        request: Vec<u8>,
        position: usize,
        key: Vec<u8>,
        fd_count: u64,
        partial: bool,
        closed: bool,
    }
    impl Syscalls<Core> for Transport {
        fn initialize(&mut self, _: &mut Core) -> Result<(), Error> {
            Ok(())
        }
        fn ecall(&mut self, m: &mut Core) -> Result<bool, Error> {
            let [a0, a1, a2, a3, a4, a5] = [A0, A1, A2, A3, A4, A5].map(|r| m.registers()[r]);
            let result = match m.registers()[A7] {
                2607 => {
                    m.memory_mut().store64(&a0, &2)?;
                    m.memory_mut().store64(&a1, &self.fd_count)?;
                    0
                }
                2606 => {
                    if a0 != 2 || self.closed {
                        6
                    } else if self.position == self.request.len() {
                        7
                    } else {
                        let capacity = m.memory_mut().load64(&a2)? as usize;
                        let count = capacity.min(self.request.len() - self.position).min(if self.partial { 7 } else { usize::MAX });
                        m.memory_mut().store_bytes(a1, &self.request[self.position..self.position + count])?;
                        m.memory_mut().store64(&a2, &(count as u64))?;
                        self.position += count;
                        0
                    }
                }
                2608 => {
                    if a0 != 2 || self.closed {
                        6
                    } else {
                        self.closed = true;
                        0
                    }
                }
                2081 | 2092 => {
                    if a3 != 0 || a4 != 3 {
                        1
                    } else {
                        let bytes = if m.registers()[A7] == 2081 {
                            assert_eq!(a5, 1);
                            hash(&self.key).to_vec()
                        } else {
                            self.key.clone()
                        };
                        let offset = (a2 as usize).min(bytes.len());
                        let rest = &bytes[offset..];
                        let capacity = m.memory_mut().load64(&a1)? as usize;
                        m.memory_mut().store_bytes(a0, &rest[..rest.len().min(capacity)])?;
                        m.memory_mut().store64(&a1, &(rest.len() as u64))?;
                        if rest.len() > capacity {
                            3
                        } else {
                            0
                        }
                    }
                }
                _ => return Ok(false),
            };
            m.set_register(A0, result);
            Ok(true)
        }
    }
    fn run(request: Vec<u8>, key: Vec<u8>, fd_count: u64, partial: bool) -> (i8, u64) {
        let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../target/riscv64imac-unknown-none-elf/release/cellscript-zk-transition-verifier");
        let elf = std::fs::read(path).expect("build the pinned RISC-V child before running its harness");
        let core = Core::new(ISA_IMC | ISA_B | ISA_MOP, VERSION2, 250_000_000);
        let mut vm = TraceMachine::new(
            DefaultMachineBuilder::new(core)
                .instruction_cycle_func(Box::new(estimate_cycles))
                .syscall(Box::new(Transport { request, position: 0, key, fd_count, partial, closed: false }))
                .build(),
        );
        vm.load_program(&elf.into(), std::iter::empty::<Result<ckb_vm::Bytes, Error>>()).unwrap();
        let exit = vm.run().expect("bounded child execution");
        (exit, vm.machine.cycles())
    }
    #[test]
    fn real_pairing_binds_every_statement_byte_and_strict_codecs() {
        let (key, request) = fixture();
        assert_eq!(verifier_core::verify(&key, &request.proof, &request.statement.public_inputs()), Ok(()));
        for index in 0..wire::STATEMENT_BYTES {
            let mut bytes = request.statement.encode();
            bytes[index] ^= 1;
            let pi = wire::Statement::decode(&bytes).unwrap().public_inputs();
            assert!(verifier_core::verify(&key, &request.proof, &pi).is_err(), "unbound byte {index}");
        }
        let pi = request.statement.public_inputs();
        for length in [0, 1, 127, 129] {
            let mut proof = request.proof.to_vec();
            proof.resize(length, 0);
            assert!(verifier_core::verify(&key, &proof, &pi).is_err());
        }
        let mut noncanonical = pi;
        noncanonical[4..36].fill(255);
        assert!(verifier_core::verify(&key, &request.proof, &noncanonical).is_err());
        let mut oversized = key.clone();
        oversized[224..232].copy_from_slice(&u64::MAX.to_le_bytes());
        assert!(verifier_core::verify(&oversized, &request.proof, &pi).is_err());
    }
    #[test]
    fn exact_riscv_child_checks_pairing_and_transport_failures() {
        let (key, request) = fixture();
        let canonical = request.encode();
        let mut rows = Vec::new();
        let (exit, cycles) = run(canonical.to_vec(), key.clone(), 1, false);
        assert_eq!(exit, 0);
        rows.push(serde_json::json!({"case":"valid","exit":exit,"instruction_cycles":cycles}));
        for index in [236, 268, 300, 332, 364, 396, 428] {
            let mut bytes = canonical;
            bytes[index] ^= 1;
            let (exit, cycles) = run(bytes.to_vec(), key.clone(), 1, false);
            assert_eq!(exit, 83, "statement field at {index}");
            rows.push(serde_json::json!({"case":format!("changed-statement-{index}"),"exit":exit,"instruction_cycles":cycles}));
        }
        for (name, bytes, fd_count, partial, expected) in [
            ("truncated", canonical[..431].to_vec(), 1, false, 80),
            ("trailing", [canonical.as_slice(), &[0]].concat(), 1, false, 80),
            ("missing-fd", canonical.to_vec(), 0, false, 80),
            ("extra-fd", canonical.to_vec(), 2, false, 80),
            ("partial-read", canonical.to_vec(), 1, true, 80),
        ] {
            let (exit, cycles) = run(bytes, key.clone(), fd_count, partial);
            assert_eq!(exit, expected);
            rows.push(serde_json::json!({"case":name,"exit":exit,"instruction_cycles":cycles}));
        }
        let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("target/child-evidence.json");
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(
            path,
            serde_json::to_vec_pretty(&serde_json::json!({"schema":"cellscript-zk-child-research-v1",
            "scope":"exact child ELF with modeled syscalls; excludes CKB scheduler charges and transaction binding",
            "setup":"public deterministic test-only seed; no production authorization circuit",
            "ckb_vm":"0.24.14","vm_version":2,"isa":"IMC|B|MOP","rows":rows}))
            .unwrap(),
        )
        .unwrap();
    }
}

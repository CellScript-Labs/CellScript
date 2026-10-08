//! Checked client for the fixed two-parent migration application.
//! The trusted manifest is an application decision, not an admission certificate.
//! Supplied Cells must come from the client's checked chain provider; byte and
//! OutPoint correspondence here does not establish liveness or confirmation.
use crate::{hash32, parent, Deployment, PreparedIncrement};
use anyhow::{ensure, Context, Result};
use cellscript::zk_client::TransitionParent;
use cellscript_zk_private_counter::{self as counter, package, wire};
use ckb_types::{bytes::Bytes, core::TransactionView, packed, prelude::*};
use serde::{Deserialize, Serialize};

const MAX_CELLS: usize = 64;
const MAX_TX_BYTES: usize = 16_384;
pub type ResolvedCell = (packed::OutPoint, packed::CellOutput, Bytes);

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CodeCell {
    pub tx_hash: String,
    pub index: u32,
    pub data_hash: String,
}
impl CodeCell {
    pub fn out_point(&self) -> Result<packed::OutPoint> {
        Ok(packed::OutPoint::new_builder().tx_hash(hash32(&self.tx_hash, "code tx_hash")?).index(self.index).build())
    }
    fn check_bytes(&self, bytes: &[u8], name: &str) -> Result<()> {
        self.out_point()?;
        ensure!(hash32(&self.data_hash, name)? == counter::hash(bytes), "migration {name} data hash mismatch");
        Ok(())
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Version {
    pub verifier: Deployment,
    pub parent: CodeCell,
    pub key: CodeCell,
    pub setup: package::Manifest,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Manifest {
    pub schema: String,
    pub chain_id: String,
    pub genesis_hash: String,
    pub lifecycle: CodeCell,
    pub config_guard: CodeCell,
    pub instance: InstanceScripts,
    pub versions: [Version; 2],
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct InstanceScripts {
    /// Complete canonical Molecule Script bytes, encoded as hex (not a hash).
    pub counter: String,
    pub configuration: String,
}

pub struct Artifacts<'a> {
    pub lifecycle: &'a [u8],
    pub config_guard: &'a [u8],
    pub children: [&'a [u8]; 2],
    pub keys: [&'a [u8]; 2],
}

/// Immutable after checking the externally trusted manifest digest, network,
/// actual code/VK bytes and exact generated parents. This does not certify the
/// setup operator, code review or production eligibility of those artifacts.
pub struct CheckedDeployment {
    manifest: Manifest,
    parents: [TransitionParent; 2],
    keys: [Vec<u8>; 2],
    parent_hashes: [[u8; 32]; 2],
    lifecycle_hash: [u8; 32],
    config_hash: [u8; 32],
    instance: Instance,
}

impl CheckedDeployment {
    pub fn load(
        manifest_bytes: &[u8],
        trusted_manifest_hash: [u8; 32],
        observed_chain_id: &str,
        observed_genesis: [u8; 32],
        artifacts: Artifacts<'_>,
    ) -> Result<Self> {
        ensure!(manifest_bytes.len() <= 32_768, "migration manifest exceeds 32768 bytes");
        ensure!(counter::hash(manifest_bytes) == trusted_manifest_hash, "migration manifest differs from trusted digest");
        let manifest: Manifest = serde_json::from_slice(manifest_bytes)?;
        ensure!(manifest.schema == "cellscript-counter-migration-v1", "unsupported migration manifest schema");
        ensure!(
            !manifest.chain_id.is_empty()
                && manifest.chain_id == observed_chain_id
                && hash32(&manifest.genesis_hash, "genesis_hash")? == observed_genesis,
            "migration network differs from observed chain/genesis"
        );
        ensure!(
            [artifacts.lifecycle, artifacts.config_guard].iter().all(|bytes| !bytes.is_empty() && bytes.len() <= 131_072),
            "migration lifecycle/config code exceeds bounds"
        );
        manifest.lifecycle.check_bytes(artifacts.lifecycle, "lifecycle")?;
        manifest.config_guard.check_bytes(artifacts.config_guard, "config guard")?;
        let circuit = counter::circuit_identity()?;
        let mut parents = Vec::new();
        for (i, version) in manifest.versions.iter().enumerate() {
            ensure!(
                version.verifier.chain_id == manifest.chain_id
                    && hash32(&version.verifier.genesis_hash, "parent genesis")? == observed_genesis,
                "migration parent network mismatch"
            );
            ensure!(
                artifacts.keys[i].len() == wire::VK_BYTES
                    && !artifacts.children[i].is_empty()
                    && artifacts.children[i].len() <= 4 * 1024 * 1024,
                "migration child/VK bytes exceed bounds"
            );
            ensure!(
                version.setup.schema == "cellscript-counter-setup-v1"
                    && !version.setup.production_admitted
                    && version.setup.circuit == circuit
                    && version.setup.rust_toolchain == "1.97.1"
                    && version.setup.dependency_lock_sha256
                        == package::sha256(include_bytes!("../../../contracts/zk-private-counter/Cargo.lock"))
                    && version.setup.verification_key_sha256 == package::sha256(artifacts.keys[i])
                    && hash32(&version.setup.verification_key_data_hash, "setup VK")? == counter::hash(artifacts.keys[i]),
                "migration setup circuit/toolchain/VK identity mismatch or self-admission"
            );
            version.key.check_bytes(artifacts.keys[i], "VK")?;
            let parent = parent(&version.verifier, artifacts.children[i], artifacts.keys[i])?;
            version.parent.check_bytes(cellscript::strip_vm_abi_trailer(&parent.compiled.artifact_bytes), "parent")?;
            parents.push(parent);
        }
        let parent_hashes =
            [hash32(&manifest.versions[0].parent.data_hash, "parent 0")?, hash32(&manifest.versions[1].parent.data_hash, "parent 1")?];
        ensure!(
            parent_hashes[0] != parent_hashes[1] && parent_hashes.iter().all(|hash| *hash != [0; 32]),
            "migration parents must be distinct and nonzero"
        );
        ensure!(artifacts.keys[0] != artifacts.keys[1], "migration requires distinct verification keys");
        let parents = parents.try_into().map_err(|_| anyhow::anyhow!("two migration parents required"))?;
        let instance = Instance {
            counter: packed::Script::from_slice(&hex::decode(&manifest.instance.counter)?)
                .context("migration manifest counter Script")?,
            config: packed::Script::from_slice(&hex::decode(&manifest.instance.configuration)?)
                .context("migration manifest configuration Script")?,
        };
        let deployment = Self {
            lifecycle_hash: counter::hash(artifacts.lifecycle),
            config_hash: counter::hash(artifacts.config_guard),
            keys: artifacts.keys.map(<[u8]>::to_vec),
            manifest,
            parents,
            parent_hashes,
            instance,
        };
        deployment.bind_instance(deployment.instance.counter.clone(), deployment.instance.config.clone())?;
        Ok(deployment)
    }

    /// Output indices are global. The creation builder must retain this first
    /// input and these indices when completing fees; otherwise regenerate both.
    pub fn instance(&self, first: &packed::CellInput, counter_index: u64, config_index: u64) -> Result<Instance> {
        let instance = self.derive_instance(first, counter_index, config_index)?;
        self.bind_instance(instance.counter, instance.config)
    }

    fn derive_instance(&self, first: &packed::CellInput, counter_index: u64, config_index: u64) -> Result<Instance> {
        ensure!(
            counter_index < 64 && config_index < 64 && counter_index != config_index,
            "migration creation indices must be distinct and below 64"
        );
        let type_id = |index: u64| {
            let mut bytes = first.as_slice().to_vec();
            bytes.extend(index.to_le_bytes());
            counter::hash(&bytes)
        };
        let mut args = type_id(config_index).to_vec();
        args.extend(type_id(counter_index));
        args.extend(self.lifecycle_hash);
        args.extend(self.parent_hashes[0]);
        args.extend(self.parent_hashes[1]);
        let config = data2_script(self.config_hash, args);
        let mut args = type_id(counter_index).to_vec();
        args.extend(config.calc_script_hash().as_slice());
        Ok(Instance { counter: data2_script(self.lifecycle_hash, args), config })
    }

    pub fn bind_instance(&self, counter: packed::Script, config: packed::Script) -> Result<Instance> {
        ensure!(
            counter == self.instance.counter && config == self.instance.config,
            "migration instance differs from pinned code or trusted instance Scripts"
        );
        let args = config.args().raw_data();
        let counter_args = counter.args().raw_data();
        ensure!(args.len() == 160 && counter_args.len() == 64, "migration instance args have incorrect widths");
        ensure!(
            config == data2_script(self.config_hash, args.to_vec())
                && counter == data2_script(self.lifecycle_hash, counter_args.to_vec())
                && args[32..64] == counter_args[..32]
                && args[64..96] == self.lifecycle_hash
                && args[96..128] == self.parent_hashes[0]
                && args[128..160] == self.parent_hashes[1]
                && counter_args[32..] == *config.calc_script_hash().as_slice(),
            "migration instance differs from pinned code, parents or paired Script identity"
        );
        Ok(Instance { counter, config })
    }

    pub fn config_data(&self, selector: u8) -> Result<Bytes> {
        ensure!(selector <= 1, "migration selector must be 0 or 1");
        let mut bytes = b"CSZKCFG1".to_vec();
        bytes.push(selector);
        bytes.extend(self.parent_hashes[usize::from(selector)]);
        Ok(Bytes::from(bytes))
    }

    /// Check the final creation transaction after fees/dependencies are complete.
    /// Recompute both Type IDs instead of trusting a pre-fee builder's indices.
    pub fn check_creation(
        &self,
        tx: &TransactionView,
        inputs: &[ResolvedCell],
        dependency_cells: &[ResolvedCell],
        instance: &Instance,
    ) -> Result<()> {
        bounds(tx)?;
        check_inputs(tx, inputs)?;
        self.bind_instance(instance.counter.clone(), instance.config.clone())?;
        ensure!(
            matching(inputs, &instance.counter).is_empty() && matching(inputs, &instance.config).is_empty(),
            "migration creation cannot consume an existing instance"
        );
        let output_index = |script: &packed::Script| -> Result<usize> {
            let matches: Vec<_> = tx
                .outputs()
                .into_iter()
                .enumerate()
                .filter(|(_, cell)| cell.type_().to_opt().as_ref() == Some(script))
                .map(|(i, _)| i)
                .collect();
            let [index] = matches.as_slice() else {
                anyhow::bail!("migration creation requires exactly one output per paired Script");
            };
            Ok(*index)
        };
        let counter_index = output_index(&instance.counter)?;
        let config_index = output_index(&instance.config)?;
        let expected = self.derive_instance(
            &tx.inputs().get(0).context("migration creation input missing")?,
            counter_index as u64,
            config_index as u64,
        )?;
        ensure!(
            expected.counter == instance.counter && expected.config == instance.config,
            "migration creation first input/output indices changed; regenerate both Type IDs"
        );
        let state = tx.outputs_data().get(counter_index).context("migration counter output data missing")?.raw_data();
        ensure!(
            state.len() == 48 && &state[..8] == b"CSZKCNT1" && state[40..] == [0; 8],
            "migration creation counter must start at zero with canonical state"
        );
        ensure!(
            tx.outputs_data().get(config_index).context("migration config output data missing")?.raw_data() == self.config_data(0)?,
            "migration creation configuration must select parent 0"
        );
        let deps = expand_dependencies(tx, dependency_cells)?;
        ensure!(
            !deps.iter().any(|cell| cell.1.type_().to_opt().as_ref() == Some(&instance.config)),
            "migration creation configuration cannot also be a dependency"
        );
        for (code, name) in [(&self.manifest.lifecycle, "lifecycle"), (&self.manifest.config_guard, "config guard")] {
            let point = code.out_point()?;
            let matches: Vec<_> = deps.iter().filter(|cell| cell.0 == point).collect();
            ensure!(matches.len() == 1, "migration creation {name} dependency missing or duplicated");
            code.check_bytes(&matches[0].2, name)?;
        }
        Ok(())
    }

    pub fn prepare<'a>(
        &'a self,
        transaction: TransactionView,
        inputs: &[ResolvedCell],
        dependency_cells: &[ResolvedCell],
        instance: &Instance,
    ) -> Result<PreparedTransition<'a>> {
        self.bind_instance(instance.counter.clone(), instance.config.clone())?;
        bounds(&transaction)?;
        check_inputs(&transaction, inputs)?;
        let deps = expand_dependencies(&transaction, dependency_cells)?;
        for dep in &deps {
            if let Some(input) = inputs.iter().find(|input| input.0 == dep.0) {
                ensure!(input == *dep, "migration input/dependency observations disagree");
            }
        }
        let outputs: Vec<_> = transaction
            .outputs()
            .into_iter()
            .zip(transaction.outputs_data())
            .map(|(cell, data)| (packed::OutPoint::default(), cell, data.raw_data()))
            .collect();
        let config_inputs = matching(inputs, &instance.config);
        let config_outputs = matching(&outputs, &instance.config);
        let config_deps: Vec<_> =
            deps.iter().copied().filter(|cell| cell.1.type_().to_opt().as_ref() == Some(&instance.config)).collect();
        let (old_config, migrating) = match (config_inputs.as_slice(), config_outputs.as_slice(), config_deps.as_slice()) {
            ([], [], [dep]) => (*dep, false),
            ([input], [output], []) => {
                ensure!(
                    input.1.lock() == output.1.lock() && input.1.capacity() == output.1.capacity(),
                    "migration configuration Lock/capacity changed"
                );
                ensure!(
                    input.2 == self.config_data(0)? && output.2 == self.config_data(1)?,
                    "migration requires exactly one 0 -> 1 transition"
                );
                (*input, true)
            }
            _ => anyhow::bail!("migration requires one configuration dependency or one input/output pair, without overlap"),
        };
        let selector = old_config.2.get(8).copied().context("migration configuration is truncated")?;
        ensure!(old_config.2 == self.config_data(selector)?, "migration configuration magic/parent mismatch");
        let selected = usize::from(selector);
        let version = &self.manifest.versions[selected];
        let child_point = packed::OutPoint::new_builder()
            .tx_hash(hash32(&version.verifier.child_tx_hash, "child tx_hash")?)
            .index(version.verifier.child_index)
            .build();
        ensure!(
            deps.first().is_some_and(|cell| cell.0 == child_point),
            "migration selected verifier child must occupy resolved CellDep slot 0"
        );
        let mut required = vec![
            (child_point, hash32(&version.verifier.child_data_hash, "child hash")?, "child"),
            (version.key.out_point()?, hash32(&version.key.data_hash, "key hash")?, "VK"),
            (version.parent.out_point()?, self.parent_hashes[selected], "parent"),
            (self.manifest.lifecycle.out_point()?, self.lifecycle_hash, "lifecycle"),
        ];
        if migrating {
            required.push((self.manifest.config_guard.out_point()?, self.config_hash, "config guard"));
        }
        for (point, hash, label) in required {
            let found: Vec<_> = deps.iter().filter(|cell| counter::hash(&cell.2) == hash).collect();
            ensure!(
                found.len() == 1 && found[0].0 == point,
                "migration {label} dependency is missing, ambiguous or at another OutPoint"
            );
        }
        let increment = PreparedIncrement::new(transaction, inputs, &instance.counter)?;
        Ok(PreparedTransition { deployment: self, selected, migrating, increment })
    }
}

pub struct Instance {
    counter: packed::Script,
    config: packed::Script,
}
impl Instance {
    pub fn counter(&self) -> &packed::Script {
        &self.counter
    }
    pub fn config(&self) -> &packed::Script {
        &self.config
    }
}

pub struct PreparedTransition<'a> {
    deployment: &'a CheckedDeployment,
    selected: usize,
    migrating: bool,
    increment: PreparedIncrement,
}
impl PreparedTransition<'_> {
    pub fn selected_version(&self) -> usize {
        self.selected
    }
    pub fn is_migration(&self) -> bool {
        self.migrating
    }
    pub fn statement(&self) -> &wire::Statement {
        self.increment.statement()
    }
    pub fn attach_proof(&mut self, proof: &[u8]) -> Result<TransactionView> {
        let parent = &self.deployment.parents[self.selected];
        let transaction =
            self.increment.attach_proof(&parent.compiled, &parent.handle, &self.deployment.keys[self.selected], proof)?;
        if let Err(error) = bounds(&transaction) {
            self.increment.proved = None;
            return Err(error);
        }
        Ok(transaction)
    }
    pub fn check_signed(&self, proved: &TransactionView, signed: &TransactionView) -> Result<()> {
        bounds(signed)?;
        self.increment.check_signed(proved, signed)
    }
}

fn data2_script(hash: [u8; 32], args: Vec<u8>) -> packed::Script {
    packed::Script::new_builder().code_hash(hash).hash_type(4u8).args(Bytes::from(args).pack()).build()
}
fn bounds(tx: &TransactionView) -> Result<()> {
    ensure!(
        tx.inputs().len() <= MAX_CELLS && tx.outputs().len() <= MAX_CELLS && tx.cell_deps().len() <= MAX_CELLS,
        "migration transaction exceeds 64 Cells/dependencies"
    );
    ensure!(tx.outputs().len() == tx.outputs_data().len(), "migration output/data count mismatch");
    ensure!(tx.data().as_slice().len() <= MAX_TX_BYTES, "migration transaction exceeds 16384 bytes including witnesses");
    Ok(())
}
fn matching<'a>(cells: &'a [ResolvedCell], script: &packed::Script) -> Vec<&'a ResolvedCell> {
    cells.iter().filter(|cell| cell.1.type_().to_opt().as_ref() == Some(script)).collect()
}

fn check_inputs(tx: &TransactionView, inputs: &[ResolvedCell]) -> Result<()> {
    ensure!(inputs.len() == tx.inputs().len(), "migration resolved input count mismatch");
    for (i, (input, cell)) in tx.inputs().into_iter().zip(inputs).enumerate() {
        ensure!(
            input.previous_output() == cell.0 && !inputs[..i].iter().any(|other| other.0 == cell.0),
            "migration input resolution mismatch or duplicate input"
        );
    }
    Ok(())
}

/// Preserve raw dependency order and duplicates, including one-level dep-group
/// expansion, like the pinned node resolver. Dep-group Cells are not themselves
/// in the resolved CellDep view. Node freshness remains a separate requirement.
fn expand_dependencies<'a>(tx: &TransactionView, cells: &'a [ResolvedCell]) -> Result<Vec<&'a ResolvedCell>> {
    ensure!(cells.len() <= 128, "migration dependency observations exceed bounds");
    let mut bytes = 0usize;
    for (i, cell) in cells.iter().enumerate() {
        bytes = bytes.checked_add(cell.2.len()).context("migration dependency byte overflow")?;
        ensure!(bytes <= 16 * 1024 * 1024, "migration dependency observations exceed 16 MiB");
        ensure!(!cells[..i].iter().any(|other| other.0 == cell.0), "migration duplicate dependency observation");
    }
    let find =
        |point: &packed::OutPoint| cells.iter().find(|cell| cell.0 == *point).context("migration dependency observation missing");
    let mut result = Vec::new();
    for dep in tx.cell_deps() {
        let cell = find(&dep.out_point())?;
        match u8::from(dep.dep_type()) {
            0 => {
                ensure!(result.len() < MAX_CELLS, "migration resolved dependencies exceed 64");
                result.push(cell);
            }
            1 => {
                ensure!(cell.2.len() <= 4 + MAX_CELLS * 36, "migration dep-group exceeds bounds");
                let points = packed::OutPointVec::from_slice(&cell.2).context("migration malformed dep-group")?;
                ensure!(
                    !points.is_empty() && result.len() + points.len() <= MAX_CELLS,
                    "migration resolved dependencies exceed 64 or empty dep-group"
                );
                for point in points {
                    result.push(find(&point)?);
                }
            }
            _ => anyhow::bail!("migration invalid dependency type"),
        }
    }
    Ok(result)
}

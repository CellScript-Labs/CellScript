use super::{
    backend::{cell, copy_package, Backend, Cell, Program, CELL_CAPACITY},
    quote,
};
use anyhow::{Context, Result};
use cellscript::EntryWitnessArg as Arg;
use ckb_types::{bytes::Bytes, packed, prelude::*};
use std::{fs, path::Path};

pub struct Programs {
    pub asset: Program,
    pub reserve: Program,
    pub lp: Program,
    pub minimum: Program,
    pub pool: Program,
    pub escrow: Program,
    pub order: Option<Program>,
    pub route: Program,
    pub settlement: Option<Program>,
    pub allocation: Option<Program>,
}
impl Programs {
    pub fn compile(root: &Path, run: &Path, cellc: &Path, backend: &Backend, include_orders: bool) -> Result<Self> {
        let packages = run.join("packages");
        copy_package(&root.join("examples/finance"), &packages.join("finance"))?;
        copy_package(&root.join("examples/finance_math"), &packages.join("finance_math"))?;
        let artifacts = run.join("artifacts");
        let compile = |name: &str, action| {
            Program::compile(&packages.join("finance").join(name), name, action, cellc, &backend.chain, &backend.genesis, &artifacts)
        };
        let asset = compile("asset", true)?;
        let reserve = compile("reserve", false)?;
        let lp = compile("lp", true)?;
        let minimum = compile("minimum", false)?;
        let escrow = compile("escrow", false)?;
        let mut configuration = vec![
            ("asset_code_hash", ckb_hash::blake2b_256(asset.code())),
            ("reserve_code_hash", ckb_hash::blake2b_256(reserve.code())),
            ("lp_code_hash", ckb_hash::blake2b_256(lp.code())),
            ("minimum_code_hash", ckb_hash::blake2b_256(minimum.code())),
            ("escrow_code_hash", ckb_hash::blake2b_256(escrow.code())),
            ("allocation_code_hash", [0; 32]),
            ("network_domain", backend.network_domain),
        ];
        let write_configuration = |values: &[(&str, [u8; 32])]| -> Result<()> {
            let mut source = String::from("module finance_deployment;\n");
            for (name, hash) in values {
                let literal = hash.iter().map(|byte| format!("\\x{byte:02x}")).collect::<String>();
                source.push_str(&format!("public fn {name}() -> Hash {{ return Hash::from_bytes(b\"{literal}\") }}\n"));
            }
            fs::write(packages.join("finance/deployment/src/lib.cell"), source)?;
            Ok(())
        };
        write_configuration(&configuration)?;
        let allocation = if include_orders {
            let preliminary = compile("allocation", false)?;
            configuration
                .iter_mut()
                .find(|(name, _)| *name == "allocation_code_hash")
                .context("allocation configuration missing")?
                .1 = ckb_hash::blake2b_256(preliminary.code());
            write_configuration(&configuration)?;
            let actual = compile("allocation", false)?;
            if preliminary.code() != actual.code() {
                anyhow::bail!("allocation self-hash changes its ELF; no cyclic deployment admitted");
            }
            Some(actual)
        } else {
            None
        };
        let pool = compile("pool", false)?;
        let order = include_orders.then(|| compile("order", false)).transpose()?;
        let route = compile("route", false)?;
        let settlement = include_orders.then(|| compile("settlement", false)).transpose()?;
        Ok(Self { asset, reserve, lp, minimum, pool, escrow, order, route, settlement, allocation })
    }
    pub fn all(&self) -> Vec<&Program> {
        let mut programs = vec![&self.asset, &self.reserve, &self.lp, &self.minimum, &self.pool, &self.escrow, &self.route];
        programs.extend(self.order.as_ref());
        programs.extend(self.settlement.as_ref());
        programs.extend(self.allocation.as_ref());
        programs
    }
}

pub struct Wallet {
    pub key: u8,
    pub funding: Vec<packed::OutPoint>,
    pub a: packed::OutPoint,
    pub b: packed::OutPoint,
    pub lp: Option<packed::OutPoint>,
}
pub struct Pool {
    pub points: Vec<packed::OutPoint>,
    pub identity: packed::Script,
    pub controller: packed::Script,
    pub assets: [packed::Script; 2],
    pub lp: packed::Script,
    pub minimum: packed::Script,
    pub state: quote::PoolState,
    pub nonce: u64,
    pub revision: u64,
}
impl Wallet {
    pub fn funding(&mut self) -> Result<packed::OutPoint> {
        self.funding.pop().context("no remaining real funding Cell")
    }
}

pub fn bootstrap(programs: &Programs, backend: &mut Backend) -> Result<(Wallet, Wallet)> {
    let primary = backend.key_labels[0];
    let secondary = backend.key_labels[1];
    let mut outputs = Vec::new();
    let initial_programs =
        programs.all().into_iter().filter(|p| !matches!(p.name.as_str(), "settlement" | "allocation")).collect::<Vec<_>>();
    for program in &initial_programs {
        let mut output = cell(backend.owner(primary), None, program.code().to_vec());
        let capacity = output.0.occupied_capacity(ckb_types::core::Capacity::bytes(output.1.len())?)?.as_u64() + CELL_CAPACITY;
        output.0 = output.0.as_builder().capacity(capacity).build();
        outputs.push(output);
    }
    let first_funding = outputs.len();
    for _ in 0..80 {
        let mut output = cell(backend.owner(primary), None, vec![]);
        output.0 = output.0.as_builder().capacity(10 * CELL_CAPACITY).build();
        outputs.push(output);
    }
    let secondary_index = outputs.len();
    for _ in 0..30 {
        let mut output = cell(backend.owner(secondary), None, vec![]);
        output.0 = output.0.as_builder().capacity(10 * CELL_CAPACITY).build();
        outputs.push(output);
    }
    let secondary_end = outputs.len();
    outputs.push(cell(backend.owner(primary), None, vec![]));
    let minimum = outputs.iter().map(|cell| u64::from(cell.0.capacity())).sum::<u64>() + super::backend::FEE;
    let funds = backend.node.collect_spendable(minimum)?;
    let mut inputs = vec![];
    for value in funds["cells"].as_array().context("mined bootstrap cells missing")? {
        let point = super::backend::point(value)?;
        backend.refresh(&point)?;
        inputs.push(point);
    }
    let change = outputs.len() - 1;
    let tx = backend.build(&inputs, outputs, change, vec![])?;
    let points = backend.commit(&tx, "disposable-miner-bootstrap-code-deployment-and-untyped-signed-wallet-funding")?;
    for (index, program) in initial_programs.iter().enumerate() {
        backend.code_points.insert(program.name.clone(), points[index].clone());
        backend.deps.push(packed::CellDep::new_builder().out_point(points[index].clone()).build());
    }
    let funding = points[first_funding..secondary_index].to_vec();
    Ok((
        Wallet { key: primary, funding, a: packed::OutPoint::default(), b: packed::OutPoint::default(), lp: None },
        Wallet {
            key: secondary,
            funding: points[secondary_index..secondary_end].to_vec(),
            a: packed::OutPoint::default(),
            b: packed::OutPoint::default(),
            lp: None,
        },
    ))
}

pub fn token(programs: &Programs, backend: &Backend, asset: &packed::Script, key: u8, value: u128) -> Result<Cell> {
    Ok(cell(backend.owner(key), Some(asset.clone()), programs.asset.data("AssetData", &[("amount", value.to_le_bytes().to_vec())])?))
}

pub fn issue_assets(
    programs: &Programs,
    backend: &mut Backend,
    wallet: &mut Wallet,
    secondary: &mut Wallet,
) -> Result<[packed::Script; 2]> {
    let assets = [
        programs.asset.script(backend.owner(wallet.key).calc_script_hash().as_slice()),
        programs.asset.script(backend.owner(backend.key_labels[1]).calc_script_hash().as_slice()),
    ];
    for (index, asset) in assets.iter().enumerate() {
        let issuer = backend.key_labels[index];
        let funding = if index == 0 { wallet.funding()? } else { secondary.funding()? };
        let outputs = vec![token(programs, backend, asset, wallet.key, 1_000_000)?, cell(backend.owner(issuer), None, vec![])];
        let unsigned = backend.build(&[funding], outputs, 1, vec![programs.asset.witness(&[])?])?;
        let tx = backend.sign(&unsigned)?;
        let points = backend.commit(&tx, &format!("genuine-enforcing-asset-{index}-issuance-with-upstream-issuer-signature"))?;
        if index == 0 {
            wallet.a = points[0].clone();
        } else {
            wallet.b = points[0].clone();
        }
    }
    Ok(assets)
}

pub fn pool_data(programs: &Programs, backend: &Backend, pool: &Pool, state: quote::PoolState, revision: u64) -> Result<Vec<u8>> {
    programs.pool.data(
        "PoolState",
        &[
            ("pool", pool.identity.calc_script_hash().as_slice().to_vec()),
            ("asset_a", pool.assets[0].calc_script_hash().as_slice().to_vec()),
            ("asset_b", pool.assets[1].calc_script_hash().as_slice().to_vec()),
            ("lp", pool.lp.calc_script_hash().as_slice().to_vec()),
            ("minimum_lock", pool.minimum.calc_script_hash().as_slice().to_vec()),
            ("network", backend.network_domain.to_vec()),
            ("fee", state.fee_bps.to_le_bytes().to_vec()),
            ("nonce", pool.nonce.to_le_bytes().to_vec()),
            ("reserve_a", state.reserve_a.to_le_bytes().to_vec()),
            ("reserve_b", state.reserve_b.to_le_bytes().to_vec()),
            ("total_lp", state.total_lp.to_le_bytes().to_vec()),
            ("revision", revision.to_le_bytes().to_vec()),
        ],
    )
}
pub fn reserve(programs: &Programs, pool: &Pool, asset: usize, amount: u128) -> Result<Cell> {
    let bytes = programs.reserve.data(
        "ReserveData",
        &[
            ("amount", amount.to_le_bytes().to_vec()),
            ("pool", pool.identity.calc_script_hash().as_slice().to_vec()),
            ("controller", pool.controller.calc_script_hash().as_slice().to_vec()),
            ("asset", pool.assets[asset].calc_script_hash().as_slice().to_vec()),
        ],
    )?;
    Ok(cell(programs.reserve.script(&ckb_hash::blake2b_256(&bytes[16..])), Some(pool.assets[asset].clone()), bytes))
}
pub fn shares(programs: &Programs, pool: &Pool, holder: packed::Script, amount: u128) -> Result<Cell> {
    let bytes = programs.lp.data(
        "ShareData",
        &[
            ("amount", amount.to_le_bytes().to_vec()),
            ("pool", pool.identity.calc_script_hash().as_slice().to_vec()),
            ("pool_code", pool.controller.code_hash().as_slice().to_vec()),
        ],
    )?;
    Ok(cell(holder, Some(pool.lp.clone()), bytes))
}
pub fn witness(
    programs: &Programs,
    backend: &Backend,
    mode: u64,
    ib: u64,
    a: u128,
    b: u128,
    shares: u128,
    min_a: u128,
    min_b: u128,
    key: u8,
) -> Result<packed::WitnessArgs> {
    programs.pool.witness(&[
        Arg::U64(mode),
        Arg::U64(ib),
        Arg::U64(0),
        Arg::U128(a),
        Arg::U128(b),
        Arg::U128(shares),
        Arg::U128(min_a),
        Arg::U128(min_b),
        Arg::Hash(backend.owner(key).calc_script_hash().unpack()),
    ])
}
pub fn value(backend: &Backend, point: &packed::OutPoint) -> u128 {
    u128::from_le_bytes(backend.live[point].1[..16].try_into().unwrap())
}

pub fn initialise(
    programs: &Programs,
    backend: &mut Backend,
    wallet: &mut Wallet,
    assets: [packed::Script; 2],
    fee: u64,
    nonce: u64,
    a: u128,
    b: u128,
) -> Result<(Pool, ckb_types::core::TransactionView)> {
    let funding = wallet.funding()?;
    let first = packed::CellInput::new_builder().previous_output(funding.clone()).build();
    let identity = packed::Script::new_builder()
        .code_hash(cellscript::ckb_abi::type_id::CODE_HASH)
        .hash_type(1u8)
        .args(Bytes::copy_from_slice(&cellscript_ckb_adapter::type_id_args_from_first_input(&first, 0)).pack())
        .build();
    let configuration =
        [identity.calc_script_hash().as_slice(), packed::CellOutput::calc_data_hash(programs.pool.code()).as_slice()].concat();
    let quotation = backend.measure_quote("pool-initialise", || quote::initialise(a, b, fee, 1))?;
    let mut pool = Pool {
        points: vec![],
        identity,
        controller: programs.pool.script(&[]),
        assets,
        lp: programs.lp.script(&ckb_hash::blake2b_256(&configuration)),
        minimum: programs.minimum.script(&[]),
        state: quotation.next,
        nonce,
        revision: 0,
    };
    let state = pool_data(programs, backend, &pool, pool.state, 0)?;
    pool.controller = programs.pool.script(&ckb_hash::blake2b_256(&state[..208]));
    let bootfund = wallet.funding()?;
    let unsigned = backend.build(
        &[bootfund],
        vec![cell(pool.controller.clone(), None, state.clone()), cell(backend.owner(wallet.key), None, vec![])],
        1,
        vec![],
    )?;
    let signed = backend.sign(&unsigned)?;
    let bootstrap = backend.commit(&signed, "real-pool-bootstrap-without-type-id-claim")?[0].clone();
    let outputs = vec![
        cell(pool.controller.clone(), Some(pool.identity.clone()), state),
        reserve(programs, &pool, 0, a)?,
        reserve(programs, &pool, 1, b)?,
        token(
            programs,
            backend,
            &pool.assets[0],
            wallet.key,
            value(backend, &wallet.a).checked_sub(a).context("provider lacks requested asset A in selected Cell")?,
        )?,
        token(
            programs,
            backend,
            &pool.assets[1],
            wallet.key,
            value(backend, &wallet.b).checked_sub(b).context("provider lacks requested asset B in selected Cell")?,
        )?,
        shares(programs, &pool, backend.owner(wallet.key), quotation.minted)?,
        shares(programs, &pool, pool.minimum.clone(), quote::LOCKED_MINIMUM)?,
        cell(backend.owner(wallet.key), None, vec![]),
    ];
    let unsigned = backend.build(
        &[funding, bootstrap, wallet.a.clone(), wallet.b.clone()],
        outputs,
        7,
        vec![
            packed::WitnessArgs::default(),
            witness(programs, backend, 0, 1, a, b, quotation.minted, 0, 0, wallet.key)?,
            programs.asset.witness(&[])?,
            programs.asset.witness(&[])?,
            packed::WitnessArgs::default(),
            programs.lp.witness(&[])?,
        ],
    )?;
    Ok((pool, unsigned))
}

pub fn swap(
    programs: &Programs,
    backend: &Backend,
    pool: &Pool,
    wallet: &Wallet,
    funding: packed::OutPoint,
    input: u128,
    direction: quote::Direction,
) -> Result<(ckb_types::core::TransactionView, quote::PoolState, u128)> {
    let forward = direction == quote::Direction::AForB;
    let quoted = backend.measure_quote("pool-swap-exact-input", || quote::swap_exact_in(pool.state, direction, input, 1))?;
    let mut inputs = pool.points.clone();
    inputs.extend([funding, wallet.a.clone(), wallet.b.clone()]);
    let outputs = vec![
        cell(
            pool.controller.clone(),
            Some(pool.identity.clone()),
            pool_data(programs, backend, pool, quoted.next, pool.revision + 1)?,
        ),
        reserve(programs, pool, 0, quoted.next.reserve_a)?,
        reserve(programs, pool, 1, quoted.next.reserve_b)?,
        token(
            programs,
            backend,
            &pool.assets[0],
            wallet.key,
            if forward { value(backend, &wallet.a) - input } else { value(backend, &wallet.a) + quoted.amount_out },
        )?,
        token(
            programs,
            backend,
            &pool.assets[1],
            wallet.key,
            if forward { value(backend, &wallet.b) + quoted.amount_out } else { value(backend, &wallet.b) - input },
        )?,
        cell(backend.owner(wallet.key), None, vec![]),
    ];
    Ok((
        backend.build(
            &inputs,
            outputs,
            5,
            vec![
                witness(
                    programs,
                    backend,
                    if forward { 1 } else { 2 },
                    0,
                    if forward { input } else { 0 },
                    if forward { 0 } else { input },
                    0,
                    if forward { 0 } else { quoted.amount_out },
                    if forward { quoted.amount_out } else { 0 },
                    wallet.key,
                )?,
                programs.asset.witness(&[])?,
                programs.asset.witness(&[])?,
            ],
        )?,
        quoted.next,
        quoted.amount_out,
    ))
}

pub fn withdraw(
    programs: &Programs,
    backend: &Backend,
    pool: &Pool,
    wallet: &Wallet,
    funding: packed::OutPoint,
    burn: u128,
) -> Result<(ckb_types::core::TransactionView, quote::WithdrawQuote)> {
    let quoted = backend.measure_quote("pool-withdraw", || quote::withdraw(pool.state, burn, 1, 1))?;
    let lp = wallet.lp.as_ref().context("holder has no live LP Cell")?;
    let remaining = value(backend, lp).checked_sub(burn).context("LP burn exceeds actual holder balance")?;
    let mut inputs = pool.points.clone();
    inputs.extend([funding, lp.clone()]);
    let mut outputs = vec![
        cell(
            pool.controller.clone(),
            Some(pool.identity.clone()),
            pool_data(programs, backend, pool, quoted.next, pool.revision + 1)?,
        ),
        reserve(programs, pool, 0, quoted.next.reserve_a)?,
        reserve(programs, pool, 1, quoted.next.reserve_b)?,
        token(programs, backend, &pool.assets[0], wallet.key, quoted.paid_a)?,
        token(programs, backend, &pool.assets[1], wallet.key, quoted.paid_b)?,
    ];
    if remaining > 0 {
        outputs.push(shares(programs, pool, backend.owner(wallet.key), remaining)?);
    }
    let change = outputs.len();
    outputs.push(cell(backend.owner(wallet.key), None, vec![]));
    let tx = backend.build(
        &inputs,
        outputs,
        change,
        vec![
            witness(programs, backend, 4, 0, 0, 0, burn, quoted.paid_a, quoted.paid_b, wallet.key)?,
            programs.asset.witness(&[])?,
            programs.asset.witness(&[])?,
            packed::WitnessArgs::default(),
            programs.lp.witness(&[])?,
        ],
    )?;
    Ok((tx, quoted))
}

pub fn deposit(
    programs: &Programs,
    backend: &Backend,
    pool: &Pool,
    wallet: &Wallet,
    funding: packed::OutPoint,
    max_a: u128,
    max_b: u128,
) -> Result<(ckb_types::core::TransactionView, quote::DepositQuote)> {
    let quoted = backend.measure_quote("pool-proportional-deposit", || quote::deposit(pool.state, max_a, max_b, 1))?;
    let inputs = [pool.points.clone(), vec![funding, wallet.a.clone(), wallet.b.clone()]].concat();
    let outputs = vec![
        cell(
            pool.controller.clone(),
            Some(pool.identity.clone()),
            pool_data(programs, backend, pool, quoted.next, pool.revision + 1)?,
        ),
        reserve(programs, pool, 0, quoted.next.reserve_a)?,
        reserve(programs, pool, 1, quoted.next.reserve_b)?,
        token(
            programs,
            backend,
            &pool.assets[0],
            wallet.key,
            value(backend, &wallet.a).checked_sub(quoted.used_a).context("deposit A balance")?,
        )?,
        token(
            programs,
            backend,
            &pool.assets[1],
            wallet.key,
            value(backend, &wallet.b).checked_sub(quoted.used_b).context("deposit B balance")?,
        )?,
        shares(programs, pool, backend.owner(wallet.key), quoted.minted)?,
        cell(backend.owner(wallet.key), None, vec![]),
    ];
    let tx = backend.build(
        &inputs,
        outputs,
        6,
        vec![
            witness(programs, backend, 3, 0, max_a, max_b, quoted.minted, 0, 0, wallet.key)?,
            programs.asset.witness(&[])?,
            programs.asset.witness(&[])?,
            packed::WitnessArgs::default(),
            packed::WitnessArgs::default(),
            programs.lp.witness(&[])?,
        ],
    )?;
    Ok((tx, quoted))
}

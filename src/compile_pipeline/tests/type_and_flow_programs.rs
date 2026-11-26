const TYPE_HASH_PROGRAM: &str = r#"
module test;

struct Pool {
    amount: u64,
}

action pool_id(witness pool: Pool) -> Hash {

        return pool.type_hash();
}
"#;

const ZERO_PROGRAM: &str = r#"
module test;

action is_zero(witness target: Address) -> bool {

        return target == Address::zero();
}
"#;

const LOCAL_ZERO_PROGRAM: &str = r#"
module test;

action is_zero(witness target: Address) -> bool {

        let zero = Address::zero();
        return target == zero;
}
"#;

const U128_EQ_PROGRAM: &str = r#"
module test;

action eq128(witness left: u128, witness right: u128) -> bool {

        return left == right;
}
"#;

const U128_MOD_PROGRAM: &str = r#"
module test;

fn mod128(left: u128, right: u128) -> u128 {
    return left % right;
}

action remainder(witness left: u128, witness right: u128) -> u128 {

        return mod128(left, right);
}
"#;

const U128_LITERAL_PROGRAM: &str = r#"
module test;

const MAX: u128 = 340282366920938463463374607431768211455;

fn max_value() -> u128 {
    let inferred = 340282366920938463463374607431768211455;
    return inferred;
}

action is_max(witness value: u128) -> bool {

        return (value == MAX) && (value == max_value());
}
"#;

const U128_MUTATE_PROGRAM: &str = r#"
module test;

shared Ledger has store {
    balance: u128,
    owner: Address,
}

action credit(input ledger_before: Ledger, witness delta: u64,  output ledger_after: Ledger) {

        require ledger_after.owner == ledger_before.owner;
        require ledger_after.balance == ledger_before.balance + delta;
}
"#;

const LARGE_PRESERVED_MUTATE_PROGRAM: &str = r#"
module test;

shared BigState has store {
    balance: u64,
    pad: [u8; 600],
}

action update(input state_before: BigState, witness delta: u64,  output state_after: BigState) {

        require state_after.balance == state_before.balance + delta;
        require state_after.pad == state_before.pad;
}
"#;

const SUMMARY_PROGRAM: &str = r#"
module test;

shared Config {
    threshold: u64,
}

resource Token has store, replace, relock, consume, burn {
    amount: u64,
}

action update(witness amount: u64) -> u64 {

        let cfg = read_ref::<Config>();
        let token = create Token { amount: amount, };
        consume token;
        return cfg.threshold;
}
"#;

const BAD_LOCK_PROGRAM: &str = r#"
module test;

lock invalid(witness owner: Address) -> u64 {

        return 1;
}
"#;

const LOCK_CREATE_PROGRAM: &str = r#"
module test;

resource Token {
    amount: u64,
}

lock bad() -> bool {

        let token = create Token { amount: 1, };
        return true;
}
"#;

const LOCK_DESTROY_PROGRAM: &str = r#"
module test;

lock bad() -> bool {

        destroy token;
        return true;
}
"#;

const LOCK_READ_REF_PROGRAM: &str = r#"
module test;

shared Config {
    threshold: u64,
}

lock guard() -> bool {

        let cfg = read_ref::<Config>();
        return cfg.threshold > 0;
}
"#;

const FIXED_BYTE_PARAM_AND_CONST_OUTPUT_PROGRAM: &str = r#"
module test;

resource Config has store {
    symbol: [u8; 8],
}

resource Fingerprint has store {
    digest: Hash,
}

action make_config(witness symbol: [u8; 8]) -> Config {

        create Config {
            symbol: symbol,
        }

}
action make_fingerprint() -> Fingerprint {

        create Fingerprint {
            digest: Hash::zero(),
        }
}
"#;

const CONST_LOCK_OUTPUT_PROGRAM: &str = r#"
module test;

resource Token has store {
    amount: u64,
}

action mint() -> Token {

        create Token {
            amount: 42,
        } with_lock(Address::zero())
}
"#;

const MISSING_DESTROY_CAPABILITY_PROGRAM: &str = r#"
module test;

resource Token has store {
    amount: u64,
}

action burn(input token: Token) {

        destroy token;
}
"#;

const STATE_MACHINE_NOOP_TRANSITION_PROGRAM: &str = r#"
module test;

receipt Ticket has store {
    state: u8,
    id: u64,
}

flow Ticket.state {
    Created -> Created;
}

action noop() -> u64 {

        return 0;
}
"#;

const FLOW_MISSING_STATE_CREATE_PROGRAM: &str = r#"
module test;

receipt Ticket has store {
    state: u8,
    id: u64,
}

flow Ticket.state {
    Created -> Active;
}

action make() -> Ticket {

        return create Ticket {
            id: 1,
        };
}
"#;

const FLOW_MISSING_STATE_FIELD_PROGRAM: &str = r#"
module test;

receipt Ticket has store {
    id: u64,
}

flow Ticket.state {
    Created -> Active;
}

action noop() -> u64 {

        return 0;
}
"#;

const FLOW_BAD_STATE_TYPE_PROGRAM: &str = r#"
module test;

receipt Ticket has store {
    state: bool,
    id: u64,
}

flow Ticket.state {
    Created -> Active;
}

action noop() -> u64 {

        return 0;
}
"#;

const FLOW_OUT_OF_RANGE_STATE_CREATE_PROGRAM: &str = r#"
module test;

receipt Ticket has store {
    state: u8,
    id: u64,
}

flow Ticket.state {
    Created -> Active;
}

action make() -> Ticket {

        return create Ticket {
            state: 2,
            id: 1,
        };
}
"#;

const FLOW_NON_INITIAL_CREATE_PROGRAM: &str = r#"
module test;

receipt Ticket has store {
    state: u8,
    id: u64,
}

flow Ticket.state {
    Created -> Active;
}

action make() -> Ticket {

        return create Ticket {
            state: 1,
            id: 1,
        };
}
"#;

const FLOW_DYNAMIC_INITIAL_CREATE_PROGRAM: &str = r#"
module test;

receipt Ticket has store {
    state: u8,
    id: u64,
}

flow Ticket.state {
    Created -> Active;
}

action make(witness state: u8) -> Ticket {

        return create Ticket {
            state: state,
            id: 1,
        };
}
"#;

const FLOW_RESET_UPDATE_PROGRAM: &str = r#"
module test;

receipt Ticket has store {
    state: u8,
    id: u64,
}

flow Ticket.state {
    Created -> Active;
}

action reset(input ticket: Ticket) -> Ticket {

        consume ticket;
        return create Ticket {
            state: 0,
            id: ticket.id,
        };
}
"#;

const FLOW_STATIC_UPDATE_PROGRAM: &str = r#"
module test;

receipt Ticket has store {
    state: u8,
    id: u64,
}

flow Ticket.state {
    Created -> Active;
}

action activate(input ticket: Ticket,  output output: Ticket) {
    transition ticket.state: Created -> output.state: Active;

        create output = Ticket {
            state: Active,
            id: ticket.id,
        }
}
"#;

const FLOW_SELF_LOOP_UPDATE_PROGRAM: &str = r#"
module test;

receipt Ticket has store {
    state: u8,
    claimed: u64,
}

flow Ticket.state {
    Active -> Active;
    Active -> Closed;
}

action claim(input ticket: Ticket,  output output: Ticket) {
    transition ticket.state: Active -> output.state: Active;

        create output = Ticket {
            state: Active,
            claimed: ticket.claimed + 1,
        }
}
"#;

const FLOW_INITIAL_STATE_NAME_CREATE_PROGRAM: &str = r#"
module test;

receipt Ticket has store {
    state: u8,
    id: u64,
}

flow Ticket.state {
    Created -> Active;
}

action make() -> Ticket {

        return create Ticket {
            state: Created,
            id: 1,
        };
}
"#;

const FLOW_QUALIFIED_STATE_NAME_PROGRAM: &str = r#"
module test;

receipt Ticket has store {
    state: u8,
    id: u64,
}

flow Ticket.state {
    Created -> Active;
}

action activate(input ticket: Ticket,  output output: Ticket) {
    transition ticket.state: Created -> output.state: Active;

        require ticket.state < Ticket::Active, "already active";
        create output = Ticket {
            state: Ticket::Active,
            id: ticket.id,
        }
}
"#;

const FLOW_WRONG_QUALIFIED_STATE_FIELD_PROGRAM: &str = r#"
module test;

receipt Ticket has store {
    state: u8,
    id: u64,
}

receipt OtherTicket has store {
    state: u8,
    id: u64,
}

flow Ticket.state {
    Created -> Active;
}

flow OtherTicket.state {
    Draft -> Live;
}

action activate(input ticket: Ticket) -> Ticket {

        consume ticket;
        return create Ticket {
            state: OtherTicket::Live,
            id: ticket.id,
        };
}
"#;

fn action_metadata_with_scheduler_fields(scheduler_witness_hex: &str) -> ActionMetadata {
    ActionMetadata {
        name: "scheduler".to_string(),
        params: vec![],
        return_type: Some("unit".to_string()),
        effects: ir::EffectFacts::empty(),
        parallelizable: false,
        touches_shared: vec![],
        estimated_cycles: 0,
        scheduler_witness_abi: SCHEDULER_WITNESS_ABI_MOLECULE.to_string(),
        scheduler_witness_hex: scheduler_witness_hex.to_string(),
        consume_set: vec![],
        read_refs: vec![],
        create_set: vec![],
        mutate_set: vec![],
        state_transition_edges: vec![],
        protocol_role_candidates: vec![],
        pool_primitives: vec![],
        ckb_script_group: None,
        ckb_runtime_accesses: vec![],
        ckb_runtime_features: vec![],
        fail_closed_runtime_features: vec![],
        verifier_obligations: vec![],
        proof_plan: vec![],
        transaction_runtime_input_requirements: vec![],
        elf_compatible: true,
        standalone_runner_compatible: true,
        block_count: 0,
    }
}

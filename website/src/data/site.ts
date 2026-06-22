import { readFileSync } from "node:fs";
import { fileURLToPath } from "node:url";
import { dirname, resolve } from "node:path";

// site.ts lives at website/src/data/site.ts. We need to reach the
// repo root (../../..) to read the examples/ directory.
const here = dirname(fileURLToPath(import.meta.url));
const repoRoot = resolve(here, "..", "..", "..");

export const links = {
  docs: "https://github.com/a19q3/CellScript/tree/main/docs",
  examples: "https://github.com/a19q3/CellScript/tree/main/examples",
  source: "https://github.com/a19q3/CellScript",
  wiki: "https://github.com/a19q3/CellScript/wiki",
};

/**
 * Full compilable source for each hero example, read from examples/ at
 * build time. The heroExamples array above uses truncated display
 * snippets ("// ... omitted") for the landing page; the playground
 * needs the real file so the WASM compiler can actually compile it.
 */
export const exampleFullSources: Record<string, string> = {
  token: readFileSync(resolve(repoRoot, "examples", "token.cell"), "utf-8"),
  nft: readFileSync(resolve(repoRoot, "examples", "nft.cell"), "utf-8"),
  amm: readFileSync(resolve(repoRoot, "examples", "amm_pool.cell"), "utf-8"),
  vesting: readFileSync(resolve(repoRoot, "examples", "vesting.cell"), "utf-8"),
};

export const heroExamples = [
  {
    id: "token",
    file: "token.cell",
    command: "cellc examples/token.cell --target-profile ckb",
    lines: [
      "module cellscript::fungible_token",
      "// ... invariant and MintAuthority omitted",
      "resource Token has store, create, consume, replace, burn, relock {",
      "    amount: u64,",
      "    symbol: [u8; 8],",
      "}",
      "",
      "action transfer_token(token: Token, to: Address) -> next_token: Token",
      "where",
      "    consume token",
      "    create next_token = Token { amount: token.amount, symbol: token.symbol } with_lock(to)",
      "",
      "action burn(token: Token)",
      "where",
      "    assert(token.amount > 0, \"cannot burn zero\")",
      "    destroy token",
    ],
  },
  {
    id: "nft",
    file: "nft.cell",
    command: "cellc examples/nft.cell --target-profile ckb",
    lines: [
      "module cellscript::nft",
      "// ... constants and Metadata struct omitted",
      "resource NFT has store, create, consume, replace, burn, relock, read_ref {",
      "    token_id: u64,",
      "    owner: Address,",
      "    metadata_hash: Hash,",
      "    royalty_recipient: Address,",
      "    royalty_bps: u16,",
      "}",
      "",
      "// ... listing and offer receipts omitted",
      "action transfer(nft_before: NFT, to: Address) -> nft_after: NFT",
      "where",
      "    assert(nft_before.owner != to, \"Cannot transfer to self\")",
      "    preserve nft_after from nft_before {",
      "        token_id",
      "        metadata_hash",
      "        royalty_recipient",
      "        royalty_bps",
      "    }",
      "    require nft_after.owner == to",
    ],
  },
  {
    id: "amm",
    file: "amm_pool.cell",
    command: "cellc examples/amm_pool.cell --target-profile ckb --json",
    lines: [
      "module cellscript::amm_pool",
      "use cellscript::fungible_token::Token",
      "// ... LPReceipt omitted",
      "shared Pool has store, create, replace {",
      "    token_a_symbol: [u8; 8],",
      "    token_b_symbol: [u8; 8],",
      "    reserve_a: u64,",
      "    reserve_b: u64,",
      "    total_lp: u64,",
      "    fee_rate_bps: u16,",
      "}",
      "",
      "action swap_a_for_b(pool_before: Pool, input: Token, min_output: u64, to: Address) -> (pool_after: Pool, token_out: Token)",
      "where",
      "    assert(input.symbol == pool_before.token_a_symbol, \"wrong input token\")",
      "    let fee = input.amount * pool_before.fee_rate_bps as u64 / 10000",
      "    let net_input = input.amount - fee",
      "    let amount_out = pool_before.reserve_b * net_input / (pool_before.reserve_a + net_input)",
      "    // ... preserve, require, consume, create omitted",
      "    assert(amount_out >= min_output, \"slippage exceeded\")",
    ],
  },
  {
    id: "vesting",
    file: "vesting.cell",
    command: "cellc constraints examples/vesting.cell --target-profile ckb",
    lines: [
      "module cellscript::vesting",
      "use cellscript::fungible_token::Token",
      "// ... VestingConfig and VestingGrant fields omitted",
      "flow VestingGrant.state {",
      "    Granted -> Claimable;",
      "    Granted -> FullyClaimed;",
      "    Claimable -> FullyClaimed;",
      "}",
      "",
      "action claim_vested(grant: VestingGrant) -> (tokens: Token, updated_grant: VestingGrant)",
      "    transition grant.state: Claimable -> updated_grant.state: FullyClaimed",
      "where",
      "    let now = env::current_timepoint()",
      "    assert(now >= grant.cliff_timepoint, \"cliff not reached\")",
      "    // ... vesting arithmetic omitted",
      "    consume grant",
      "    create tokens = Token { amount: claimable, symbol: grant.token_symbol } with_lock(grant.beneficiary)",
    ],
  },
] as const;

export const modelCards = [
  {
    id: "resource",
    name: "resource",
    source: "examples/token.cell",
    code: "// examples/token.cell\nresource Token has store, create, consume, replace, burn, relock {\n    amount: u64,\n    symbol: [u8; 8],\n}",
  },
  {
    id: "shared",
    name: "shared",
    source: "examples/amm_pool.cell",
    code: "// examples/amm_pool.cell\nshared Pool has store, create, replace {\n    token_a_symbol: [u8; 8],\n    token_b_symbol: [u8; 8],\n    // ... reserves, LP supply, fee rate omitted\n}",
  },
  {
    id: "receipt",
    name: "receipt",
    source: "examples/nft.cell",
    code: "// examples/nft.cell\nreceipt Listing has create, consume, burn {\n    token_id: u64,\n    seller: Address,\n    price: u64,\n    // ... created_at and state omitted\n}",
  },
  {
    id: "action",
    name: "action",
    source: "examples/vesting.cell",
    code: "// examples/vesting.cell\naction claim_vested(grant: VestingGrant) -> (tokens: Token, updated_grant: VestingGrant)\n    transition grant.state: Claimable -> updated_grant.state: FullyClaimed\nwhere\n    // ... timepoint checks and vesting arithmetic omitted\n    consume grant",
  },
  {
    id: "lock",
    name: "lock",
    source: "examples/language/canonical_style.cell",
    code: "// examples/language/canonical_style.cell\nlock vault_owner(protected vault: Vault, lock_args owner: Address, witness claimed_owner: Address) -> bool {\n    let input = source::group_input(0)\n    let witness_lock = witness::lock(input)\n    // ... digest and ownership checks omitted\n}",
  },
  {
    id: "flow",
    name: "flow",
    source: "examples/vesting.cell",
    code: "// examples/vesting.cell\nflow VestingGrant.state {\n    Granted -> Claimable;\n    Granted -> FullyClaimed;\n    Claimable -> FullyClaimed;\n}",
  },
  {
    id: "invariant",
    name: "invariant",
    source: "examples/language/v0_15_scoped_invariant.cell",
    code: "// examples/language/v0_15_scoped_invariant.cell\ninvariant token_amount_conservation {\n    trigger: type_group\n    scope: group\n    reads: group_inputs<Token>.amount, group_outputs<Token>.amount\n    assert_sum(group_outputs<Token>.amount) == assert_sum(group_inputs<Token>.amount)\n}",
  },
  {
    id: "structEnum",
    name: "struct / enum",
    source: "examples/nft.cell",
    code: "// examples/nft.cell\nstruct Metadata {\n    name: String,\n    description: String,\n    image_uri: String,\n    attributes: Vec<(String, String)>,\n}",
  },
  {
    id: "identity",
    name: "identity",
    source: "examples/language/v0_15_identity_lifecycle.cell",
    code: "// examples/language/v0_15_identity_lifecycle.cell\naction mint_unique_nft(recipient: Address, tid: u64) -> UniqueNFT\nwhere\n    create_unique<UniqueNFT>(identity = field(token_id)) { token_id: tid, owner: recipient } with_lock(recipient)",
  },
] as const;

export const tools = [
  {
    id: "metadata",
    name: "metadata",
    command: "cellc metadata examples/vesting.cell --target-profile ckb --json",
  },
  {
    id: "constraints",
    name: "constraints",
    command: "cellc constraints examples/vesting.cell --target-profile ckb",
  },
  {
    id: "auditBundle",
    name: "audit-bundle",
    command: "cellc audit-bundle examples/token.cell --target-profile ckb --out audit/",
  },
  {
    id: "lsp",
    name: "lsp",
    command: "cellc lsp",
  },
] as const;

export const exampleGroups = [
  {
    id: "protocols",
    items: [
      { path: "examples/token.cell", id: "token", tags: ["resource", "invariant", "burn"] },
      { path: "examples/nft.cell", id: "nft", tags: ["resource", "receipt", "preserve"] },
      { path: "examples/amm_pool.cell", id: "ammPool", tags: ["shared", "receipt", "slippage"] },
      { path: "examples/vesting.cell", id: "vesting", tags: ["flow", "receipt", "env"] },
      { path: "examples/launch.cell", id: "launch", tags: ["flow", "settle", "claim"] },
      { path: "examples/registry.cell", id: "registry", tags: ["resource", "identity", "replace"] },
    ],
  },
  {
    id: "primitives",
    items: [
      { path: "examples/multisig.cell", id: "multisig", tags: ["lock", "witness", "threshold"] },
      { path: "examples/timelock.cell", id: "timelock", tags: ["lock", "env", "timepoint"] },
      { path: "examples/language/canonical_style.cell", id: "canonicalStyle", tags: ["protected", "witness", "sighash"] },
      { path: "examples/language/v0_14_capacity_time.cell", id: "capacityTime", tags: ["capacity", "env", "time"] },
      { path: "examples/language/v0_14_witness_source.cell", id: "witnessSource", tags: ["source", "witness", "boundary"] },
      { path: "examples/language/v0_14_delegate_verify.cell", id: "delegateVerify", tags: ["delegate", "verify", "lock"] },
    ],
  },
  {
    id: "language",
    items: [
      { path: "examples/language/v0_15_identity_lifecycle.cell", id: "identityLifecycle", tags: ["identity", "unique", "lifecycle"] },
      { path: "examples/language/v0_15_scoped_invariant.cell", id: "scopedInvariant", tags: ["invariant", "assert_sum", "scope"] },
      { path: "examples/language/order_book.cell", id: "orderBook", tags: ["orders", "matching", "state"] },
      { path: "examples/language/registry.cell", id: "languageRegistry", tags: ["registry", "fields", "replace"] },
      { path: "examples/language/v0_14_hash_blake2b.cell", id: "blake2bHash", tags: ["hash", "blake2b", "stdlib"] },
      { path: "examples/ickb_benchmark/ickb_logic.cell", id: "ickbLogic", tags: ["benchmark", "receipt", "logic"] },
    ],
  },
] as const;

export const workflowSteps = [
  { id: "source", icon: "source" },
  { id: "parse", icon: "parse" },
  { id: "metadata", icon: "metadata" },
  { id: "riscv", icon: "riscv" },
  { id: "elf", icon: "elf" },
] as const;

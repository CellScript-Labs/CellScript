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
 * Minimal type definitions used as inline replacements for cross-file
 * imports. The browser playground compiles one source string, so package
 * imports from examples/ need to become local declarations there.
 */
const TOKEN_TYPE_INLINE = `resource Token has store, create, consume, replace, burn, relock {
    amount: u64,
    symbol: [u8; 8],
}`;

const FUNGIBLE_TOKEN_TYPES_INLINE = `${TOKEN_TYPE_INLINE}

resource MintAuthority has store, create, replace {
    token_symbol: [u8; 8],
    max_supply: u64,
    minted: u64,
}`;

const AMM_POOL_TYPES_INLINE = `receipt LPReceipt has store, create, consume {
    pool_id: Hash,
    lp_amount: u64,
    provider: Address,
}

shared Pool has store, create, replace {
    token_a_symbol: [u8; 8],
    token_b_symbol: [u8; 8],
    reserve_a: u64,
    reserve_b: u64,
    total_lp: u64,
    fee_rate_bps: u16,
}`;

/**
 * Make an example source self-contained for single-file playground
 * compilation. The source files themselves remain unchanged; this is
 * only the browser presentation copy.
 */
const makeStandalone = (source: string): string =>
  source
    .replace(
      /^use cellscript::fungible_token::Token\s*$/m,
      TOKEN_TYPE_INLINE,
    )
    .replace(
      /^use cellscript::fungible_token::\{\s*Token,\s*MintAuthority\s*\}\s*$/m,
      FUNGIBLE_TOKEN_TYPES_INLINE,
    )
    .replace(
      /^use cellscript::amm_pool::\{\s*Pool,\s*LPReceipt\s*\}\s*$/m,
      AMM_POOL_TYPES_INLINE,
    );

export const playgroundExamples = [
  { id: "token", label: "Fungible token", file: "examples/token.cell" },
  { id: "nft", label: "NFT collection", file: "examples/nft.cell" },
  { id: "amm", label: "AMM pool", file: "examples/amm_pool.cell" },
  { id: "vesting", label: "Vesting flow", file: "examples/vesting.cell" },
  { id: "launch", label: "Token launch", file: "examples/launch.cell" },
  { id: "multisig", label: "Multisig wallet", file: "examples/multisig.cell" },
  { id: "timelock", label: "Timelock escrow", file: "examples/timelock.cell" },
  { id: "canonicalStyle", label: "Canonical style", file: "examples/language/canonical_style.cell" },
  { id: "orderBook", label: "Order book", file: "examples/language/order_book.cell" },
  { id: "languageRegistry", label: "Language registry", file: "examples/language/registry.cell" },
  { id: "stdlib", label: "Stdlib constraints", file: "examples/language/stdlib.cell" },
  { id: "ckbTypeIdCreate", label: "CKB TYPE_ID create", file: "examples/language/v0_14_ckb_type_id_create.cell" },
  { id: "delegateVerify", label: "Delegate verify", file: "examples/language/v0_14_delegate_verify.cell" },
  { id: "blake2bHash", label: "Blake2b hash lock", file: "examples/language/v0_14_hash_blake2b.cell" },
  { id: "multiStepPipeline", label: "Spawn pipeline", file: "examples/language/v0_14_multi_step_pipeline.cell" },
  { id: "witnessSource", label: "Witness source", file: "examples/language/v0_14_witness_source.cell" },
  { id: "identityLifecycle", label: "Identity lifecycle", file: "examples/language/v0_15_identity_lifecycle.cell" },
  { id: "scopedInvariant", label: "Scoped invariants", file: "examples/language/v0_15_scoped_invariant.cell" },
] as const;

const playgroundSourceNotes: Record<string, readonly string[]> = {
  token: [
    "Start here: this example shows the Cell lifecycle for a simple token.",
    "Follow the consume/create/destroy lines; they are the core of CellScript.",
  ],
  nft: [
    "Shows a larger asset model: collection state, NFT ownership, sale receipts, and royalties.",
    "Use it after the token example; there are more moving parts.",
  ],
  amm: [
    "Shows shared pool state and receipts for liquidity providers.",
    "The important checks are reserve updates, fee bounds, and slippage protection.",
  ],
  vesting: [
    "Shows time-based state: grants move through a small flow before tokens can be claimed.",
    "Look for env::current_timepoint and the transition line.",
  ],
  launch: [
    "Shows a launch transaction that creates several outputs at once.",
    "The checks keep allocation totals, pool seed amount, and change output consistent.",
  ],
  multisig: [
    "Shows receipt-style workflow around proposals, signatures, and execution records.",
    "This is a larger example; focus first on create_wallet and propose_transfer.",
  ],
  timelock: [
    "Shows absolute and relative lock timing.",
    "The useful idea is that unlock conditions are explicit checks, not hidden wallet logic.",
  ],
  canonicalStyle: [
    "Shows the recommended shape for source reads, witness data, and lock args.",
    "The comments in the lock explain what is real authority and what is only decoded data.",
  ],
  orderBook: [
    "Small local-data exercise for Vec operations and order matching.",
    "It is not a full exchange; it teaches list operations inside verifier code.",
  ],
  languageRegistry: [
    "Small local registry exercise for Vec insert, remove, set, truncate, and reverse.",
    "Useful for learning collection operations without a full protocol around them.",
  ],
  stdlib: [
    "Shows compiler-recognised std:: helpers that expand to ordinary verifier checks.",
    "Use this when you want to see how preserve, transfer, claim, and settle are written.",
  ],
  ckbTypeIdCreate: [
    "Shows TYPE_ID-style identity at the boundary between source, metadata, and builder evidence.",
    "The source declares intent; deployment still needs builder-side evidence.",
  ],
  delegateVerify: [
    "Advanced: shows bounded verifier reuse through spawn/wait.",
    "Read it as a CKB runtime boundary example, not as a beginner contract.",
  ],
  blake2bHash: [
    "Small lock example for hashing witness data and comparing the result.",
    "Good for seeing how a lock returns true only after explicit checks pass.",
  ],
  multiStepPipeline: [
    "Advanced: shows pipe descriptors and delegated verifier communication.",
    "The teaching point is resource cleanup: every descriptor is closed before exit.",
  ],
  witnessSource: [
    "Shows how protected Cells, lock args, witness data, and sighash relate.",
    "Names like claimed_owner are not authority by themselves; the checks make them meaningful.",
  ],
  identityLifecycle: [
    "Shows unique Cell identity policies such as TYPE_ID, field identity, and script args.",
    "Use it to learn create_unique, replace_unique, and policy-specific destruction.",
  ],
  scopedInvariant: [
    "Shows aggregate invariants such as sum conservation and uniqueness.",
    "These are review obligations: inspect the metadata before relying on them.",
  ],
};

const annotateForPlayground = (id: string, source: string): string => {
  const notes = playgroundSourceNotes[id];
  if (!notes?.length) return source;
  return [
    "// Playground teaching note:",
    ...notes.map((note) => `// ${note}`),
    "",
    source,
  ].join("\n");
};

const readExampleSource = (id: string, file: string): string =>
  annotateForPlayground(id, makeStandalone(readFileSync(resolve(repoRoot, file), "utf-8")));

/**
 * Full compilable source for each playground example, read from examples/
 * at build time. The heroExamples array below still uses short display
 * snippets for the landing page.
 */
export const exampleFullSources = Object.fromEntries(
  playgroundExamples.map((example) => [example.id, readExampleSource(example.id, example.file)]),
) as Record<string, string>;

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

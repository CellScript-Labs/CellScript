# RFC: CellScript Website Paradigm Upgrade

## Status

Proposed. Implementation gated on review of WASM bundle size budget
(see §10) and playground editor approach (see §4.2 and Appendix A).

Updated: 2026-06-22.

## Goal

The current CellScript landing page (`website/src/pages/index.astro`)
executes the 2023-2025 dev-tool landing-page convention well: large
hero wordmark, aurora gradient, three stat cards, tabbed static code
panels, stacked sections. It is clean and disciplined, but the
convention itself is now generic. This RFC proposes a paradigm shift
that makes the site unmistakably a CellScript site: a live compiler in
the browser, the language's core primitive (`typed transition`) as the
navigation metaphor, on-chain substrate as the visual subject rather
than hidden detail, scroll-driven compiler-pipeline narrative, and a
permanent provenance rail that lets the site practise what the
language preaches.

The governing constraints are:

1. **No new effects.** No keyframe animations beyond the existing
   `aurora-drift` / `shiny-text-sweep` / `ide-border-scan`. Motion is
   limited to opacity transitions, class toggles, and
   `transform: scaleY()` progress bars driven by scroll position.
2. **Cool, ergonomic, legible, appropriately dense, layered.**
   Information density increases by layer; visual weight decreases by
   layer.
3. **WASM bundle size budgeted.** Target `.wasm` gzipped <= 600KB.
4. **Hero pseudo-console is an entry point, not a real IDE.** The
   real IDE lives on a separate `/playground` page. Clicking the hero
   console opens the playground with the current example preloaded.

This RFC records the full plan for review. It touches no code.

## Background: WASM feasibility

The compiler already exposes a browser-suitable entry point.
`cellscript::compile_metadata(source: &str, target: Option<String>)`
runs the pure in-memory pipeline
`lex -> parse -> types::check -> flow::check -> ir::generate -> metadata`
and returns a `CompileMetadata` struct that is already
`#[derive(Serialize, Deserialize)]` with semantic fields (`module`,
`target_profile`, `types`, `actions`, `lowering`, `runtime`,
`constraints`, `proof_plan`). `ActionMetadata` in particular carries
`effect_class`, `consume_set`, `create_set`, `mutate_set`,
`estimated_cycles`, `ckb_runtime_features`, `elf_compatible`, and
`proof_plan` - exactly the typed-transition evidence the playground
must render.

The only WASM blocker is dependency shape, not architecture:

- `tokio` (`features = ["full"]`, which pulls `mio`/`std::net`) and
  `tower-lsp` are pulled into the library build unconditionally via
  `pub mod lsp`.
- `clap`, `colored`, `env_logger` are pulled in by the CLI and REPL
  modules.
- `ckb-vm` is already `optional` (feature `vm-runner`).
- The core path (`lexer`, `parser`, `types`, `flow`, `ir`,
  `codegen`, `proof_plan`, `ast`, `error`) and the hash crate
  (`blake2b_simd`, pure Rust) are WASM-safe.

The ELF backend is also viable: `assemble_elf_internal` is a
self-contained pure-Rust RISC-V-to-ELF encoder, and
`try_external_elf_toolchain` returns `Ok(None)` in the browser
because `env::var` is empty, so the compiler falls back to the
internal assembler with no filesystem or shell access. Path B (full
ELF) is therefore possible but deferred to v2 to respect the bundle
size budget; v1 ships the metadata-only path.

See §10 for the size control plan and §11 for the two-path split.

## 1. Overall architecture

```
website/
+-- src/pages/
|   +-- index.astro            (refit: pseudo-console -> entry + rail + transition nav)
|   +-- playground.astro       (new: real IDE, loads wasm, live compile)
+-- src/components/
|   +-- PseudoConsole.astro    (refit: hero entry card; click -> /playground)
|   +-- ProvenanceRail.astro   (new: persistent right rail, context metadata)
|   +-- TransitionOverlay.astro(new: section-change action/consume/create flash)
+-- src/lib/
|   +-- wasm/                  (wasm loader + debounced compile + error formatting)
|   +-- provenance.ts          (view-model extraction from CompileMetadata)
|   +-- transition.ts          (section transition caption generation)
+-- public/wasm/               (prebuilt cellscript_wasm_bg.wasm)

crates/cellscript-wasm/        (new Rust crate)
+-- src/lib.rs                 (wasm_bindgen shim)
```

The site remains a static Astro build. The playground is a second
`.astro` page; the WASM module is a prebuilt static asset.

## 2. Rust side: WASM module (path A first, path B reserved)

### 2.1 Cargo.toml change (feature gating, no logic change)

Four native crates become optional:

```toml
tokio = { version = "1", features = ["full"], optional = true }
tower-lsp = { version = "0.20", optional = true }
clap = { version = "=4.5.49", features = ["derive"], optional = true }
colored = { version = "2.1", optional = true }
env_logger = { version = "0.11", optional = true }

[features]
default = ["cli", "lsp"]
cli = ["dep:clap", "dep:colored", "dep:env_logger"]
lsp = ["dep:tokio", "dep:tower-lsp"]
wasm = []
```

### 2.2 lib.rs module gating (cfg attributes only)

```rust
#[cfg(not(feature = "wasm"))]
pub mod lsp;
#[cfg(not(feature = "wasm"))]
pub mod repl;
#[cfg(not(feature = "wasm"))]
pub mod incremental;
#[cfg(not(feature = "wasm"))]
pub mod cli;
```

`lexer`, `parser`, `types`, `flow`, `ir`, `codegen`, `proof_plan`,
`ast`, `error` get **no cfg** (they are clean). `compile()` and
`compile_metadata()` keep their current signatures.

### 2.3 New crate `crates/cellscript-wasm`

A thin shim exposing three functions:

```rust
#[wasm_bindgen]
pub fn compile_metadata_json(source: &str, target: Option<String>) -> String;

#[wasm_bindgen]
pub fn compile_full_json(source: &str) -> String;  // path B, v2

#[wasm_bindgen]
pub fn version() -> String;
```

`compile_metadata_json` serialises `CompileMetadata` to JSON; errors
serialise to `{"error": "...", "span": {...}}` so the editor can map
them to line numbers.

### 2.4 Build

```bash
wasm-pack build crates/cellscript-wasm \
  --target web --no-default-features --features wasm \
  --release
```

The output is copied to `website/public/wasm/` and served as a static
asset.

## 3. Hero pseudo-console refit (entry, not IDE)

The current `.code-shell` (`index.astro:172`) shows four tabs of
static, non-editable code. It stays visually unchanged but becomes an
entry point:

1. Add a primary CTA button "Open in Playground ->" in accent colour
   above the tabs. Click navigates to `/playground?example=<current>`.
2. Add a hint line below the code: `"Edit this live ->"` with an
   arrow icon, same target.
3. Tab switching (token/nft/amm/vesting) is preserved; the CTA
   remembers the currently selected example.
4. The code area remains read-only; no edit affordances are added.

### 3.1 Visual reinforcement (on-chain aesthetics, no effects)

- Add a static compile-output indicator line to `.code-head`:
  `token.cell - 4 types - 2 actions - Pure/Mutating` in mono `--dim`.
  This communicates "this is a real compile summary" without
  animation.
- Dim the aurora by lowering `--page-glow` opacity 0.24 -> 0.14 so the
  code area becomes the visual subject rather than competing with the
  background.

The hero's job is to entice and route, not to host an IDE.

## 4. Playground page (`/playground`) - the real IDE

### 4.1 Layout (three columns, no effects, layered density)

```
+-------------------------------------------------------------+
| [CellScript]  playground            [token v] [Format]      |  top bar: example switch + format
+--------------+--------------------------+-------------------+
|              |                          |                   |
|  SOURCE      |     COMPILE OUTPUT       |  PROVENANCE       |
|  (editor)    |  (tabbed, 4 views)       |  (persistent rail)|
|              |                          |                   |
|  module      | [Metadata][AST][Types]   |  module:          |
|  cellscript  | [Errors]                 |   cellscript::... |
|  ::token     |                          |  target: ckb      |
|              |  {                       |  --------------   |
|  resource    |    "module": "...",      |  ACTIONS (2)      |
|  Token ...   |    "types": [...],       |  > transfer       |
|              |    "actions": [...]      |     Pure          |
|  action      |  }                       |     consume:Token |
|  transfer... |                          |     create:Token  |
|              |                          |     3.2k cycles   |
|              |                          |  > burn           |
|              |                          |     Mutating      |
|              |                          |     ...           |
|              |                          |                   |
+--------------+--------------------------+-------------------+
| status: compiled in 12ms - 4 types - 2 actions - 0 errors   |  bottom bar: compile summary
+-------------------------------------------------------------+
```

Column widths and information layering rationale:

- **Left SOURCE** (32%): the user's edit surface. Mono 14px, line
  numbers. The origin of all downstream state.
- **Middle OUTPUT** (40%): compile artifacts. Four tabs switch views.
- **Right PROVENANCE** (28%): the always-on human-readable summary
  that is CellScript's signature feature. Updating this on every edit
  is the live demonstration of `effect_class` / `consume_set` /
  `create_set`.

### 4.2 Editor (zero-dependency textarea + overlay highlight)

The playground does **not** adopt Monaco or CodeMirror as a code
dependency. Rationale:

- **Size.** Monaco ships ~3.5MB; CodeMirror 6 is ~300KB before a
  custom language grammar. Either would dominate the WASM budget or
  double the JS payload for no CellScript-specific benefit.
- **No language support.** CellScript is a custom DSL; no editor
  ships a grammar for it, so the value of a heavy editor collapses to
  "line numbers and bracket matching", both of which are trivial.
- **Reuse.** The existing `classifyCellToken()` tokenizer
  (`index.astro:47`) already drives hero syntax colouring and is
  semantically aligned with the compiler's token classes.

Implementation: a transparent `<textarea>` carrying the caret, with an
absolutely-positioned `<pre>` overlay rendering the highlighted
output of `classifyCellToken()`. A `requestAnimationFrame` pass
re-renders the overlay on input; a 300ms debounce triggers the WASM
compile.

Mature editor projects are still valuable as **reference clones**
(behaviour to imitate, not code to vendor) - see Appendix A.

### 4.3 Compile trigger (debounced)

```
input -> debounce 300ms -> wasm.compile_metadata_json(source) ->
  parse JSON -> update OUTPUT tabs + PROVENANCE rail + bottom status
```

- 300ms debounce prevents per-keystroke compiles and avoids flicker.
- Errors carry span info; the editor renders them as red wavy
  underlines on the offending line.
- The WASM module loads once per page session and is cached; a
  skeleton screen shows during first load.

### 4.4 Four OUTPUT views (tab switching, reusing the model/tooling tab pattern)

| Tab      | Content                                                       | Source                          |
|----------|---------------------------------------------------------------|---------------------------------|
| Metadata | `CompileMetadata` JSON, syntax-highlighted                    | `compile_metadata_json`         |
| AST      | Indented tree: `module / resource / action / where` nodes     | `CompileResult.ast` (path B) or JSON `lowering` |
| Types    | Per-type `kind / capabilities / encoded_size / flow_states`   | `CompileMetadata.types`         |
| Errors   | Compile diagnostics with line numbers and messages            | parsed error JSON               |

### 4.5 PROVENANCE rail (playground variant, live)

The right rail is the live, human-readable summary:

- **Module + Target** pinned at top.
- **Actions list**: each action is expandable; shows `effect_class`,
  `consume_set`, `create_set`, `estimated_cycles`,
  `ckb_runtime_features`.
- **Types overview**: `Token (resource, 5 capabilities, 16 bytes)` as
  a collapsed list.

Editing one line of source updates the rail's consume/create
instantly - this is the assurance concept demonstrated as a living
UI, not described in a JSON block.

### 4.6 Example data

Reuse `src/data/site.ts` `heroExamples` (token/nft/amm/vesting). The
top-bar `[token v]` switches between presets; users may also edit
freely.

### 4.7 Performance and feel

- WASM loads asynchronously via `<script type="module">`; it does not
  block first paint.
- Playground first paint: render the three-column skeleton with
  "Loading compiler...", then auto-compile the initial example once
  the module is ready.
- Compile latency on the metadata path is typically <20ms on desktop,
  reading as instantaneous.

## 5. Transition navigation metaphor (across index.astro)

### 5.1 Mechanism

On each section change (scroll into a new section, or nav anchor
click) a single line flashes at the bottom of the screen:

```
action: view_core_model | consume: [hero] | create: [model] | effect: Pure
```

- **Position**: fixed bottom-centre, above the footer.
- **Timing**: 400ms fade in, hold 1200ms, 400ms fade out (2s total,
  non-intrusive).
- **Trigger**: an `IntersectionObserver` over the six sections; on
  entering a new section, fire once.
- **Reduced motion**: hidden entirely; the nav instead statically
  shows the current section name.

### 5.2 Transition caption map (per section)

| Section          | action             | consume        | create          | effect     |
|------------------|--------------------|----------------|-----------------|------------|
| hero             | `init`             | `[]`           | `[landing]`     | `Pure`     |
| workflow         | `view_workflow`    | `[landing]`    | `[workflow]`    | `Pure`     |
| core-model       | `view_model`       | `[workflow]`   | `[model]`       | `Pure`     |
| assurance        | `view_assurance`   | `[model]`      | `[assurance]`   | `Pure`     |
| tooling          | `view_tooling`     | `[assurance]`  | `[tooling]`     | `Pure`     |
| examples         | `view_examples`    | `[tooling]`    | `[examples]`    | `Pure`     |
| getting-started  | `start`            | `[examples]`   | `[playground]`  | `Mutating` |

The final row uses `Mutating` as a small conceit: "starting to use"
is the only state-changing action. Captions live in
`translations.ts` with EN/ZH strings.

### 5.3 Visual style (no effects, but textured)

```
+-------------------------------------------------+
| action: view_model . consume [workflow]         |  mono, --dim, fixed bottom
| -> create [model] . effect: Pure                |
+-------------------------------------------------+
```

- Background: `--panel-strong` + `backdrop-filter: blur(8px)`.
- Text: mono 13px, `--dim`; the action name uses `--accent`.
- No positional animation; opacity transition only, honouring
  `prefers-reduced-motion`.

## 6. Provenance rail (landing page, persistent)

### 6.1 Layout

- Desktop (>=1100px): a fixed 240px right rail beside the main
  content.
- Mobile (<1100px): hidden, replaced by an in-section collapsible
  card.

### 6.2 Content (changes with scroll context)

| Current section | Rail shows                                                       |
|-----------------|------------------------------------------------------------------|
| hero            | "CellScript v0.20 - target: ckb - 4 examples"                    |
| core-model      | Current tab's type metadata (`kind / capabilities / size`)       |
| assurance       | Current example's `effect_class / consume / create / proof_plan` |
| examples        | Currently hovered example's metadata summary                     |

### 6.3 Data source

The landing page is static, so these metadata are **precompiled** at
build time: a single run of `cellc metadata` produces
`src/data/provenance.json`, which the rail reads. The landing page
does **not** load WASM (only the playground does), keeping it light.

## 7. On-chain aesthetics system (site-wide)

### 7.1 New CSS variables (token system)

```css
:root {
  /* substrate aesthetics: bytes/hash/cycle */
  --substrate-bg: oklch(0.08 0.008 180);
  --substrate-text: oklch(0.62 0.01 180);
  --substrate-accent: oklch(0.7 0.15 60);   /* orange, for cycles/capacity */

  --font-substrate: var(--font-mono);
  --size-substrate: 0.7rem;

  --bar-track: oklch(0.15 0.004 180);
  --bar-fill: var(--accent);
}
```

### 7.2 Application points

- **Hero code footer**: a line `artifact: 0x7a3f...e2b1 - 12,847 cycles - 8.2 KB`
  from precompiled real data, static.
- **Assurance section**: each summary card gains a capacity bar
  `8.2 KB / 64 KB` (static fill).
- **Provenance rail**: hash/cycle values use `--substrate-accent`
  orange to distinguish them from brand green.
- **Uniform rule**: all "machine products" (hash/size/cycle) render
  in mono + small size + subtle orange; all "human copy" renders in
  sans + normal size + neutral. This forms a legible two-tier visual
  hierarchy: human-readable vs machine-produced.

## 8. Scroll-driven compiler narrative (Workflow section)

### 8.1 Current state

The Workflow section is a static 5-step flow diagram plus a checklist.

### 8.2 Refit (scroll-driven, but no scroll-triggered animation)

No `position: sticky` frame-by-frame animation (that would be an
effect). Instead, scroll-progress synchronisation:

- The five steps stack vertically, each one-third of a viewport tall.
- On scroll, a thin progress bar (2px, accent) on the left fills from
  0% to 100%, tracking the five steps.
- The current step highlights (accent border); the others dim.
- Each step embeds a real artifact fragment from that pipeline stage
  (precompiled):
  - Step 1 source: CellScript source fragment
  - Step 2 AST: indented tree fragment
  - Step 3 metadata: JSON fragment (consume/create)
  - Step 4 RISC-V: assembly fragment
  - Step 5 ELF: hex-dump fragment

This is not an effect: the progress bar is a CSS
`transform: scaleY()` driven by scroll position; step highlighting is
a class toggle. There is no keyframe animation and no JS animation
loop.

## 9. Information layering (why this density distribution)

| Layer | Content                                             | Visual                         | Carrier                                   |
|-------|-----------------------------------------------------|--------------------------------|-------------------------------------------|
| L0 hook | "What CellScript is" (hero wordmark + subtitle)   | Largest size, brand colour     | hero                                      |
| L1 evidence | "It runs for you" (playground link + pseudo-console) | Code highlight, mono       | hero console -> playground                |
| L2 concept | transition / model / assurance                    | Section titles + cards         | six sections                              |
| L3 machine products | hash / cycle / bytes / metadata         | Small mono + orange accent     | provenance rail + substrate line          |
| L4 nav metaphor | consume/create/effect                         | Bottom mono flash line         | transition overlay                        |
| L5 action | "Try it" (playground + getting started)           | CTA buttons                    | getting-started section                   |

**Principle**: density rises by layer, visual weight falls by layer.
A user scanning sees L0-L1; the curious read L2; the technically
curious inspect L3-L4; those ready to act reach L5.

## 10. Ergonomics detail (human factors)

This section records the ergonomic decisions that make the site and
playground comfortable to use for extended periods. Numbers are
targets, to be verified during implementation.

### 10.1 Typography and reading

- Body text base 16px, line-height 1.55 for paragraphs; 1.45 for
  mono. The existing scale is retained (refined in prior commits).
- Measure (line length): cap prose at 64-72 characters
  (`max-width: 60ch`) to keep saccade length comfortable. Code panels
  are exempt.
- Mono substrate text (hash/cycle/size) at `0.72rem` (11.5px) floor;
  never below. This is the legibility floor after the earlier
  contrast pass.
- Hero subtitle weight 400 (already landed); h1 line-height 1.05
  (already landed) to avoid descender clipping.

### 10.2 Editor ergonomics (playground)

- Source editor line-height 1.6 and 14px font; the textarea and the
  overlay `<pre>` must share identical `font-family`, `font-size`,
  `line-height`, `letter-spacing`, `padding`, and `tab-size` so the
  caret never drifts from the highlight. A shared CSS class enforces
  this.
- Tab key inserts two spaces (not focus-stealing); handled in the
  textarea `keydown`.
- Visible caret: `caret-color: var(--accent)` so the insertion point
  is findable in a dense mono block.
- Error underline: `text-decoration: underline wavy var(--error)`
  on the overlay span; hover reveals the message in a non-blocking
  inline tooltip (not a system tooltip, which is jarring).
- Auto-indent on Enter: the editor copies the leading whitespace of
  the current line to the next. No smart-indent heuristic (keeps
  behaviour predictable).

### 10.3 Layout and viewports

- Content max-width 1180px retained. With the 240px provenance rail,
  the main column reduces to ~900px so prose measure stays in range.
- Breakpoints: 1100px (rail folds to card), 820px (three columns ->
  stacked tabs), 390px (single column, playground editor full width).
- Playground columns become tab-switchable below 820px:
  `[Source][Output][Provenance]`, one visible at a time, to preserve
  editor width on tablets.
- No horizontal scroll anywhere; `overflow-x: clip` on `html`
  (already present).

### 10.4 Motion and accessibility

- `prefers-reduced-motion: reduce`:
  - transition overlay hidden; nav shows current section statically.
  - Workflow progress bar renders as a static fill at the last
    position, not animated by scroll.
  - The existing language-switch fade is already skipped (prior work).
- `prefers-color-scheme`: not used for theming (the site has an
  explicit toggle); but the default theme on first visit honours
  `prefers-color-scheme` if no stored preference exists (already the
  case via the inline `<head>` script).
- All new interactive elements get `:focus-visible` outlines matching
  the site standard (2px accent, 3px offset).
- Touch targets >= 44x44px on all new controls (CTA, rail toggles,
  playground tabs).
- The transition overlay caption is `aria-live="polite"` so screen
  readers announce section context without interrupting.

### 10.5 Cognitive load and pacing

- One concept per section. The six sections map 1:1 to one idea each;
  no section teaches two things.
- The transition overlay gives a one-line "you are here" breadcrumb
  that orients without nagging; 2s display then it is gone.
- The provenance rail never auto-scrolls or flashes; it updates values
  in place so the eye does not have to chase movement.
- Playground compile latency is kept under 50ms perceptible by the
  300ms debounce (the user finishes a keystroke cluster before the
  compile fires); the bottom status bar confirms "compiled in Xms" to
  make the cause-effect visible and reassuring.
- Error messages are actionable: they name the line and the expected
  shape, never just "syntax error".

### 10.6 Colour and contrast

- All new text passes WCAG AA (4.5:1 for small text, 3:1 for large),
  verified by the same Playwright contrast probe used in prior passes.
- The new `--substrate-accent` orange is only used on
  >=0.72rem mono text on dark substrate backgrounds; its contrast is
  verified per use, not assumed.
- Brand green (`--accent`, hue 158) remains the sole accent for
  interactive affordances; orange is reserved for data, so colour
  semantics stay stable: green = "you can act", orange = "machine
  value".

### 10.7 Performance feel

- First Contentful Paint on the landing page must stay under 400ms
  (no WASM load there); the playground may reach 3s including WASM,
  with a skeleton screen masking the wait.
- The WASM module is served pre-compressed (`.wasm.br` or `.wasm.gz`)
  with correct `Content-Encoding`; the build step produces both.
- The playground editor overlay re-renders on
  `requestAnimationFrame`, never on raw `input`, to coalesce bursts.

## 11. WASM bundle size plan

### 11.1 Budget

- Target: `.wasm` gzipped <= 600KB for v1 (path A, metadata only).
- Ceiling: 800KB gzipped. Beyond that the playground is not worth the
  cost; reconsider the editor or the compile path.

### 11.2 Controls (in priority order)

1. `--release` build with
   `RUSTFLAGS="-C opt-level=z -C lto=fat -C codegen-units=1"`.
2. Export only `compile_metadata_json` in v1. The codegen module's
   large body is then largely dead-stripped because no public
   function references the ELF path. Path B (full ELF) is a separate
   v2 build that is allowed to be larger.
3. `[profile.release] panic = "abort"` to drop unwinding tables.
4. `wasm-opt -Oz` post-processing (typically 10-15% further).
5. Serve `.wasm.gz` / `.wasm.br` pre-compressed.
6. CI guard: `gzip -c public/wasm/*.wasm | wc -c` fails the build if
   it exceeds the budget.

### 11.3 Two-path split

- **Path A (v1, this RFC):** `compile_metadata_json` only. Renders
  AST summary, types, actions, metadata. No ELF bytes. Keeps the
  bundle within budget and still demonstrates the typed-transition
  value proposition live.
- **Path B (v2, deferred):** add `compile_full_json` returning ELF
  hex, `artifact_size_bytes`, `artifact_hash`, cycle estimate. Adds
  the internal ELF assembler to the bundle (estimated +250-350KB
  gzipped). Re-evaluate the budget before enabling; until then the
  playground shows a "ELF output available in v2" note where the
  bytes would appear.

## 12. Execution plan (four PRs, independently mergeable)

| PR         | Content                                                                     | Depends on | Effort   |
|------------|-----------------------------------------------------------------------------|------------|----------|
| PR-3 first | Landing refit: pseudo-console entry, provenance rail, transition overlay, on-chain aesthetics | none (no WASM) | 1 day    |
| PR-1       | WASM module: Cargo.toml features, lib.rs cfg, `crates/cellscript-wasm`, build script | none       | 0.5 day  |
| PR-2       | Playground page: `/playground`, editor, four tabs, WASM integration        | PR-1       | 1.5 days |
| PR-4       | Scroll narrative: Workflow progress bar + artifact fragments                | none       | 0.5 day  |

**Recommended merge order: PR-3 -> PR-1 -> PR-2 -> PR-4.** PR-3 lands
first because it needs no WASM and is immediately visible. PR-1/PR-2
are the core technical debt; PR-4 is polish.

## 13. Acceptance criteria

### Functional

- `/playground` loads in < 3s (including WASM); after input, compile
  output refreshes within 300ms + compile time.
- Hero "Open in Playground" carries the example query param; the
  playground auto-loads it.
- Landing provenance rail updates as the user scrolls between
  sections.
- Transition overlay flashes on section change; hidden under
  reduced motion.
- The four playground tabs (Metadata/AST/Types/Errors) render
  correctly.

### Non-functional

- WASM `.wasm` gzipped <= 600KB.
- Landing Lighthouse Performance >= 90; playground >= 80 (WASM load
  accepted).
- `prefers-reduced-motion`: overlay hidden, workflow bar static.
- Mobile: provenance rail folds; playground three columns become
  tabs.
- WCAG AA across all new text.

### Explicit non-goals (hard constraints)

- No keyframe animations beyond the existing three.
- No Monaco/CodeMirror as a code dependency (textarea + overlay only).
- No WASM load on the landing page (playground only).
- No path B (ELF compile) in v1 (bundle size control).

## 14. Risks and mitigations

| Risk                                              | Likelihood | Mitigation                                                                |
|---------------------------------------------------|------------|---------------------------------------------------------------------------|
| WASM size exceeds 600KB                           | Medium     | Path A only; `wasm-opt -Oz`; CI size guard                                 |
| Textarea overlay caret drift                      | Low        | Shared CSS class for identical metrics; mobile testing                     |
| Transition overlay distracts readers              | Medium     | 2s display; reduced-motion hides it; bottom placement avoids content      |
| Provenance rail consumes desktop width            | Low        | 240px fixed; main column recalculated; folds on mobile                     |
| Slow first WASM load                              | Medium     | Async load + skeleton; CDN cache; pre-compressed asset                     |
| `compile_metadata` pulls in unintended native code | Low        | `cargo check --lib --target wasm32-unknown-unknown` gate in CI             |

## Appendix A: Reference editor projects (clone, not vendor)

The playground editor is hand-rolled (§4.2), but the following
projects are studied as **reference clones** for behaviour and feel -
their UX patterns are imitated, their code is not vendored:

- **Monaco Editor** (Microsoft) - reference for: stable caret model,
  bracket pair guides, minimap concept (deliberately omitted for
  size), error squiggle styling, and the "peek" affordance for
  diagnostics. We clone the *feel* of squiggles and diagnostics
  presentation, not the editor core.
- **CodeMirror 6** (Marijn Haverbeke) - reference for: the
  imperative Decoration API pattern (we approximate it with a
  re-tokenised overlay), line-number gutter behaviour, and mobile
  touch handling for code. CM6's compartmentalised architecture is
  the conceptual model for keeping the overlay re-render cheap.
- **Rust Playground** (`play.rust-lang.org`) - reference for: the
  three-pane source/output/diagnostics layout, the
  debounce-then-compile interaction, the "share URL" pattern (encode
  source in the fragment so it is shareable without a backend), and
  the status bar that reports compile time. This is the closest
  precedent for a compiler DSL playground and the primary UX
  template.
- **Svelte REPL** (`svelte.dev/repl`) - reference for: instant
  recompile feel, the left-editor/right-output split, and
  example-switching without losing user edits (history stack).
- **TypeScript Playground** - reference for: the multi-tab output
  (AST/JS/DTS) which directly informs the Metadata/AST/Types/Errors
  tab design, and the inline error rendering below the source line.
- **Etherscan / Foundry output** - reference for: the
  "show-your-work" substrate aesthetic (transaction hashes, bytecode
  hex, cycle/gas figures presented with pride, not hidden), which
  informs the on-chain aesthetic system in §7.

The Rust Playground layout and the TypeScript Playground multi-tab
output are the two strongest influences on §4.1 and §4.4 respectively.

## Appendix B: Precedent mapping

| Proposed feature            | Closest precedent            | What we do differently                                     |
|-----------------------------|------------------------------|------------------------------------------------------------|
| Live compiler in hero/playground | Rust Playground, Svelte REPL | First live-compile blockchain DSL playground; on-chain substrate aesthetic |
| Transition as nav metaphor  | (none observed)              | Novel: the language's core primitive becomes the navigation language |
| Provenance rail             | TypeScript Playground "AST" tab | Persistent context rail tied to assurance metadata, not on-demand |
| On-chain substrate aesthetic | Etherscan, Foundry          | Promoted from debug output to primary design language       |
| Scroll-driven pipeline      | Apple product pages, NYT interactives | Applied to a compiler pipeline, with real intermediate artifacts |

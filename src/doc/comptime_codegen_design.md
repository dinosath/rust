# Compile-Time Item Generation: Prototype Design

## Goal

Add an opt-in, framework-neutral path for compile-time code to request ordinary
Rust functions and statics. The compiler materializes requests as AST items
before name resolution. The resulting items then use the normal resolver,
type checker, borrow checker, lint, monomorphization, and codegen paths.

This is a Rust-native design inspired by Zig's compile-time programming model,
not an attempt to copy Zig syntax. The initial scope is typed item generation;
general compile-time type/value computation can grow from the same evaluator
and semantic type representation later.

## Constraints

- Do not generate or parse arbitrary Rust source strings.
- Do not implement generation as a procedural macro, `build.rs`, linker
  inventory, or runtime registration mechanism.
- Do not add framework-specific annotations or policies to rustc.
- Keep compiler-only identities out of runtime item data.
- Generated functions and statics must be caller-named and normally visible
  according to their requested visibility and placement.
- Invalid requests must produce normal diagnostics, not an ICE.

## Proposed Source Model

An unstable, opt-in compile-time item function returns a compiler-owned
`ItemSet` description. The current descriptor prototype uses fixed slices and
const constructors:

```rust,ignore
const PARAMETERS: &[Parameter] = &[Parameter::new("value", TypeId::of::<u32>())];
const BODY: Expression = Expression::Binary {
  op: BinaryOp::Add,
  lhs: &Expression::Parameter(0),
  rhs: &Expression::Integer { ty: TypeId::of::<u32>(), bits: 1 },
};
const REQUESTS: &[Item] = &[Item::new(
  &[],
  ItemVisibility::Public,
  ItemKind::Function(Function::new(
    "increment",
    PARAMETERS,
    TypeId::of::<u32>(),
    false,
    BODY,
  )),
)];

#[rustc_comptime]
fn generated_items() -> ItemSet {
  ItemSet::new(REQUESTS)
}
```

This API currently produces descriptor values only; rustc does not yet consume
them to generate declarations. The schema must not accept source text as its
input language. Names are identifiers, types are semantic type references,
and bodies/initializers are structured typed expressions.

The first prototype should deliberately bound its surface:

- Free functions and statics only; no impls, trait impls, or generated modules.
- One request is placed in the module containing its generator unless a later
  placement design is approved.
- No generic generated items in the first slice.
- Fixed-size descriptor collections initially; const heap allocation is not
  a prerequisite.
- Function bodies and static initializers use a small expression-description
  API. The initial node set should be chosen by the AST insertion spike, not
  guessed in advance.

## Descriptor Boundary

The evaluated result is a compiler-owned, typed request model, not a runtime
registry. It contains:

- An item kind and caller-selected identifier.
- Visibility and module placement.
- A structural signature or static type, retaining semantic type identity.
- Structured expression nodes for the body or initializer.
- An origin referring to the generator invocation for diagnostics.

The request model must not contain `NodeId`, `HirId`, `DefId`, `Span`, hygiene
context IDs, or `TypeId` values intended to survive into runtime code. If the
compiler uses a separate analysis and final-compilation context, requests need
a stable handoff representation. Type references should encode local/external
definition identity plus structural generic arguments, then be resolved in the
final compilation context. Names used only for display are not type identity.

Generated node IDs, spans, and hygiene contexts are allocated in the final
context. Generated items inherit the generator invocation as their diagnostic
origin, while identifiers receive a fresh context so generated names do not
capture unrelated source bindings.

## Pipeline Proposal

### Pass A: Analyze requests

1. Parse and expand the source crate.
2. Resolve and type-check enough of the crate to build MIR for item generators
   and expose reflection metadata.
3. Evaluate opted-in generators through CTFE and collect typed requests.
4. Preserve generator diagnostics and associate each request with its source
   invocation.

This pass must not emit code. Source references to generated names are expected
to be unresolved during this pass; the prototype must determine how rustc can
continue evaluating independent generators without hiding unrelated errors.

### Pass B: Final compilation

1. Reuse the expanded source crate, or replay expansion under a proven
   deterministic contract.
2. Materialize typed requests as AST items in their target modules.
3. Run name resolution and all ordinary later compiler phases on the augmented
   crate.
4. Report generated-item conflicts and type errors through normal diagnostics
   at the generator invocation or the most specific available generated span.

The insertion point is immediately before `Resolver::resolve_crate` in
`rustc_interface::passes::configure_and_expand`. That point solves ordering,
but it does not by itself solve how Pass A hands requests and expanded syntax
to Pass B.

## Architecture Alternatives

### Re-enter the compiler with a retained expanded crate

This avoids running procedural macros twice and preserves their expansion
result. It requires proving that expanded AST, source files, spans, and hygiene
can be transferred into a fresh resolution/type-check context without retaining
query-local IDs. This is the preferred direction if rustc can support it
without destabilizing incremental compilation or query invariants.

### Re-run the full compiler and expand twice

This is mechanically simpler to prototype but macro expansion is not generally
a deterministic or side-effect-free operation. It can duplicate diagnostics,
observe changed external state, or produce different AST. It is not an
acceptable default unless the prototype explicitly constrains/rejects such
inputs and proves diagnostic behavior.

### Inject after resolution or type checking

Rejected for the initial feature. Items added after resolution cannot be
referenced from ordinary source and will not receive the normal compiler
pipeline required by the goal.

## Diagnostics and Failure Model

- Generator evaluation failures point at the generator invocation and retain
  the underlying const-evaluation explanation.
- Malformed descriptors are rejected before AST construction with a stable
  compiler diagnostic; malformed requests must never panic or ICE.
- Duplicate names, privacy violations, unresolved paths, type errors, and
  coherence errors are diagnosed by the normal final compiler passes.
- Pass A must not report unresolved generated names as user errors. It must
  also not suppress unrelated source errors; the prototype needs an explicit
  diagnostic classification/collection strategy.
- Requests are all-or-nothing per crate for the first prototype. If any
  generator fails, do not partially insert items.

## Determinism and Incremental Compilation

Requests are ordered by stable source definition order and request order within
each generator. No hash-map iteration order may affect generated item order.
The compiler must define a stable fingerprint for the request model and include
it in the appropriate query dependency graph. Generated identifiers and
hygiene derive from stable request identity, not process-local counters alone.
No generation registration symbols or reflection IDs are emitted at runtime.

Macro expansion replay, request fingerprinting, and the ownership of generated
items in incremental compilation are open design gates; they must be resolved
before enabling the feature outside an internal prototype.

## Implementation Sequence

1. Add a compiler-internal typed AST item builder and an insertion test using a
   hand-authored request. Cover crate root, inline module, and out-of-line
   module placement, plus ordinary resolution and type errors. This spike must
   not pretend to provide CTFE generation.
2. Prototype retaining/re-entering an expanded crate in a fresh compiler
   context. Verify source maps, expansion diagnostics, hygiene, and ordinary
   source references to generated names.
3. Harden the prototype descriptor schema and validate malformed requests
  before enabling it beyond the unstable `core` API.
4. Connect CTFE output from an opt-in item generator to the request model;
  prove requests remain deterministic across clean and incremental builds.
5. Lower the evaluated requests to AST items, then verify name resolution,
  type checking, diagnostics, and generated-name references.
6. Only after these gates pass, build framework integrations on the generic
  API.

## Open Questions Before Compiler Changes

- Can the expanded AST and its source-map/hygiene inputs be safely reused by a
  second `TyCtxt`, or must a serializable expanded-crate format be designed?
- How does Pass A continue through unresolved references to generated names
  while preserving all unrelated errors and generator evaluation?
- Which typed expression nodes provide a useful first body/initializer API
  without becoming a second Rust parser or an underspecified mini-language?
- How are dependency type identities represented in requests without carrying
  compiler-session IDs across passes?
- What is the incremental-compilation invalidation boundary when a generator's
  output changes but its source signature does not?

Until these questions are answered by the pipeline spike, the two-pass proposal
is a hypothesis, not an established rustc architecture.

## Prototype Progress

The unstable `core::mem::type_info` surface now has a value-level descriptor
schema for item sets, functions, statics, parameters, and structured expression
nodes. Its builder constructors are comptime-only, and UI tests prove a
`#[rustc_comptime]` function can produce a descriptor containing semantic
`TypeId`s, a module path, visibility, and a typed expression tree. A negative
test verifies the builder cannot be called at runtime. Supported expression
nodes currently include parameters, integer and boolean literals, calls,
tuple/struct construction, field access, basic binary operations, and `await`.

The separate AST-layer spike has an internal request type for functions and
statics in `rustc_expand::comptime_codegen`. Requests retain caller-provided
AST types, expressions, signatures, names, and visibility; the active monotonic
expander assigns missing node IDs and expands the resulting item fragment.
Generated items can be placed at the crate root or into an already-loaded
inline/out-of-line module. Unit tests cover materialization and all three
placement forms.

The descriptor schema is not connected to the AST request helper or compiler
driver. The compiler does not yet evaluate a generator to an item set, lower
expression descriptors, insert generated items, or validate them through name
resolution and type checking. These prototypes therefore do not yet satisfy
generated-item acceptance criteria.
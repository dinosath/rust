# Requirements for Compile-Time Framework Support

## Purpose

This document defines compiler capabilities needed by framework crates such as
`ct-di` and `ct-web`. It is a requirements document, not a statement that the
capabilities are already implemented.

The compiler provides generic reflection and typed-item generation. Framework
crates define their own annotations, validation rules, generated APIs, and
runtime behavior. The compiler must not contain framework-specific knowledge.

## Design Boundary

### Compiler responsibilities

- Reflect over local crate items and, where explicitly requested, reachable
  dependency items.
- Expose item identity, kind, name, visibility, generic arguments, fields,
  field types, function parameter names and types, return types, asyncness,
  attributes, and source spans.
- Evaluate compile-time functions that inspect this metadata and produce a
  typed description of additional Rust items.
- Insert generated items into the ordinary compiler pipeline so they receive
  name resolution, type checking, borrow checking, linting, monomorphization,
  and code generation like source-written items.
- Diagnose invalid metadata or generated-item conflicts at the relevant
  source attribute or item, with stable and understandable diagnostics.
- Make generation deterministic and hygienic across crate-root, inline, and
  out-of-line modules.

### Framework responsibilities

- `ct-di` owns provider/config annotations, dependency scopes, graph validation,
  configuration mapping, initialization order, and the generated application
  state/bootstrap API.
- `ct-web` owns route and middleware annotations, HTTP method and path parsing,
  route conflict policy, request extraction, response conversion, router data
  structures, and OpenAPI semantics.
- Framework crates use compiler reflection and item-generation APIs; they
  implement their policies in ordinary framework code rather than adding
  framework-specific branches to rustc.

In particular, rustc must not know about `app::route`, HTTP verbs, path
parameters, Axum, SeaORM, repositories, middleware ordering, or DI lifetimes.
The compiler may treat tool attributes as generic metadata and expose their
syntax and spans without interpreting their meaning.

## Generic Reflection Requirements

Reflection must provide enough typed information for a framework to analyze
items without parsing source text:

- Enumerate annotated items in the current crate, with an explicit policy for
  dependency traversal and visibility.
- Read tool-attribute paths, structured arguments, and source spans. Literal
  values must remain distinguishable from identifiers and paths.
- Inspect structs, enums, and unions, including field names, field types,
  visibility, generic parameters, and field attributes.
- Inspect functions and methods, including parameter names/patterns, parameter
  types, declared output types, asyncness, generics, visibility, and attributes.
- Preserve semantic `TypeId`/type references. Type names may be exposed for
  diagnostics but must not be used as type identity.
- Represent unsupported or ambiguous metadata explicitly so frameworks can
  issue diagnostics instead of silently omitting items.

Reflection itself is compile-time-only. It must not create a runtime registry,
retain compiler-only IDs in generated runtime data, or make runtime values
available to compile-time evaluation.

## Typed-Item Generation Requirements

Compile-time code must be able to request typed Rust items without producing
or reparsing arbitrary source strings. The first useful item forms are:

- Functions and associated functions with typed parameters, return types,
  bodies, visibility, and asyncness.
- Static and constant items with typed expressions.
- Inherent and trait implementations, including generated methods and trait
  implementations where coherence permits them.
- References to existing items and types using semantic paths resolved in the
  generation context.

The API may use a compiler-owned item description/builder, but it must remain
framework-neutral. Generated items must:

- Be inserted before name resolution and type checking of their containing
  module, including after out-of-line modules are loaded.
- Receive valid node IDs, spans, hygiene context, and resolver bookkeeping.
- Be visible to sibling and downstream items according to normal Rust
  visibility and privacy rules.
- Fail through ordinary compiler diagnostics when types, names, traits, or
  generated-item coherence are invalid.
- Be reproducible for the same source and compiler inputs.

The framework supplies the complete item request, including its chosen name,
signature, visibility, body, and placement. Rustc only validates and
materializes that generic typed request. For example, `ct-di` may choose to
request an associated constructor named `__ct_di_new`; rustc must not invent,
recognize, or require that name. Likewise, `ct-web` chooses names and shapes
for its route table, dispatch functions, and handler wrappers. Those names
and HTTP-specific types must never appear in compiler generation logic.

Conceptually, the boundary is:

```text
ct-di analysis -> generic typed item request (ct-di chooses API and names)
ct-web analysis -> generic typed item request (ct-web chooses API and names)
                         |
                         v
               rustc materializes items
```

Compiler tests must exercise the generic request mechanism with a neutral
fixture. Framework tests must verify that `ct-di` and `ct-web` construct the
requests they need. A rustc test for DI must not assert the existence of
`__ct_di_new`, and a rustc test for web routing must not mention routes or
HTTP methods.

Item generation must not be implemented as a hidden procedural macro, linker
inventory, `build.rs` code generation, or runtime reflection registry.

## `ct-di` Requirements

Using only generic compiler facilities, `ct-di` must be able to:

- Discover provider and configuration declarations in an application crate.
- Read constructor dependencies from provider fields or provider function
  signatures, retaining exact types and source locations.
- Validate missing and ambiguous providers, dependency cycles, and invalid
  scope relationships at compile time.
- Determine a stable initialization order and generate typed constructors and
  an application state type without a runtime `TypeId` map or hash map. `ct-di`
  defines the constructor names and generated API, then submits those items
  through the generic compiler item-generation interface.
- Generate an async initialization function when providers perform runtime
  work such as opening a database connection.
- Generate typed config loading from runtime sources such as environment
  variables, applying declared defaults and reporting missing/invalid values
  at startup.
- Keep runtime state ownership, cloning, and request/singleton lifetimes
  explicit in generated Rust types.

Compile-time analysis determines the graph; it does not connect to databases,
read runtime environment values, or perform other startup I/O.

## `ct-web` Requirements

Using only generic compiler facilities, `ct-web` must be able to:

- Discover route handlers and middleware functions from its own attributes.
- Match handler parameter names and types to path, query, body, header, and
  injected application-state inputs.
- Validate route syntax, duplicate method/path pairs, unsupported signatures,
  and response compatibility with diagnostics at the attributed item.
- Generate typed handler wrappers and direct calls to handlers, with
  extraction and response conversion delegated to `ct-web` adapters. `ct-web`
  defines the generated names and signatures, then submits them through the
  same generic compiler interface used by `ct-di`.
- Generate a static route representation and dispatcher during compilation.
- Compose middleware according to `ct-web` policy without compiler knowledge
  of middleware concepts.
- Generate OpenAPI data from route/type metadata in `ct-web`, not in rustc.

The generated server path should not require runtime route registration,
linker collection, hash-map routing, or per-request compiler reflection. Any
zero-allocation requirement belongs to framework-generated/runtime code and
must be measured there, not enforced through HTTP-specific compiler logic.

## Pipeline Contract

The intended division of work is:

```text
ct-rustc generic reflection
        |
        v
ct-di / ct-web compile-time analysis and validation
        |
        v
ct-rustc typed-item generation API
        |
        v
ordinary Rust resolution, type checking, borrow checking, and codegen
        |
        v
typed application startup and static framework data
```

Framework compile-time analysis must be able to pass a typed item description
back to the compiler without depending on HTTP or DI-specific compiler APIs.

## Acceptance Criteria

### Compiler

- A compiler test uses a non-framework custom tool attribute and requests a
  generated typed function and static with caller-selected names and types.
- Generated items work in the crate root, an inline module, and an out-of-line
  module, and are callable/referencable from normal Rust source.
- Generated type errors, duplicate definitions, visibility violations, and
  malformed descriptions produce normal diagnostics rather than an ICE.
- Compiler crates contain no dependency on or special cases for `ct-di`,
  `ct-web`, HTTP methods, routes, repositories, or application annotations.
- Compiler tests do not assert framework-specific names such as
  `__ct_di_new` or `__CT_WEB_ROUTER`.
- Generated code is deterministic and contains no compiler reflection IDs or
  registration symbols at runtime.

### `ct-di`

- A framework-level test validates a resolvable graph and rejects missing,
  ambiguous, cyclic, and scope-invalid graphs.
- A test app uses generated constructors/state/bootstrap and has no manually
  wired provider chain or runtime provider map.
- Async providers are initialized at startup in dependency order.

### `ct-web`

- A framework-level test generates and exercises a static route structure
  from route metadata, including path parameters and method conflicts.
- Generated dispatch invokes parameterized async handlers with typed
  extraction and does not require a runtime route registry.
- HTTP and OpenAPI rules are implemented and tested in `ct-web`; compiler tests
  remain independent of those rules.

## Implementation Order

1. Finalize a generic reflected-item metadata contract, including source spans,
   function parameters, and type references.
2. Prototype a framework-neutral typed-item description for generated
   functions and statics, then validate insertion across module boundaries.
3. Add generated inherent/trait implementations and useful diagnostics.
4. Migrate `ct-di` from graph validation to generated constructors, state, and
   async bootstrap.
5. Migrate `ct-web` from route metadata to generated wrappers, static routing,
   and dispatch.
6. Add end-to-end examples and codegen/runtime tests for both frameworks.

Each stage must be usable and tested without adding framework-specific
semantics to rustc.

## Current Implementation Status

The reflection POC is behind `#![feature(type_info)]` (issue 146922). The
compiler changes and UI tests currently provide:

- `TypeId::has_attr` and `TypeId::attr_value`, plus corresponding `FieldId`
  queries for registered tool attributes and supported literal arguments.
- `crate_types_with_attr` for non-generic local ADTs, and
  `crate_functions_with_attr` for non-generic local functions.
- `all_functions_with_attr` for matching non-generic functions in the local
  crate and loaded dependencies.
- `TypeKind::FnDef` metadata for a function's name, declaration source span,
  parameter types, signature output type, declared output when recoverable,
  asyncness, and best-effort simple parameter names. Destructuring patterns and
  unavailable metadata produce `None`.
- A comptime-only descriptor schema in `core::mem::type_info` for caller-named
  functions and statics, semantic `TypeId` references, visibility/module paths,
  and structured expression nodes. UI coverage verifies descriptor production
  from `#[rustc_comptime]` and rejects runtime builder calls.
- Compile-time tests for attribute-based discovery, basic dependency-graph
  checks, runtime-use rejection, function metadata, and loaded dependencies.

This is a reflection prototype, not a completed implementation of the
requirements above. Attribute queries currently expose tool attributes with
supported literal values as strings; they do not expose general structured
metadata or attribute source spans. Function declaration spans are exposed
through `FnDef`, but other reflected items do not yet expose spans. Generic
items are excluded from enumeration, parameter patterns are not fully
represented, and `FnDef.output` for an async function is its future type;
`FnDef.declared_output` exposes `Future::Output` when recoverable. Compile-time
diagnostics are not yet span-targeted.

Typed-item generation is not implemented. `core::mem::type_info` has an
unstable comptime-only descriptor schema for function/static requests and
structured expressions; tests confirm a `#[rustc_comptime]` function can
produce descriptors and reject runtime builder calls. `rustc_expand` has a
separate internal AST request/materialization prototype with module placement
tests. The descriptor schema is not connected to the AST helper or compiler
driver, and no generated item has been validated through resolution or type
checking. There is still no complete framework-neutral generation pipeline for
generated functions, statics, or impls. Consequently, the `ct-di` and `ct-web`
framework requirements, generated-item acceptance tests, codegen checks, and
allocation measurements remain future work. In particular, compiler code must not
special-case application annotations, routes, HTTP methods, dependency scopes,
or router types to satisfy those goals.

The current prototype uses the existing `#[rustc_comptime]` mechanism to
produce `ItemSet` values containing typed function/static descriptors. These
values are not yet consumed as generated declarations. A future opt-in
generator marker and item-set discovery mechanism still need to be selected.
There is also a pipeline constraint: macro expansion and name resolution happen
before MIR-based const evaluation, while generated items must enter before
resolution and type checking. A viable prototype therefore needs an explicit
multi-pass design that retains or reconstructs the expanded crate, evaluates
descriptors, inserts items, and runs ordinary resolution/type checking over the
result. Re-expanding macros, preserving diagnostics, and handling source items
that refer to generated names must be addressed by that design; simply invoking
CTFE from the current expansion hook cannot work.

The reflection implementation lives primarily in
`compiler/rustc_const_eval/src/const_eval/type_info.rs` and `machine.rs`, with
intrinsic declarations in `library/core/src/intrinsics/reflection.rs`, the
public API in `library/core/src/mem/type_info.rs`, and UI coverage under
`tests/ui/reflection/`.

## Integration Constraints

- Compile-time graph analysis must not perform startup I/O or make runtime
  environment/configuration values available to const evaluation.
- Heap-backed graph construction and trait selection over reflected types need
  compiler/language support beyond the current reflection API; frameworks must
  not assume `Vec` const evaluation or a `TypeId`-to-trait-call bridge exists.
- Axum and SeaORM expose runtime router/query builders. Static route generation
  and compile-time SQL therefore belong to framework-owned designs and cannot
  be inferred from the current reflection API alone.
- A reflected `TypeId` currently has no general bridge to call a trait
  implementation for that type; the existing vtable facility is for dynamic
  traits and is not a general compile-time trait-selection API.
- Runtime values such as configured route paths or table names must remain
  runtime inputs. `#[rustc_comptime]` functions cannot be used to capture such
  values during compilation.

The next compiler milestone is to define generator discovery and CTFE result
extraction, connect the descriptor schema to AST materialization, and establish
the multi-pass handoff. Then prove generated-item insertion and ordinary
resolution/type checking across crate-root, inline, and out-of-line modules
before building framework integrations.
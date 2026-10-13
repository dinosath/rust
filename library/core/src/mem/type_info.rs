//! MVP for exposing compile-time information about types in a
//! runtime or const-eval processable way.

use crate::any::TypeId;
use crate::marker::PointeeSized;
use crate::ptr::DynMetadata;
use crate::{fmt, intrinsics};

/// Compile-time type information.
#[derive(Debug)]
#[non_exhaustive]
#[lang = "type_info"]
#[unstable(feature = "type_info", issue = "146922")]
pub struct Type {
    /// Per-type information
    pub kind: TypeKind,
}

/// Info of a trait implementation, you can retrieve the vtable with [Self::get_vtable]
#[derive(Debug, PartialEq, Eq)]
#[unstable(feature = "type_info", issue = "146922")]
pub struct TraitImpl<T: PointeeSized> {
    pub(crate) vtable: DynMetadata<T>,
}

impl<T: PointeeSized> TraitImpl<T> {
    /// Gets the raw vtable for type reflection mapping
    pub const fn get_vtable(&self) -> DynMetadata<T> {
        self.vtable
    }
}

impl TypeId {
    /// Compute the type information of a concrete type.
    /// It can only be called at compile time.
    #[unstable(feature = "type_info", issue = "146922")]
    #[rustc_const_unstable(feature = "type_info", issue = "146922")]
    #[rustc_comptime]
    pub fn info(self) -> Type {
        intrinsics::reflection::type_id_type_of(self)
    }
}

impl Type {
    /// Returns the type information of the generic type parameter.
    ///
    /// Note: Unlike `TypeId`s obtained via `TypeId::of`, the `Type`
    /// struct and its fields contain `TypeId`s that are not necessarily
    /// derived from types that outlive `'static`. This means that using
    /// the `TypeId`s (transitively) obtained from this function will
    /// be able to break invariants that other `TypeId` consuming crates
    /// may have assumed to hold.
    #[unstable(feature = "type_info", issue = "146922")]
    #[rustc_const_unstable(feature = "type_info", issue = "146922")]
    pub const fn of<T: ?Sized>() -> Self {
        const { intrinsics::reflection::type_id::<T>().info() }
    }
}

// FIXME(reflection): get rid of the static lifetime bound on TypeId and remove this function.
/// Returns the [TypeId] of the generic type parameter.
///
/// This is identical to [TypeId::of] but without the static lifetime bound. It will be removed
/// in the future.
#[must_use]
#[unstable(feature = "type_info", issue = "146922")]
#[rustc_const_unstable(feature = "type_info", issue = "146922")]
pub const fn of<T: ?Sized>() -> TypeId {
    const { intrinsics::reflection::type_id::<T>() }
}

/// Compile-time type information.
#[derive(Debug)]
#[non_exhaustive]
#[unstable(feature = "type_info", issue = "146922")]
pub enum TypeKind {
    /// Tuples.
    Tuple,
    /// Arrays.
    Array,
    /// Slices.
    Slice,
    /// Dynamic Traits.
    DynTrait(DynTrait),
    /// Structs.
    Struct,
    /// Enums.
    Enum,
    /// Unions.
    Union,
    /// Primitive boolean type.
    Bool,
    /// Primitive character type.
    Char,
    /// Primitive signed and unsigned integer type.
    Int,
    /// Primitive floating-point type.
    Float,
    /// String slice type.
    Str(Str),
    /// References.
    Reference,
    /// Pointers.
    Pointer,
    /// Function pointers.
    FnPtr,
    /// Function items.
    FnDef(FnDef),
    /// FIXME(#146922): add all the common types
    /// non exhaustive list:
    /// - Never
    Other,
}

/// Compile-time information about a function item.
#[derive(Debug)]
#[non_exhaustive]
#[unstable(feature = "type_info", issue = "146922")]
pub struct FnDef {
    /// The function's source name.
    pub name: &'static str,
    /// The source file and byte range of the function declaration.
    pub span: SourceSpan,
    /// The function's parameter types.
    pub inputs: &'static [TypeId],
    /// Names of simple identifier parameters. Destructuring patterns and unavailable metadata are
    /// represented as `None`.
    pub param_names: &'static [Option<&'static str>],
    /// The function signature's output type. For an async function, this is its future type.
    pub output: TypeId,
    /// The declared output when available: `Future::Output` for async functions, otherwise the signature output.
    pub declared_output: Option<TypeId>,
    /// Whether the function is async.
    pub is_async: bool,
}

/// A source location exposed by compile-time reflection.
#[derive(Debug, PartialEq, Eq)]
#[non_exhaustive]
#[unstable(feature = "type_info", issue = "146922")]
pub struct SourceSpan {
    /// The source file name as known to the compiler.
    pub file: &'static str,
    /// The zero-based byte offset from the beginning of `file`.
    pub start: u32,
    /// The zero-based byte offset just past the end of the span.
    pub end: u32,
}

/// A collection of typed item requests produced by compile-time code.
///
/// This descriptor format is compile-time input to compiler item generation;
/// it is not a runtime registration mechanism.
#[derive(Clone, Copy, Debug)]
#[non_exhaustive]
#[unstable(feature = "type_info", issue = "146922")]
#[rustc_diagnostic_item = "comptime_item_set"]
pub struct ItemSet {
    /// Items in deterministic caller-provided order.
    pub items: &'static [Item],
}

impl ItemSet {
    /// Creates an item set from a fixed descriptor slice.
    #[unstable(feature = "type_info", issue = "146922")]
    #[rustc_comptime]
    pub fn new(items: &'static [Item]) -> Self {
        Self { items }
    }
}

/// A typed function or static request.
#[derive(Clone, Copy, Debug)]
#[non_exhaustive]
#[unstable(feature = "type_info", issue = "146922")]
pub struct Item {
    /// Module path relative to the compile-time generator's containing module.
    pub module: &'static [&'static str],
    /// Requested Rust visibility.
    pub visibility: ItemVisibility,
    /// Requested item kind and contents.
    pub kind: ItemKind,
}

impl Item {
    /// Creates an item request in the specified module and with the requested visibility.
    #[unstable(feature = "type_info", issue = "146922")]
    #[rustc_comptime]
    pub fn new(
        module: &'static [&'static str],
        visibility: ItemVisibility,
        kind: ItemKind,
    ) -> Self {
        Self { module, visibility, kind }
    }
}

/// Visibility requested for a generated item.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[non_exhaustive]
#[unstable(feature = "type_info", issue = "146922")]
pub enum ItemVisibility {
    /// Visible only in the containing module.
    Private,
    /// Visible throughout the crate.
    Crate,
    /// Public visibility, subject to ordinary Rust privacy rules.
    Public,
}

/// Kind of generated item.
#[derive(Clone, Copy, Debug)]
#[non_exhaustive]
#[unstable(feature = "type_info", issue = "146922")]
pub enum ItemKind {
    /// A generated free function.
    Function(Function),
    /// A generated static item.
    Static(Static),
}

/// A function request with a typed signature and structured body.
#[derive(Clone, Copy, Debug)]
#[non_exhaustive]
#[unstable(feature = "type_info", issue = "146922")]
pub struct Function {
    /// Caller-selected Rust identifier.
    pub name: &'static str,
    /// Function parameters in declaration order.
    pub parameters: &'static [Parameter],
    /// Declared output type.
    pub output: TypeId,
    /// Whether the generated function is async.
    pub is_async: bool,
    /// Expression returned by the generated function.
    pub body: Expression,
}

impl Function {
    /// Creates a function request with a typed signature and expression body.
    #[unstable(feature = "type_info", issue = "146922")]
    #[rustc_comptime]
    pub fn new(
        name: &'static str,
        parameters: &'static [Parameter],
        output: TypeId,
        is_async: bool,
        body: Expression,
    ) -> Self {
        Self { name, parameters, output, is_async, body }
    }
}

/// A generated function parameter.
#[derive(Clone, Copy, Debug)]
#[non_exhaustive]
#[unstable(feature = "type_info", issue = "146922")]
pub struct Parameter {
    /// Caller-selected parameter identifier.
    pub name: &'static str,
    /// Semantic parameter type.
    pub ty: TypeId,
}

impl Parameter {
    /// Creates a named typed function parameter.
    #[unstable(feature = "type_info", issue = "146922")]
    #[rustc_comptime]
    pub fn new(name: &'static str, ty: TypeId) -> Self {
        Self { name, ty }
    }
}

/// A static item request.
#[derive(Clone, Copy, Debug)]
#[non_exhaustive]
#[unstable(feature = "type_info", issue = "146922")]
pub struct Static {
    /// Caller-selected Rust identifier.
    pub name: &'static str,
    /// Semantic static type.
    pub ty: TypeId,
    /// Static initializer expression.
    pub initializer: Expression,
    /// Whether the generated static is mutable.
    pub is_mutable: bool,
}

impl Static {
    /// Creates a static request with a typed initializer.
    #[unstable(feature = "type_info", issue = "146922")]
    #[rustc_comptime]
    pub fn new(
        name: &'static str,
        ty: TypeId,
        initializer: Expression,
        is_mutable: bool,
    ) -> Self {
        Self { name, ty, initializer, is_mutable }
    }
}

/// A structured expression in a generated function body or static initializer.
#[derive(Clone, Copy, Debug)]
#[non_exhaustive]
#[unstable(feature = "type_info", issue = "146922")]
pub enum Expression {
    /// Reference a generated function parameter by zero-based index.
    Parameter(u16),
    /// A typed integer literal, represented by its target type and bits.
    Integer {
        /// Semantic integer type.
        ty: TypeId,
        /// Raw target-independent integer bits, interpreted according to `ty`.
        bits: u128,
    },
    /// A boolean literal.
    Bool(bool),
    /// Call an existing function item identified by its semantic function-item type.
    Call {
        /// Function-item `TypeId` of the callee.
        function: TypeId,
        /// Arguments in call order.
        arguments: &'static [Expression],
    },
    /// Construct a tuple or struct with fields in declaration order.
    Construct {
        /// Tuple or struct type to construct.
        ty: TypeId,
        /// Field values in declaration order.
        fields: &'static [Expression],
    },
    /// Read a field by zero-based declaration index.
    Field {
        /// The expression producing the base value.
        base: &'static Expression,
        /// Semantic type of the base expression (a tuple or struct).
        ty: TypeId,
        /// Zero-based field index.
        index: u16,
    },
    /// A binary operation on two expressions.
    Binary {
        /// Operation applied to the operands.
        op: BinaryOp,
        /// Left-hand operand.
        lhs: &'static Expression,
        /// Right-hand operand.
        rhs: &'static Expression,
    },
    /// Await an expression in an async generated function.
    Await(&'static Expression),
}

/// Binary operations supported by generated expressions.
#[derive(Clone, Copy, Debug)]
#[non_exhaustive]
#[unstable(feature = "type_info", issue = "146922")]
pub enum BinaryOp {
    /// Integer addition.
    Add,
    /// Integer subtraction.
    Subtract,
    /// Integer multiplication.
    Multiply,
    /// Integer division.
    Divide,
    /// Integer remainder.
    Remainder,
    /// Equality comparison.
    Equal,
    /// Inequality comparison.
    NotEqual,
    /// Boolean conjunction.
    And,
    /// Boolean disjunction.
    Or,
}

/// Compile-time type information about dynamic traits.
/// FIXME(#146922): Add super traits and generics
#[derive(Debug)]
#[non_exhaustive]
#[unstable(feature = "type_info", issue = "146922")]
pub struct DynTrait {
    /// The predicates of  a dynamic trait.
    pub predicates: &'static [DynTraitPredicate],
}

/// Compile-time type information about a dynamic trait predicate.
#[derive(Debug)]
#[non_exhaustive]
#[unstable(feature = "type_info", issue = "146922")]
pub struct DynTraitPredicate {
    /// The type of the trait as a dynamic trait type.
    pub trait_ty: Trait,
}

/// Compile-time type information about a trait.
#[derive(Debug)]
#[non_exhaustive]
#[unstable(feature = "type_info", issue = "146922")]
pub struct Trait {
    /// The TypeId of the trait as a dynamic type
    pub ty: TypeId,
    /// Whether the trait is an auto trait
    pub is_auto: bool,
}

/// Compile-time type information about instantiated generics of structs, enum and union variants.
#[derive(Debug)]
#[non_exhaustive]
#[unstable(feature = "type_info", issue = "146922")]
#[lang = "type_info_generic"]
pub enum Generic {
    /// Lifetimes.
    Lifetime(Lifetime),
    /// Types.
    Type(GenericType),
    /// Const parameters.
    Const(Const),
}

/// Compile-time type information about generic lifetimes.
#[derive(Debug)]
#[non_exhaustive]
#[unstable(feature = "type_info", issue = "146922")]
pub struct Lifetime {
    // No additional information to provide for now.
}

/// Compile-time type information about instantiated generic types.
#[derive(Debug)]
#[non_exhaustive]
#[unstable(feature = "type_info", issue = "146922")]
pub struct GenericType {
    /// The type itself.
    pub ty: TypeId,
}

/// Compile-time type information about generic const parameters.
#[derive(Debug)]
#[non_exhaustive]
#[unstable(feature = "type_info", issue = "146922")]
pub struct Const {
    /// The const's type.
    pub ty: TypeId,
}

/// Compile-time type information about string slice types.
#[derive(Debug)]
#[non_exhaustive]
#[unstable(feature = "type_info", issue = "146922")]
pub struct Str {
    // No additional information to provide for now.
}

#[derive(Debug)]
#[lang = "FnPtr"]
#[unstable(feature = "type_info", issue = "146922")]
/// Function pointer, e.g. fn(u8),
pub struct FnPtr {
    is_unsafe: bool,
    abi: Abi,
    inputs: &'static [TypeId],
    output: TypeId,
    variadic: bool,
    // FIXME(splat): should these fields be private, or merged into an Option<u8/u16>?
    /// Is any function argument splatted?
    is_splatted: bool,

    splatted_index: u8,
}

impl FnPtr {
    /// Returns the splatted function argument index, or `None` if no argument is splatted.
    ///
    /// e.g. in `fn overload(a: u8, #[rustc_splat] b: (f32, usize))` the index is 1,
    /// and it can be called as `overload(a, 1.0, 2)`.
    pub const fn splatted(&self) -> Option<u8> {
        if self.is_splatted { Some(self.splatted_index) } else { None }
    }
    /// Whether this function is variadic, e.g. extern "C" fn add(n: usize, mut args: ...);
    pub const fn is_variadic(&self) -> bool {
        self.variadic
    }
    /// whether this refers to an unsafe function.
    pub const fn is_unsafe(&self) -> bool {
        self.is_unsafe
    }
    /// Returns the application binary interface. For example extern "C".
    pub const fn abi(&self) -> Abi {
        self.abi
    }
    /// The types of the functions parameters
    pub const fn inputs(&self) -> &'static [TypeId] {
        self.inputs
    }
    /// List of the types returned by the function. For a function with no output
    /// specified this returns `TypeId::of<()>`.
    pub const fn output(&self) -> TypeId {
        self.output
    }
}

#[derive(Debug, Default, Copy, Clone, PartialEq, Eq)]
#[non_exhaustive]
#[unstable(feature = "type_info", issue = "146922")]
/// Abi of [FnPtr]
pub enum Abi {
    /// Named abi, e.g. extern "custom", "stdcall" etc.
    Named(&'static str),

    /// Default
    #[default]
    ExternRust,

    /// C-calling convention
    ExternC,
}

impl TypeId {
    /// Returns `true` if the type represented by this `TypeId` is an signed integer.
    ///
    /// For everything else this returns false.
    ///
    /// # Examples
    ///
    /// ```
    /// #![feature(type_info)]
    /// use std::any::TypeId;
    ///
    /// assert_eq!(const { TypeId::of::<i32>().is_signed() }, true);
    /// assert_eq!(const { TypeId::of::<u8>().is_signed() }, false);
    /// assert_eq!(const { TypeId::of::<bool>().is_signed() }, false);
    /// ```
    #[unstable(feature = "type_info", issue = "146922")]
    #[rustc_const_unstable(feature = "type_info", issue = "146922")]
    #[rustc_comptime]
    pub fn is_signed(self) -> bool {
        intrinsics::reflection::type_id_is_signed(self)
    }

    /// When called on a `TypeId` representing an array or slice this returns the type of each
    /// element otherwise this returns `None`.
    ///
    /// # Examples
    ///
    /// ```
    /// #![feature(type_info)]
    /// use std::any::TypeId;
    ///
    /// assert_eq!(const { TypeId::of::<[u32; 16]>().element_ty() }, Some(TypeId::of::<u32>()));
    /// assert_eq!(const { TypeId::of::<u8>().element_ty() }, None); // not an array or slice
    /// ```
    #[unstable(feature = "type_info", issue = "146922")]
    #[rustc_const_unstable(feature = "type_info", issue = "146922")]
    #[rustc_comptime]
    pub fn element_ty(self) -> Option<TypeId> {
        intrinsics::reflection::type_id_element_ty(self)
    }

    /// When called on a `TypeId` representing an array this returns the length of the array in
    /// all other cases this returns zero.
    ///
    /// # Examples
    ///
    /// ```
    /// #![feature(type_info)]
    /// use std::any::TypeId;
    ///
    /// assert_eq!(const { TypeId::of::<[u32; 16]>().array_len() }, 16);
    /// assert_eq!(const { TypeId::of::<u8>().array_len() }, 0); // not an array
    /// ```
    #[unstable(feature = "type_info", issue = "146922")]
    #[rustc_const_unstable(feature = "type_info", issue = "146922")]
    #[rustc_comptime]
    pub fn array_len(self) -> usize {
        intrinsics::reflection::type_id_array_len(self)
    }

    /// Returns the size of the type represented by this `TypeId`. `None` if it is unsized.
    ///
    /// # Examples
    ///
    /// ```
    /// #![feature(type_info)]
    /// use std::any::TypeId;
    ///
    /// assert_eq!(const { TypeId::of::<u32>().size() }, Some(4));
    /// assert_eq!(const { TypeId::of::<[u8; 16]>().size() }, Some(16));
    /// ```
    #[unstable(feature = "type_info", issue = "146922")]
    #[rustc_const_unstable(feature = "type_info", issue = "146922")]
    #[rustc_comptime]
    pub fn size(self) -> Option<usize> {
        intrinsics::reflection::type_id_size_of(self)
    }

    /// Returns the number of variants of the type represented by this `TypeId`.
    ///
    /// For enums, this is the number of variants. For structs and unions, this is always 1.
    ///
    /// ```
    /// #![feature(type_info)]
    /// use std::any::TypeId;
    ///
    /// assert_eq!(const { TypeId::of::<Option<()>>().variants() }, 2);
    ///
    /// struct Unit;
    /// struct Point {
    ///     x: u32,
    ///     y: u32,
    /// }
    /// assert_eq!(const { TypeId::of::<Unit>().variants() }, 1);
    /// assert_eq!(const { TypeId::of::<Point>().variants() }, 1);
    /// assert_eq!(const { TypeId::of::<(f32, f32)>().variants() }, 1);
    /// ```
    #[unstable(feature = "type_info", issue = "146922")]
    #[rustc_const_unstable(feature = "type_info", issue = "146922")]
    #[rustc_comptime]
    pub fn variants(self) -> usize {
        intrinsics::reflection::type_id_variants(self)
    }

    // FIXME(reflection): make the errors nicer. This is a wider problem,
    // TypeId::fields has nice errors in the docs but those are not the ones shown
    // by rustc.
    /// Returns the variant representing type at the given index of the type represented by this `TypeId`. Use it to
    /// get the name of an enum variant or check whether it is non_exhaustive.
    ///
    /// ```
    /// #![feature(type_info)]
    /// use std::any::TypeId;
    ///
    /// enum Enum {
    ///     Unit,
    ///     Tuple(u32, u64),
    ///     #[non_exhaustive]
    ///     Struct { x: u32, y: u32, z: String },
    /// }
    /// assert_eq!(const { TypeId::of::<Enum>().variant(1).name() }, "Tuple");
    /// assert_eq!(const { TypeId::of::<Enum>().variant(2).name() }, "Struct");
    ///
    /// assert_eq!(const { TypeId::of::<Enum>().variant(1).non_exhaustive() }, false);
    /// assert_eq!(const { TypeId::of::<Enum>().variant(2).non_exhaustive() }, true);
    /// ```
    ///
    /// The variant index refer to the source order index of a variant in a type.
    ///
    /// Variant indexes are always `0..variant_count`, regardless of any custom discriminants that may have been defined.
    ///
    /// ```
    /// enum Enum {
    ///     Foo,  // variant index == 0
    ///     Bar,  // variant index == 1
    /// }
    /// ```
    ///
    /// Calling variant on the TypeId for a struct will be treated as a compile-time error. The same
    /// is true for out-of-bounds indexing on an enum.
    ///
    /// ```compile_fail,E0080
    /// # #![feature(type_info)]
    /// # use std::any::TypeId;
    /// #
    /// # struct Point {
    /// #     x: u32,
    /// #     y: u32,
    /// # }
    /// # enum Enum {
    /// #     Unit,
    /// #     Tuple(u32, u64),
    /// #     Struct { x: u32, y: u32, z: String },
    /// # }
    /// const {
    ///     _ = TypeId::of::<Point>().variant(0); // error: cannot get the variant of a struct
    ///     _ = TypeId::of::<Enum>().variant(10); // error: indexing out of bounds: the len is 3 but the index is 10
    /// }
    /// ```
    #[unstable(feature = "type_info", issue = "146922")]
    #[rustc_const_unstable(feature = "type_info", issue = "146922")]
    #[rustc_comptime]
    pub fn variant(self, variant_index: usize) -> VariantId {
        intrinsics::reflection::type_id_fields(self, variant_index);
        VariantId { base: self, variant: variant_index }
    }

    /// Returns the number of fields at the given `variant_index` of the type represented by this `TypeId`.
    ///
    /// ```
    /// #![feature(type_info)]
    /// use std::any::TypeId;
    ///
    /// assert_eq!(const { TypeId::of::<u32>().fields(0) }, 0);
    ///
    /// struct Point {
    ///     x: u32,
    ///     y: u32,
    /// }
    /// assert_eq!(const { TypeId::of::<Point>().fields(0) }, 2);
    ///
    /// enum Enum {
    ///     Unit,
    ///     Tuple(u32, u64),
    ///     Struct { x: u32, y: u32, z: String },
    /// }
    /// assert_eq!(const { TypeId::of::<Enum>().fields(0) }, 0);
    /// assert_eq!(const { TypeId::of::<Enum>().fields(1) }, 2);
    /// assert_eq!(const { TypeId::of::<Enum>().fields(2) }, 3);
    /// ```
    ///
    /// The variant index refers to the source order index of a variant in a type.
    ///
    /// For enums, these are always `0..variant_count`, regardless of any custom discriminants that may have been defined.
    /// `struct`s, `tuples`, and `unions`s are considered to have a single variant with variant index zero.
    ///
    /// ```
    /// enum Number {
    ///     Seven = 7, // variant index == 0
    ///     Six = 6,   // variant index == 1
    /// }
    /// ```
    ///
    /// Out-of-bounds indexing will be treated as a compile-time error.
    ///
    /// ```compile_fail,E0080
    /// # #![feature(type_info)]
    /// # use std::any::TypeId;
    /// #
    /// # struct Point {
    /// #     x: u32,
    /// #     y: u32,
    /// # }
    /// # enum Enum {
    /// #     Unit,
    /// #     Tuple(u32, u64),
    /// #     Struct { x: u32, y: u32, z: String },
    /// # }
    /// const {
    ///     _ = TypeId::of::<Point>().fields(10); // error: indexing out of bounds: the len is 2 but the index is 10
    ///     _ = TypeId::of::<Enum>().fields(10); // error: indexing out of bounds: the len is 3 but the index is 10
    /// }
    /// ```
    #[unstable(feature = "type_info", issue = "146922")]
    #[rustc_const_unstable(feature = "type_info", issue = "146922")]
    #[rustc_comptime]
    // FIXME(type_info): Add enum variant pattern types and use them to represent individual variants
    // Then add a `variant` method to get a wrapper around such a pattern type (similar to the FRT
    // type we have) and add methods on that. It's the only way to really sensibly represent
    // things like `non_exhaustive` which can be applied to variants as well.
    pub fn fields(self, variant_index: usize) -> usize {
        intrinsics::reflection::type_id_fields(self, variant_index)
    }

    /// Returns the field representing type at the given index of the type represented by this `TypeId`.
    ///
    /// ```
    /// #![feature(type_info)]
    /// use std::any::TypeId;
    ///
    /// struct Point {
    ///     x: u32,
    ///     y: u32,
    /// }
    /// assert_eq!(const { TypeId::of::<Point>().field(0, 0).type_id() }, TypeId::of::<u32>());
    /// assert_eq!(const { TypeId::of::<Point>().field(0, 1).type_id() }, TypeId::of::<u32>());
    ///
    /// enum Enum {
    ///     Unit,
    ///     Tuple(u32, u64),
    ///     Struct { x: u32, y: u32, z: String },
    /// }
    /// assert_eq!(const { TypeId::of::<Enum>().field(1, 0).type_id() }, TypeId::of::<u32>());
    /// assert_eq!(const { TypeId::of::<Enum>().field(2, 2).type_id() }, TypeId::of::<String>());
    /// ```
    ///
    /// The variant index and field index refer to the source order index of a variant in a type and
    /// the source order index of a field in a variant, respectively.
    ///
    /// For enums, variant indexes are always `0..variant_count`, regardless of any custom discriminants that may have been defined.
    /// `struct`s, `tuples`, and `unions`s are considered to have a single variant with variant index zero.
    ///
    /// As for field indexes, they may not be the same as the layout order for `repr(Rust)` types, but they are for `repr(C)` types.
    ///
    /// ```
    /// enum Enum {
    ///     Foo,  // variant index == 0
    ///     Bar { // variant index == 1
    ///         a: (), // field index == 0 in `Bar`
    ///         b: (), // field index == 1 in `Bar`
    ///     }
    /// }
    /// ```
    ///
    /// Out-of-bounds indexing will be treated as a compile-time error.
    ///
    /// ```compile_fail,E0080
    /// # #![feature(type_info)]
    /// # use std::any::TypeId;
    /// #
    /// # struct Point {
    /// #     x: u32,
    /// #     y: u32,
    /// # }
    /// # enum Enum {
    /// #     Unit,
    /// #     Tuple(u32, u64),
    /// #     Struct { x: u32, y: u32, z: String },
    /// # }
    /// const {
    ///     _ = TypeId::of::<Point>().field(0, 10); // error: indexing out of bounds: the len is 2 but the index is 10
    ///     _ = TypeId::of::<Enum>().field(2, 10); // error: indexing out of bounds: the len is 3 but the index is 10
    /// }
    /// ```
    #[unstable(feature = "type_info", issue = "146922")]
    #[rustc_const_unstable(feature = "type_info", issue = "146922")]
    #[rustc_comptime]
    pub fn field(self, variant_index: usize, field_index: usize) -> FieldId {
        FieldId {
            frt_type_id: intrinsics::reflection::type_id_field_representing_type(
                self,
                variant_index,
                field_index,
            ),
        }
    }

    /// Returns whether a type is marked with `#[non_exhaustive]`.
    /// Returns `false` for everything but adts.
    #[unstable(feature = "type_info", issue = "146922")]
    #[rustc_const_unstable(feature = "type_info", issue = "146922")]
    #[rustc_comptime]
    pub fn non_exhaustive(self) -> bool {
        intrinsics::reflection::type_id_non_exhaustive(self)
    }

    /// Returns whether this ADT carries the tool attribute `attr`, given as a `::`-separated path.
    /// Only tool attributes (see `#![register_tool]`) are visible. Returns `false` for non-ADTs.
    ///
    /// ```
    /// #![feature(type_info, register_tool)]
    /// #![register_tool(app)]
    /// use std::any::TypeId;
    ///
    /// #[app::singleton]
    /// struct Service;
    /// struct Plain;
    /// assert!(const { TypeId::of::<Service>().has_attr("app::singleton") });
    /// assert!(!const { TypeId::of::<Plain>().has_attr("app::singleton") });
    /// ```
    #[unstable(feature = "type_info", issue = "146922")]
    #[rustc_const_unstable(feature = "type_info", issue = "146922")]
    #[rustc_comptime]
    pub fn has_attr(self, attr: &'static str) -> bool {
        intrinsics::reflection::type_id_has_attr(self, attr)
    }

    /// Returns the literal of `key` in `#[attr(key = lit)]` on this ADT, or of `#[attr = lit]` if
    /// `key` is empty. String literals are unescaped, other literals are returned as written.
    ///
    /// ```
    /// #![feature(type_info, register_tool)]
    /// #![register_tool(app)]
    /// use std::any::TypeId;
    ///
    /// #[app::config(prefix = "database")]
    /// struct Config;
    /// assert_eq!(
    ///     const { TypeId::of::<Config>().attr_value("app::config", "prefix") },
    ///     Some("database"),
    /// );
    /// ```
    #[unstable(feature = "type_info", issue = "146922")]
    #[rustc_const_unstable(feature = "type_info", issue = "146922")]
    #[rustc_comptime]
    pub fn attr_value(self, attr: &'static str, key: &'static str) -> Option<&'static str> {
        intrinsics::reflection::type_id_attr_value(self, attr, key)
    }

    /// Returns a list of generic parameters of the type.
    /// Returns an empty slice for everything that doesn't have generics.
    #[unstable(feature = "type_info", issue = "146922")]
    #[rustc_const_unstable(feature = "type_info", issue = "146922")]
    #[rustc_comptime]
    pub fn generics(self) -> &'static [Generic] {
        intrinsics::reflection::type_id_generics(self)
    }

    /// Given a `TypeId` that represents a pointer this returns the `TypeId`
    /// which that pointer points to. When called on anything else this returns
    /// None.
    ///
    /// ```
    /// #![feature(type_info)]
    /// use std::any::TypeId;
    ///
    /// assert_eq!(
    ///     const { TypeId::of::<&i32>().points_to() },
    ///     const { Some(TypeId::of::<i32>()) },
    /// );
    ///
    /// assert_eq!(
    ///     const { TypeId::of::<*const i32>().points_to() },
    ///     const { Some(TypeId::of::<i32>()) },
    /// );
    /// ```
    #[unstable(feature = "type_info", issue = "146922")]
    #[rustc_const_unstable(feature = "type_info", issue = "146922")]
    #[rustc_comptime]
    pub fn points_to(self) -> Option<TypeId> {
        intrinsics::reflection::type_id_points_to(self)
    }

    /// Given a `TypeId` that represents a pointer returns whether that pointer is mutable.
    /// When called on anything else this returns `false`.
    ///
    /// ```
    /// #![feature(type_info)]
    /// use std::any::TypeId;
    ///
    /// assert!(const { TypeId::of::<&mut i32>().points_mutably() });
    /// assert!(const { !TypeId::of::<&i32>().points_mutably() });
    ///
    /// assert!(!const { TypeId::of::<*const i32>().points_mutably() });
    /// assert!(const { TypeId::of::<*mut i32>().points_mutably() });
    /// ```
    #[unstable(feature = "type_info", issue = "146922")]
    #[rustc_const_unstable(feature = "type_info", issue = "146922")]
    #[rustc_comptime]
    pub fn points_mutably(self) -> bool {
        intrinsics::reflection::type_id_points_mutably(self)
    }

    /// Given a `TypeId` that represents a function pointer returns an
    /// [`FnPtr`]. When called on something else this returns `None`.
    /// ```
    /// #![feature(type_info)]
    /// use std::any::TypeId;
    /// use std::mem::type_info::{Abi, FnPtr};
    ///
    /// const F: FnPtr = TypeId::of::<fn(u8, u16) -> usize>()
    ///     .function_ptr()
    ///     .expect("TypeId of a function ptr");
    ///
    /// assert!(F.inputs() == [TypeId::of::<u8>(), TypeId::of::<u16>()]);
    /// assert!(F.output() == TypeId::of::<usize>());
    /// assert!(F.abi() == Abi::default());
    /// ```
    /// ```
    /// #![feature(type_info)]
    /// # use std::any::TypeId;
    /// # use std::mem::type_info::{Abi, FnPtr};
    /// #
    /// const F: FnPtr = TypeId::of::<unsafe fn()>()
    ///     .function_ptr()
    ///     .expect("TypeId of a function ptr");
    ///
    /// assert!(F.inputs() == []);
    /// assert!(F.output() == TypeId::of::<()>());
    /// assert!(F.abi() == Abi::default());
    /// ```
    #[unstable(feature = "type_info", issue = "146922")]
    #[rustc_const_unstable(feature = "type_info", issue = "146922")]
    #[rustc_comptime]
    pub fn function_ptr(self) -> Option<FnPtr> {
        intrinsics::reflection::type_id_function_ptr(self)
    }
}

/// Variant representing type ID. Representing a variant of an enum.
#[derive(Copy, PartialOrd, Ord, Hash)]
#[derive_const(Clone, PartialEq, Eq)]
#[unstable(feature = "type_info", issue = "146922")]
pub struct VariantId {
    base: TypeId,
    variant: usize,
}

#[unstable(feature = "type_info", issue = "146922")]
impl fmt::Debug for VariantId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "Variant({:#034x}-{})", self.base.as_u128(), self.variant)
    }
}

impl VariantId {
    /// Returns the name of the variant.
    ///
    /// ```
    /// #![feature(type_info)]
    /// use std::any::TypeId;
    ///
    /// enum Enum {
    ///     Unit,
    ///     Tuple(bool),
    ///     Struct { a: bool },
    /// }
    /// assert_eq!(
    ///     const { TypeId::of::<Enum>().variant(1).name() },
    ///     "Tuple",
    /// );
    /// ```
    #[unstable(feature = "type_info", issue = "146922")]
    #[rustc_const_unstable(feature = "type_info", issue = "146922")]
    #[rustc_comptime]
    pub fn name(self) -> &'static str {
        intrinsics::reflection::type_id_variant_name(self.base, self.variant)
    }

    /// Returns whether this variant is marked with `#[non_exhaustive]`.
    #[unstable(feature = "type_info", issue = "146922")]
    #[rustc_const_unstable(feature = "type_info", issue = "146922")]
    #[rustc_comptime]
    pub fn non_exhaustive(self) -> bool {
        intrinsics::reflection::type_id_variant_non_exhaustive(self.base, self.variant)
    }
}

/// Field representing type ID. Representing a field of a struct, tuple or enum variant.
#[derive(Copy, PartialOrd, Ord, Hash)]
#[derive_const(Clone, PartialEq, Eq)]
#[unstable(feature = "type_info", issue = "146922")]
pub struct FieldId {
    frt_type_id: TypeId,
}

#[unstable(feature = "type_info", issue = "146922")]
impl fmt::Debug for FieldId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "FieldId({:#034x})", self.frt_type_id.as_u128())
    }
}

impl FieldId {
    /// Returns the `TypeId` of the actual field type.
    ///
    /// ```
    /// #![feature(type_info)]
    /// use std::any::TypeId;
    ///
    /// struct Point {
    ///     x: u32,
    ///     y: u32,
    /// }
    /// assert_eq!(
    ///     const { TypeId::of::<Point>().field(0, 0).type_id() },
    ///     TypeId::of::<u32>()
    /// );
    /// ```
    #[unstable(feature = "type_info", issue = "146922")]
    #[rustc_const_unstable(feature = "type_info", issue = "146922")]
    #[rustc_comptime]
    pub fn type_id(self) -> TypeId {
        intrinsics::field_representing_type_actual_type_id(self.frt_type_id)
    }

    /// Returns the name of the field.
    ///
    /// ```
    /// #![feature(type_info)]
    /// use std::any::TypeId;
    ///
    /// struct Point {
    ///     x: u32,
    ///     y: u32,
    /// }
    /// assert_eq!(
    ///     const { TypeId::of::<Point>().field(0, 0).name() },
    ///     "x",
    /// );
    /// ```
    #[unstable(feature = "type_info", issue = "146922")]
    #[rustc_const_unstable(feature = "type_info", issue = "146922")]
    #[rustc_comptime]
    pub fn name(self) -> &'static str {
        intrinsics::field_representing_type_name(self.frt_type_id)
    }
    /// Returns the offset of the field wrt to its containing type.
    ///
    /// ```
    /// #![feature(type_info)]
    /// use std::any::TypeId;
    ///
    /// #[repr(C)]
    /// struct Point {
    ///     x: u32,
    ///     y: u32,
    /// }
    /// assert_eq!(
    ///     const { TypeId::of::<Point>().field(0, 1).offset() },
    ///     4,
    /// );
    /// ```
    #[unstable(feature = "type_info", issue = "146922")]
    #[rustc_const_unstable(feature = "type_info", issue = "146922")]
    #[rustc_comptime]
    pub fn offset(self) -> usize {
        intrinsics::field_representing_type_offset(self.frt_type_id)
    }

    /// Returns whether the field carries the tool attribute `attr`.
    #[unstable(feature = "type_info", issue = "146922")]
    #[rustc_const_unstable(feature = "type_info", issue = "146922")]
    #[rustc_comptime]
    pub fn has_attr(self, attr: &'static str) -> bool {
        intrinsics::reflection::field_representing_type_has_attr(self.frt_type_id, attr)
    }

    /// Returns the literal of `key` in `#[attr(key = lit)]` on the field, or of `#[attr = lit]` if
    /// `key` is empty.
    #[unstable(feature = "type_info", issue = "146922")]
    #[rustc_const_unstable(feature = "type_info", issue = "146922")]
    #[rustc_comptime]
    pub fn attr_value(self, attr: &'static str, key: &'static str) -> Option<&'static str> {
        intrinsics::reflection::field_representing_type_attr_value(self.frt_type_id, attr, key)
    }
}

/// Returns the non-generic structs, enums and unions of the crate being compiled that carry the
/// tool attribute `attr`, in definition order.
///
/// ```
/// #![feature(type_info, register_tool)]
/// #![register_tool(app)]
/// use std::any::TypeId;
/// use std::mem::type_info::crate_types_with_attr;
///
/// #[app::singleton]
/// struct A;
/// struct B;
/// #[app::singleton]
/// enum C {}
/// const SINGLETONS: &[TypeId] = crate_types_with_attr("app::singleton");
/// assert_eq!(SINGLETONS, &[TypeId::of::<A>(), TypeId::of::<C>()]);
/// ```
#[unstable(feature = "type_info", issue = "146922")]
#[rustc_const_unstable(feature = "type_info", issue = "146922")]
#[rustc_comptime]
pub fn crate_types_with_attr(attr: &'static str) -> &'static [TypeId] {
    intrinsics::reflection::crate_types_with_attr(attr)
}

/// Returns the non-generic functions of the crate being compiled that carry the tool attribute
/// `attr`, in definition order.
#[unstable(feature = "type_info", issue = "146922")]
#[rustc_const_unstable(feature = "type_info", issue = "146922")]
#[rustc_comptime]
pub fn crate_functions_with_attr(attr: &'static str) -> &'static [TypeId] {
    intrinsics::reflection::crate_functions_with_attr(attr)
}

/// Returns the non-generic functions of the current crate and its loaded dependencies that carry
/// the tool attribute `attr`. Local functions are listed first, followed by external crates in
/// compiler metadata order; definitions within each crate follow metadata order.
#[unstable(feature = "type_info", issue = "146922")]
#[rustc_const_unstable(feature = "type_info", issue = "146922")]
#[rustc_comptime]
pub fn all_functions_with_attr(attr: &'static str) -> &'static [TypeId] {
    intrinsics::reflection::all_functions_with_attr(attr)
}

//! Reads `core::mem::type_info::ItemSet` constants into plain, session-independent data.
//!
//! The data produced here is serialized by the driver and replayed into a fresh compilation
//! session, so it must not contain `Symbol`s, `DefId`s, `Ty`s or spans.
#![allow(rustc::untranslatable_diagnostic, rustc::diagnostic_outside_of_impl)]

use rustc_abi::{FieldIdx, VariantIdx};
use rustc_hir::def::DefKind;
use rustc_hir::def_id::{DefId, LOCAL_CRATE, LocalDefId};
use rustc_hir::definitions::DefPathData;
use rustc_macros::{Decodable_NoContext, Encodable_NoContext};
use rustc_middle::ty::{self, Ty, TyCtxt, TypingEnv};
use rustc_span::{ErrorGuaranteed, Span, sym};

use super::CanAccessMutGlobal;
use super::eval_queries::mk_eval_cx_to_read_const_val;
use crate::interpret::{CtfeProvenance, InterpCx, Projectable};

pub use super::machine::CompileTimeMachine;

#[derive(Debug, Clone, Encodable_NoContext, Decodable_NoContext)]
pub enum GenRoot {
    /// `crate::`
    Crate,
    /// `::name::`
    Extern(String),
}

#[derive(Debug, Clone, Encodable_NoContext, Decodable_NoContext)]
pub struct GenPath {
    pub root: GenRoot,
    pub segments: Vec<String>,
}

#[derive(Debug, Clone, Encodable_NoContext, Decodable_NoContext)]
pub enum GenTy {
    Prim(String),
    Path(GenPath, Vec<GenTy>),
    Ref { mutable: bool, inner: Box<GenTy> },
    Tuple(Vec<GenTy>),
    Slice(Box<GenTy>),
    Array(Box<GenTy>, u64),
    Never,
}

#[derive(Debug, Clone, Copy, Encodable_NoContext, Decodable_NoContext)]
pub enum GenBinOp {
    Add,
    Sub,
    Mul,
    Div,
    Rem,
    Eq,
    Ne,
    And,
    Or,
}

#[derive(Debug, Clone, Encodable_NoContext, Decodable_NoContext)]
pub enum GenExpr {
    Local(String),
    Int { ty: String, negative: bool, magnitude: u128 },
    Bool(bool),
    Call(GenPath, Vec<GenExpr>),
    Tuple(Vec<GenExpr>),
    Struct(GenPath, Vec<(String, GenExpr)>),
    Field(Box<GenExpr>, String),
    Binary(GenBinOp, Box<GenExpr>, Box<GenExpr>),
    Await(Box<GenExpr>),
}

#[derive(Debug, Clone, Copy, Encodable_NoContext, Decodable_NoContext)]
pub enum GenVisibility {
    Private,
    Crate,
    Public,
}

#[derive(Debug, Clone, Encodable_NoContext, Decodable_NoContext)]
pub enum GenKind {
    Fn { name: String, params: Vec<(String, GenTy)>, output: GenTy, is_async: bool, body: GenExpr },
    Static { name: String, ty: GenTy, init: GenExpr, mutable: bool },
}

#[derive(Debug, Clone, Encodable_NoContext, Decodable_NoContext)]
pub struct GenItem {
    /// Absolute module path from the crate root.
    pub module: Vec<String>,
    pub visibility: GenVisibility,
    pub kind: GenKind,
}

type R<T> = Result<T, String>;

/// Evaluates every `const` of type `ItemSet` in the local crate.
pub fn evaluate_item_sets(tcx: TyCtxt<'_>) -> Result<Vec<GenItem>, ErrorGuaranteed> {
    let mut out = Vec::new();
    let mut failed = None;
    for def_id in tcx.hir_body_owners() {
        if !matches!(tcx.def_kind(def_id), DefKind::Const { .. }) {
            continue;
        }
        let ty = tcx.type_of(def_id).skip_binder();
        let ty::Adt(adt, _) = ty.kind() else { continue };
        if !tcx.is_diagnostic_item(sym::comptime_item_set, adt.did()) {
            continue;
        }
        let span = tcx.def_span(def_id);
        let value = match tcx.const_eval_poly(def_id.to_def_id()) {
            Ok(value) => value,
            Err(_) => {
                // Already reported by const evaluation.
                failed = Some(tcx.dcx().delayed_bug("comptime item generator failed"));
                continue;
            }
        };
        match read_item_set(tcx, def_id, span, value, ty) {
            Ok(items) => out.extend(items),
            Err(msg) => {
                failed = Some(tcx.dcx().span_err(span, format!("invalid generated items: {msg}")));
            }
        }
    }
    match failed {
        Some(guar) => Err(guar),
        None => Ok(out),
    }
}

fn read_item_set<'tcx>(
    tcx: TyCtxt<'tcx>,
    def_id: LocalDefId,
    span: Span,
    value: rustc_middle::mir::ConstValue,
    ty: Ty<'tcx>,
) -> R<Vec<GenItem>> {
    let ecx = mk_eval_cx_to_read_const_val(
        tcx,
        span,
        TypingEnv::fully_monomorphized(),
        CanAccessMutGlobal::No,
    );
    let op = ecx.const_val_to_op(value, ty, None).map_err(ierr)?;
    let module = module_path(tcx, def_id);
    let reader = Reader { ecx: &ecx, tcx, module };
    // ItemSet { items: &[Item] }
    let items = reader.field(&op, 0)?;
    reader.slice(&items, |item| reader.item(item))
}

fn ierr(e: rustc_middle::mir::interpret::InterpErrorInfo<'_>) -> String {
    format!("{:?}", e.into_kind())
}

fn module_path(tcx: TyCtxt<'_>, def_id: LocalDefId) -> Vec<String> {
    let module = tcx.parent_module_from_def_id(def_id).to_def_id();
    def_path_segments(tcx, module).unwrap_or_default()
}

fn def_path_segments(tcx: TyCtxt<'_>, def_id: DefId) -> R<Vec<String>> {
    let mut segments = Vec::new();
    for data in &tcx.def_path(def_id).data {
        match data.data {
            DefPathData::TypeNs(name) | DefPathData::ValueNs(name) => {
                segments.push(name.as_str().to_owned())
            }
            _ => return Err(format!("`{}` has no nameable path", tcx.def_path_str(def_id))),
        }
    }
    Ok(segments)
}

fn gen_path(tcx: TyCtxt<'_>, def_id: DefId) -> R<GenPath> {
    let segments = def_path_segments(tcx, def_id)?;
    let root = if def_id.krate == LOCAL_CRATE {
        GenRoot::Crate
    } else {
        let name = tcx.crate_name(def_id.krate);
        // `core` and `alloc` are not necessarily nameable from the user's crate; their items
        // are re-exported at the same paths from `std`.
        let name = match name.as_str() {
            "core" | "alloc" => "std",
            other => other,
        };
        GenRoot::Extern(name.to_owned())
    };
    Ok(GenPath { root, segments })
}

struct Reader<'a, 'tcx> {
    ecx: &'a InterpCx<'tcx, CompileTimeMachine<'tcx>>,
    tcx: TyCtxt<'tcx>,
    module: Vec<String>,
}

impl<'a, 'tcx> Reader<'a, 'tcx> {
    /// Fields are addressed by their declaration index in `core::mem::type_info`.
    fn field<P: Projectable<'tcx, CtfeProvenance>>(&self, op: &P, idx: usize) -> R<P> {
        self.ecx.project_field(op, FieldIdx::from_usize(idx)).map_err(ierr)
    }

    /// Returns the variant name and the downcast operand.
    fn variant<P: Projectable<'tcx, CtfeProvenance>>(&self, op: &P) -> R<(String, P)> {
        let idx: VariantIdx = self.ecx.read_discriminant(op).map_err(ierr)?;
        let ty::Adt(adt, _) = op.layout().ty.kind() else {
            return Err("expected an enum".into());
        };
        let name = adt.variant(idx).name.as_str().to_owned();
        let down = self.ecx.project_downcast(op, idx).map_err(ierr)?;
        Ok((name, down))
    }

    fn slice<P: Projectable<'tcx, CtfeProvenance>, T>(
        &self,
        op: &P,
        mut f: impl FnMut(&crate::interpret::MPlaceTy<'tcx>) -> R<T>,
    ) -> R<Vec<T>> {
        let place = self.ecx.deref_pointer(op).map_err(ierr)?;
        let len = place.len(self.ecx).map_err(ierr)?;
        let mut out = Vec::with_capacity(len as usize);
        for i in 0..len {
            let elem = self.ecx.project_index(&place, i).map_err(ierr)?;
            out.push(f(&elem)?);
        }
        Ok(out)
    }

    fn string<P: Projectable<'tcx, CtfeProvenance>>(&self, op: &P) -> R<String> {
        let place = self.ecx.deref_pointer(op).map_err(ierr)?;
        Ok(self.ecx.read_str(&place).map_err(ierr)?.to_owned())
    }

    fn boolean<P: Projectable<'tcx, CtfeProvenance>>(&self, op: &P) -> R<bool> {
        self.ecx.read_scalar(op).map_err(ierr)?.to_bool().map_err(ierr)
    }

    fn item(&self, op: &crate::interpret::MPlaceTy<'tcx>) -> R<GenItem> {
        // Item { module, visibility, kind }
        let rel: Vec<String> =
            self.slice(&self.field(op, 0)?, |s| self.string(s))?;
        let mut module = self.module.clone();
        module.extend(rel);
        let visibility = match self.variant(&self.field(op, 1)?)?.0.as_str() {
            "Private" => GenVisibility::Private,
            "Crate" => GenVisibility::Crate,
            "Public" => GenVisibility::Public,
            other => return Err(format!("unknown visibility `{other}`")),
        };
        let (kind, payload) = self.variant(&self.field(op, 2)?)?;
        let inner = self.field(&payload, 0)?;
        let kind = match kind.as_str() {
            "Function" => self.function(&inner)?,
            "Static" => self.static_item(&inner)?,
            other => return Err(format!("unknown item kind `{other}`")),
        };
        Ok(GenItem { module, visibility, kind })
    }

    fn function(&self, op: &crate::interpret::MPlaceTy<'tcx>) -> R<GenKind> {
        // Function { name, parameters, output, is_async, body }
        let name = self.string(&self.field(op, 0)?)?;
        let mut names = Vec::new();
        let params = self.slice(&self.field(op, 1)?, |p| {
            // Parameter { name, ty }
            let pname = self.string(&self.field(p, 0)?)?;
            let ty = self.type_id_ty(&self.field(p, 1)?)?;
            names.push(pname.clone());
            Ok((pname, self.gen_ty(ty)?))
        })?;
        let output = self.gen_ty(self.type_id_ty(&self.field(op, 2)?)?)?;
        let is_async = self.boolean(&self.field(op, 3)?)?;
        let body = self.expr(&self.field(op, 4)?, &names)?;
        Ok(GenKind::Fn { name, params, output, is_async, body })
    }

    fn static_item(&self, op: &crate::interpret::MPlaceTy<'tcx>) -> R<GenKind> {
        // Static { name, ty, initializer, is_mutable }
        let name = self.string(&self.field(op, 0)?)?;
        let ty = self.gen_ty(self.type_id_ty(&self.field(op, 1)?)?)?;
        let init = self.expr(&self.field(op, 2)?, &[])?;
        let mutable = self.boolean(&self.field(op, 3)?)?;
        Ok(GenKind::Static { name, ty, init, mutable })
    }

    fn type_id_ty<P: Projectable<'tcx, CtfeProvenance>>(&self, op: &P) -> R<Ty<'tcx>> {
        self.ecx.read_type_id(&op.to_op(self.ecx).map_err(ierr)?).map_err(ierr)
    }

    fn expr<P: Projectable<'tcx, CtfeProvenance>>(&self, op: &P, params: &[String]) -> R<GenExpr> {
        let (name, v) = self.variant(op)?;
        let sub = |idx: usize| -> R<GenExpr> {
            let p = self.ecx.deref_pointer(&self.field(&v, idx)?).map_err(ierr)?;
            self.expr(&p, params)
        };
        Ok(match name.as_str() {
            "Parameter" => {
                let idx = self.ecx.read_scalar(&self.field(&v, 0)?).map_err(ierr)?;
                let idx = idx.to_u16().map_err(ierr)? as usize;
                GenExpr::Local(
                    params.get(idx).ok_or_else(|| format!("no parameter {idx}"))?.clone(),
                )
            }
            "Integer" => {
                let ty = self.type_id_ty(&self.field(&v, 0)?)?;
                let bits =
                    self.ecx.read_scalar(&self.field(&v, 1)?).map_err(ierr)?.to_u128().map_err(ierr)?;
                self.integer(ty, bits)?
            }
            "Bool" => GenExpr::Bool(self.boolean(&self.field(&v, 0)?)?),
            "Call" => {
                let ty = self.type_id_ty(&self.field(&v, 0)?)?;
                let ty::FnDef(did, args) = ty.kind() else {
                    return Err(format!("`{ty}` is not a function item"));
                };
                if !args.is_empty() {
                    return Err(format!("generic function `{ty}` is not supported"));
                }
                let path = gen_path(self.tcx, *did)?;
                let args = self.slice(&self.field(&v, 1)?, |a| self.expr(a, params))?;
                GenExpr::Call(path, args)
            }
            "Construct" => {
                let ty = self.type_id_ty(&self.field(&v, 0)?)?;
                let fields = self.slice(&self.field(&v, 1)?, |f| self.expr(f, params))?;
                match ty.kind() {
                    ty::Tuple(tys) if tys.len() == fields.len() => GenExpr::Tuple(fields),
                    ty::Adt(adt, _) if adt.is_struct() => {
                        let variant = adt.non_enum_variant();
                        if variant.fields.len() != fields.len() {
                            return Err(format!(
                                "`{ty}` has {} fields, {} given",
                                variant.fields.len(),
                                fields.len()
                            ));
                        }
                        let named = variant
                            .fields
                            .iter()
                            .zip(fields)
                            .map(|(def, expr)| (def.name.as_str().to_owned(), expr))
                            .collect();
                        GenExpr::Struct(gen_path(self.tcx, adt.did())?, named)
                    }
                    _ => return Err(format!("cannot construct `{ty}`")),
                }
            }
            "Field" => {
                // Field { base, ty, index }
                let base = sub(0)?;
                let ty = self.type_id_ty(&self.field(&v, 1)?)?;
                let idx = self.ecx.read_scalar(&self.field(&v, 2)?).map_err(ierr)?;
                let idx = idx.to_u16().map_err(ierr)? as usize;
                let name = match ty.kind() {
                    ty::Tuple(_) => idx.to_string(),
                    ty::Adt(adt, _) if adt.is_struct() => adt
                        .non_enum_variant()
                        .fields
                        .get(FieldIdx::from_usize(idx))
                        .ok_or_else(|| format!("`{ty}` has no field {idx}"))?
                        .name
                        .as_str()
                        .to_owned(),
                    _ => return Err(format!("cannot read a field of `{ty}`")),
                };
                GenExpr::Field(Box::new(base), name)
            }
            "Binary" => {
                let op = match self.variant(&self.field(&v, 0)?)?.0.as_str() {
                    "Add" => GenBinOp::Add,
                    "Subtract" => GenBinOp::Sub,
                    "Multiply" => GenBinOp::Mul,
                    "Divide" => GenBinOp::Div,
                    "Remainder" => GenBinOp::Rem,
                    "Equal" => GenBinOp::Eq,
                    "NotEqual" => GenBinOp::Ne,
                    "And" => GenBinOp::And,
                    "Or" => GenBinOp::Or,
                    other => return Err(format!("unknown binary operation `{other}`")),
                };
                let lhs = self.ecx.deref_pointer(&self.field(&v, 1)?).map_err(ierr)?;
                let rhs = self.ecx.deref_pointer(&self.field(&v, 2)?).map_err(ierr)?;
                GenExpr::Binary(
                    op,
                    Box::new(self.expr(&lhs, params)?),
                    Box::new(self.expr(&rhs, params)?),
                )
            }
            "Await" => GenExpr::Await(Box::new(sub(0)?)),
            other => return Err(format!("unsupported expression `{other}`")),
        })
    }

    fn integer(&self, ty: Ty<'tcx>, bits: u128) -> R<GenExpr> {
        let ptr_bits = self.tcx.data_layout.pointer_size().bits();
        let (name, width, signed) = match ty.kind() {
            ty::Int(i) => (i.name_str(), i.bit_width().unwrap_or(ptr_bits), true),
            ty::Uint(u) => (u.name_str(), u.bit_width().unwrap_or(ptr_bits), false),
            _ => return Err(format!("`{ty}` is not an integer type")),
        };
        let mask = if width >= 128 { u128::MAX } else { (1u128 << width) - 1 };
        let bits = bits & mask;
        let negative = signed && bits >> (width - 1) & 1 == 1;
        let magnitude = if negative { (!bits & mask).wrapping_add(1) & mask } else { bits };
        // `iN::MIN` has magnitude 2^(N-1), which wraps to itself above and is still correct.
        let magnitude = if negative && magnitude == 0 { 1u128 << (width - 1) } else { magnitude };
        Ok(GenExpr::Int { ty: name.to_owned(), negative, magnitude })
    }

    fn gen_ty(&self, ty: Ty<'tcx>) -> R<GenTy> {
        Ok(match ty.kind() {
            ty::Bool => GenTy::Prim("bool".into()),
            ty::Char => GenTy::Prim("char".into()),
            ty::Str => GenTy::Prim("str".into()),
            ty::Int(i) => GenTy::Prim(i.name_str().into()),
            ty::Uint(u) => GenTy::Prim(u.name_str().into()),
            ty::Float(f) => GenTy::Prim(f.name_str().into()),
            ty::Never => GenTy::Never,
            ty::Tuple(tys) => {
                GenTy::Tuple(tys.iter().map(|t| self.gen_ty(t)).collect::<R<Vec<_>>>()?)
            }
            ty::Slice(inner) => GenTy::Slice(Box::new(self.gen_ty(*inner)?)),
            ty::Array(inner, len) => {
                let len = len
                    .try_to_target_usize(self.tcx)
                    .ok_or_else(|| format!("array length of `{ty}` is not known"))?;
                GenTy::Array(Box::new(self.gen_ty(*inner)?), len)
            }
            ty::Ref(_, inner, mutability) => GenTy::Ref {
                mutable: mutability.is_mut(),
                inner: Box::new(self.gen_ty(*inner)?),
            },
            ty::Adt(adt, args) => {
                let args = args
                    .iter()
                    .filter_map(|arg| arg.as_type())
                    .map(|t| self.gen_ty(t))
                    .collect::<R<Vec<_>>>()?;
                GenTy::Path(gen_path(self.tcx, adt.did())?, args)
            }
            _ => return Err(format!("type `{ty}` cannot be named in a generated item")),
        })
    }
}

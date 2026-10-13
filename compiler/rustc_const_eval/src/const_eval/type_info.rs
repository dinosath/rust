mod adt;

use std::borrow::Cow;

use rustc_abi::{ExternAbi, FieldIdx, Size};
use rustc_attr_ir::lang_items::LangItem;
use rustc_data_structures::fx::FxHashSet;
use rustc_hir::def::DefKind;
use rustc_hir::def_id::{CRATE_DEF_INDEX, DefId};
use rustc_middle::ty::layout::TyAndLayout;
use rustc_middle::ty::{self, FnHeader, FnSigKind, FnSigTys, ScalarInt, Ty, TyCtxt};
use rustc_span::{RemapPathScopeComponents, Symbol, span_bug, sym};

use crate::const_eval::CompileTimeMachine;
use crate::interpret::{
    CtfeProvenance, Immediate, InterpCx, InterpResult, MPlaceTy, MemoryKind, OpTy,
    Scalar, Writeable, interp_ok,
};

impl<'tcx> InterpCx<'tcx, CompileTimeMachine<'tcx>> {
    // A general method to write an array to a static slice place.
    pub(super) fn allocate_fill_and_write_slice_ptr(
        &mut self,
        slice_place: &impl Writeable<'tcx, CtfeProvenance>,
        len: u64,
        writer: impl Fn(&mut Self, /* index */ u64, MPlaceTy<'tcx>) -> InterpResult<'tcx>,
    ) -> InterpResult<'tcx> {
        // Array element type
        let field_ty = slice_place
            .layout()
            .ty
            .builtin_deref(false)
            .unwrap()
            .sequence_element_type(self.tcx.tcx);

        // Allocate an array
        let array_layout = self.layout_of(Ty::new_array(self.tcx.tcx, field_ty, len))?;
        let array_place = self.allocate(array_layout, MemoryKind::Stack)?;

        // Fill the array fields
        let mut field_places = self.project_array_fields(&array_place)?;
        while let Some((i, place)) = field_places.next(self)? {
            writer(self, i, place)?;
        }

        // Write the slice pointing to the array
        let array_place = array_place.map_provenance(CtfeProvenance::as_immutable);
        let ptr = Immediate::new_slice(array_place.ptr(), len, self);
        self.write_immediate(ptr, slice_place)
    }

    /// Writes a `core::mem::type_info::TypeInfo` for a given type, `ty` to the given place.
    pub(crate) fn write_type_info(
        &mut self,
        ty: Ty<'tcx>,
        dest: &impl Writeable<'tcx, CtfeProvenance>,
    ) -> InterpResult<'tcx> {
        let ty_struct = self.tcx.require_lang_item(LangItem::Type, self.tcx.span);
        let ty_struct = self.tcx.type_of(ty_struct).no_bound_vars().unwrap();
        assert_eq!(ty_struct, dest.layout().ty);
        let ty_struct = ty_struct.ty_adt_def().unwrap().non_enum_variant();
        // Fill all fields of the `TypeInfo` struct.
        for (idx, field) in ty_struct.fields.iter_enumerated() {
            let field_dest = self.project_field(dest, idx)?;
            match field.name {
                sym::kind => {
                    let variant_index = match ty.kind() {
                        ty::Tuple(fields) => {
                            let (variant, variant_place) =
                                self.project_downcast_named(&field_dest, sym::Tuple)?;
                            // project to the single tuple variant field of `type_info::Tuple` struct type
                            let tuple_place = self.project_field(&variant_place, FieldIdx::ZERO)?;
                            assert_eq!(
                                1,
                                tuple_place
                                    .layout()
                                    .ty
                                    .ty_adt_def()
                                    .unwrap()
                                    .non_enum_variant()
                                    .fields
                                    .len()
                            );
                            self.write_tuple_type_info(tuple_place, fields, ty)?;
                            variant
                        }
                        ty::Array(_, _) => {
                            let (variant, _variant_place) =
                                self.project_downcast_named(&field_dest, sym::Array)?;
                            variant
                        }
                        ty::Slice(_) => {
                            let (variant, _variant_place) =
                                self.project_downcast_named(&field_dest, sym::Slice)?;
                            variant
                        }
                        ty::Adt(adt_def, _) => self.write_adt_type_info(&field_dest, *adt_def)?,
                        ty::Bool => {
                            let (variant, _variant_place) =
                                self.project_downcast_named(&field_dest, sym::Bool)?;
                            variant
                        }
                        ty::Char => {
                            let (variant, _variant_place) =
                                self.project_downcast_named(&field_dest, sym::Char)?;
                            variant
                        }
                        ty::Int(_) => {
                            let (variant, _variant_place) =
                                self.project_downcast_named(&field_dest, sym::Int)?;
                            variant
                        }
                        ty::Uint(_) => {
                            let (variant, _variant_place) =
                                self.project_downcast_named(&field_dest, sym::Int)?;
                            variant
                        }
                        ty::Float(_) => {
                            let (variant, _variant_place) =
                                self.project_downcast_named(&field_dest, sym::Float)?;
                            variant
                        }
                        ty::Str => {
                            let (variant, _variant_place) =
                                self.project_downcast_named(&field_dest, sym::Str)?;
                            variant
                        }
                        ty::Ref(_, _, _) => {
                            let (variant, _) =
                                self.project_downcast_named(&field_dest, sym::Reference)?;
                            variant
                        }
                        ty::RawPtr(_, _) => {
                            let (variant, _variant_place) =
                                self.project_downcast_named(&field_dest, sym::Pointer)?;
                            variant
                        }
                        ty::Dynamic(predicates, region) => {
                            let (variant, variant_place) =
                                self.project_downcast_named(&field_dest, sym::DynTrait)?;
                            let dyn_place = self.project_field(&variant_place, FieldIdx::ZERO)?;
                            self.write_dyn_trait_type_info(dyn_place, *predicates, *region)?;
                            variant
                        }
                        ty::FnPtr(_, _) => {
                            let (variant, _) =
                                self.project_downcast_named(&field_dest, sym::FnPtr)?;
                            variant
                        }
                        ty::FnDef(def_id, args) => {
                            let (variant, variant_place) =
                                self.project_downcast_named(&field_dest, sym::FnDef)?;
                            let fn_def_place =
                                self.project_field(&variant_place, FieldIdx::ZERO)?;
                            self.write_fn_def_type_info(
                                fn_def_place,
                                *def_id,
                                args.skip_binder(),
                            )?;
                            variant
                        }
                        ty::Foreign(_)
                        | ty::Pat(_, _)
                        | ty::UnsafeBinder(..)
                        | ty::Closure(..)
                        | ty::CoroutineClosure(..)
                        | ty::Coroutine(..)
                        | ty::CoroutineWitness(..)
                        | ty::Never
                        | ty::Alias(..)
                        | ty::Param(_)
                        | ty::Bound(..)
                        | ty::Placeholder(_)
                        | ty::Infer(..)
                        | ty::Error(_) => self.project_downcast_named(&field_dest, sym::Other)?.0,
                    };
                    self.write_discriminant(variant_index, &field_dest)?
                }
                other => span_bug!(self.tcx.span, "unknown `Type` field {other}"),
            }
        }

        interp_ok(())
    }

    fn write_field(
        &mut self,
        field_ty: Ty<'tcx>,
        place: MPlaceTy<'tcx>,
        layout: TyAndLayout<'tcx>,
        name: Option<Symbol>,
        idx: u64,
    ) -> InterpResult<'tcx> {
        for (field_idx, field_ty_field) in
            place.layout.ty.ty_adt_def().unwrap().non_enum_variant().fields.iter_enumerated()
        {
            let field_place = self.project_field(&place, field_idx)?;
            match field_ty_field.name {
                sym::name => {
                    let name = match name.as_ref() {
                        Some(name) => Cow::Borrowed(name.as_str()),
                        None => Cow::Owned(idx.to_string()), // For tuples
                    };
                    let name_place = self.allocate_str_dedup(&name)?;
                    let ptr = self.mplace_to_imm_ptr(&name_place, None)?;
                    self.write_immediate(*ptr, &field_place)?
                }
                sym::ty => {
                    let field_ty = self.tcx.erase_and_anonymize_regions(field_ty);
                    self.write_type_id(field_ty, &field_place)?
                }
                sym::offset => {
                    let offset = layout.fields.offset(idx as usize);
                    self.write_scalar(
                        ScalarInt::try_from_target_usize(offset.bytes(), self.tcx.tcx).unwrap(),
                        &field_place,
                    )?;
                }
                other => {
                    span_bug!(self.tcx.def_span(field_ty_field.did), "unimplemented field {other}")
                }
            }
        }
        interp_ok(())
    }

    pub(crate) fn write_tuple_type_info(
        &mut self,
        tuple_place: impl Writeable<'tcx, CtfeProvenance>,
        fields: &[Ty<'tcx>],
        tuple_ty: Ty<'tcx>,
    ) -> InterpResult<'tcx> {
        let tuple_layout = self.layout_of(tuple_ty)?;
        let fields_slice_place = self.project_field(&tuple_place, FieldIdx::ZERO)?;
        self.allocate_fill_and_write_slice_ptr(
            &fields_slice_place,
            fields.len() as u64,
            |this, i, place| {
                let field_ty = fields[i as usize];
                this.write_field(field_ty, place, tuple_layout, None, i)
            },
        )
    }

    pub(crate) fn write_type_id_generics(
        &mut self,
        place: &impl Writeable<'tcx, CtfeProvenance>,
        ty: Ty<'tcx>,
    ) -> InterpResult<'tcx> {
        let generics: ty::Binder<'_, ty::GenericArgsRef<'_>> = match *ty.kind() {
            ty::Bool
            | ty::Char
            | ty::Int(..)
            | ty::Uint(..)
            | ty::Float(..)
            | ty::Foreign(..)
            | ty::Str
            | ty::Array(..)
            | ty::Pat(..)
            | ty::RawPtr(..)
            | ty::Ref(..)
            | ty::FnPtr(..)
            | ty::Dynamic(..)
            | ty::CoroutineWitness(..)
            | ty::Never
            | ty::Tuple(..)
            | ty::Alias(..)
            | ty::Param(..)
            | ty::Bound(..)
            | ty::Placeholder(..)
            | ty::Infer(..)
            | ty::Error(..)
            | ty::Slice(..) => ty::Binder::dummy(ty::GenericArgsRef::default()),
            ty::Adt(_, args) => ty::Binder::dummy(args),
            ty::FnDef(_, binder) => binder,
            ty::UnsafeBinder(binder) => binder.rebind(ty::GenericArgsRef::default()),
            ty::Closure(_, args) => ty::Binder::dummy(args),
            ty::CoroutineClosure(_, args) => ty::Binder::dummy(args),
            ty::Coroutine(_, args) => ty::Binder::dummy(args),
        };

        // FIXME(type_info): also provide the late bound vars to reflection
        let generics = generics.skip_binder();

        self.write_generics(place, generics)
    }

    pub(crate) fn write_fn_ptr_type_info(
        &mut self,
        place: impl Writeable<'tcx, CtfeProvenance>,
        sig: &FnSigTys<TyCtxt<'tcx>>,
        fn_header: &FnHeader<TyCtxt<'tcx>>,
    ) -> InterpResult<'tcx> {
        let FnHeader { fn_sig_kind } = fn_header;

        for (field_idx, field) in
            place.layout().ty.ty_adt_def().unwrap().non_enum_variant().fields.iter_enumerated()
        {
            let field_place = self.project_field(&place, field_idx)?;

            match field.name {
                sym::is_unsafe => {
                    self.write_scalar(Scalar::from_bool(!fn_sig_kind.is_safe()), &field_place)?;
                }
                sym::abi => match fn_sig_kind.abi() {
                    ExternAbi::C { .. } => {
                        let (rust_variant, _rust_place) =
                            self.project_downcast_named(&field_place, sym::ExternC)?;
                        self.write_discriminant(rust_variant, &field_place)?;
                    }
                    ExternAbi::Rust => {
                        let (rust_variant, _rust_place) =
                            self.project_downcast_named(&field_place, sym::ExternRust)?;
                        self.write_discriminant(rust_variant, &field_place)?;
                    }
                    other_abi => {
                        let (variant, variant_place) =
                            self.project_downcast_named(&field_place, sym::Named)?;
                        let str_place = self.allocate_str_dedup(other_abi.as_str())?;
                        let str_ref = self.mplace_to_imm_ptr(&str_place, None)?;
                        let payload = self.project_field(&variant_place, FieldIdx::ZERO)?;
                        self.write_immediate(*str_ref, &payload)?;
                        self.write_discriminant(variant, &field_place)?;
                    }
                },
                sym::inputs => {
                    let inputs = sig.inputs();
                    self.allocate_fill_and_write_slice_ptr(
                        &field_place,
                        inputs.len() as _,
                        |this, i, place| this.write_type_id(inputs[i as usize], &place),
                    )?;
                }
                sym::output => {
                    let output = sig.output();
                    self.write_type_id(output, &field_place)?;
                }
                sym::variadic => {
                    self.write_scalar(Scalar::from_bool(fn_sig_kind.c_variadic()), &field_place)?;
                }
                sym::is_splatted => {
                    self.write_scalar(
                        Scalar::from_bool(fn_sig_kind.splatted().is_some()),
                        &field_place,
                    )?;
                }
                sym::splatted_index => {
                    self.write_scalar(
                        Scalar::from_u8(
                            // Currently the same encoding as FnSigKind.splatted
                            // FIXME(splat): make these two fields into a single Option<u8/u16>, or choose a stable encoding
                            fn_sig_kind.splatted().unwrap_or(FnSigKind::NO_SPLATTED_ARG_INDEX),
                        ),
                        &field_place,
                    )?;
                }
                other => span_bug!(self.tcx.def_span(field.did), "unimplemented field {other}"),
            }
        }

        interp_ok(())
    }

    pub(crate) fn write_fn_def_type_info(
        &mut self,
        place: impl Writeable<'tcx, CtfeProvenance>,
        def_id: DefId,
        args: ty::GenericArgsRef<'tcx>,
    ) -> InterpResult<'tcx> {
        let sig = self
            .tcx
            .tcx
            .fn_sig(def_id)
            .instantiate(self.tcx.tcx, args)
            .skip_norm_wip()
            .skip_binder();
        let name = self.tcx.tcx.item_name(def_id);
        let span = self.tcx.tcx.def_span(def_id);
        let source_file = self.tcx.tcx.sess.source_map().lookup_source_file(span.lo());
        let file_name = source_file
            .name
            .display(RemapPathScopeComponents::DIAGNOSTICS)
            .to_string_lossy()
            .into_owned();
        let span_start = span.lo().0 - source_file.start_pos.0;
        let span_end = span.hi().0 - source_file.start_pos.0;
        let is_async = self.tcx.tcx.asyncness(def_id).is_async();
        let declared_output = if is_async {
            self.tcx.tcx.get_impl_future_output_ty(sig.output())
        } else {
            Some(sig.output())
        };
        let param_names: Vec<Option<Symbol>> = def_id
            .as_local()
            .map(|local_def_id| {
                self.tcx
                    .tcx
                    .hir_body_owned_by(local_def_id)
                    .params
                    .iter()
                    .map(|param| match param.pat.kind {
                        rustc_hir::PatKind::Binding(_, _, ident, _) => Some(ident.name),
                        _ => None,
                    })
                    .collect()
            })
            .unwrap_or_else(|| vec![None; sig.inputs().len()]);

        for (field_idx, field) in
            place.layout().ty.ty_adt_def().unwrap().non_enum_variant().fields.iter_enumerated()
        {
            let field_place = self.project_field(&place, field_idx)?;
            match field.name {
                sym::name => {
                    let name_place = self.allocate_str_dedup(name.as_str())?;
                    let name_ref = self.mplace_to_imm_ptr(&name_place, None)?;
                    self.write_immediate(*name_ref, &field_place)?;
                }
                sym::span => {
                    for (span_field_idx, span_field) in field_place
                        .layout()
                        .ty
                        .ty_adt_def()
                        .unwrap()
                        .non_enum_variant()
                        .fields
                        .iter_enumerated()
                    {
                        let span_field_place = self.project_field(&field_place, span_field_idx)?;
                        match span_field_idx.as_u32() {
                            0 => {
                                let file_place = self.allocate_str_dedup(&file_name)?;
                                let file_ref = self.mplace_to_imm_ptr(&file_place, None)?;
                                self.write_immediate(*file_ref, &span_field_place)?;
                            }
                            1 => self.write_scalar(
                                ScalarInt::try_from_uint(span_start, Size::from_bytes(4)).unwrap(),
                                &span_field_place,
                            )?,
                            2 => self.write_scalar(
                                ScalarInt::try_from_uint(span_end, Size::from_bytes(4)).unwrap(),
                                &span_field_place,
                            )?,
                            _ => span_bug!(
                                self.tcx.def_span(span_field.did),
                                "unexpected SourceSpan field {}",
                                span_field.name
                            ),
                        }
                    }
                }
                sym::inputs => {
                    let inputs = sig.inputs();
                    self.allocate_fill_and_write_slice_ptr(
                        &field_place,
                        inputs.len() as u64,
                        |this, i, place| this.write_type_id(inputs[i as usize], &place),
                    )?;
                }
                sym::param_names => {
                    self.allocate_fill_and_write_slice_ptr(
                        &field_place,
                        param_names.len() as u64,
                        |this, i, place| this.write_opt_str(&place, param_names[i as usize]),
                    )?;
                }
                sym::output => self.write_type_id(sig.output(), &field_place)?,
                sym::declared_output => {
                    self.write_opt_type_id(&field_place, declared_output)?
                }
                sym::is_async => {
                    self.write_scalar(Scalar::from_bool(is_async), &field_place)?;
                }
                other => span_bug!(self.tcx.def_span(def_id), "unimplemented FnDef field {other}"),
            }
        }

        interp_ok(())
    }
}

// Attribute reflection. Only tool attributes (`#![register_tool(..)]`) are visible, as
// built-in attributes are parsed into `AttributeKind`s and carry no token-level arguments.
impl<'tcx> InterpCx<'tcx, CompileTimeMachine<'tcx>> {
    pub(super) fn read_static_str(&self, op: &OpTy<'tcx>) -> InterpResult<'tcx, String> {
        let place = self.deref_pointer(op)?;
        interp_ok(self.read_str(&place)?.to_owned())
    }

    pub(super) fn frt_field_def_id(&self, frt_ty: Ty<'tcx>) -> DefId {
        if let ty::Adt(def, args) = frt_ty.kind()
            && let Some(ty::FieldInfo { base, variant_idx, field_idx, .. }) =
                def.field_representing_type_info(self.tcx.tcx, args)
            && let ty::Adt(base_def, _) = base.kind()
        {
            base_def.variant(variant_idx).fields[field_idx].did
        } else {
            span_bug!(self.cur_span(), "expected field representing type, got {frt_ty}")
        }
    }

    fn attr_path(path: &str) -> Vec<Symbol> {
        path.split("::").map(Symbol::intern).collect()
    }

    pub(super) fn def_has_attr(&self, def_id: DefId, path: &str) -> bool {
        self.tcx.get_attrs_by_path(def_id, &Self::attr_path(path)).next().is_some()
    }

    /// `key == ""` selects the `#[path = lit]` form, otherwise `#[path(key = lit, ..)]`.
    pub(super) fn def_attr_value(&self, def_id: DefId, path: &str, key: &str) -> Option<Symbol> {
        let text = |lit: &rustc_ast::MetaItemLit| lit.kind.str().unwrap_or(lit.symbol);
        for attr in self.tcx.get_attrs_by_path(def_id, &Self::attr_path(path)) {
            if key.is_empty() {
                if let Some(lit) = attr.value_lit() {
                    return Some(text(lit));
                }
                continue;
            }
            for item in attr.meta_item_list().into_iter().flatten() {
                if let Some(mi) = item.meta_item()
                    && mi.ident().is_some_and(|ident| ident.as_str() == key)
                    && let Some(lit) = mi.name_value_literal()
                {
                    return Some(text(lit));
                }
            }
        }
        None
    }

    pub(super) fn write_opt_str(
        &mut self,
        dest: &impl Writeable<'tcx, CtfeProvenance>,
        value: Option<Symbol>,
    ) -> InterpResult<'tcx> {
        let Some(value) = value else {
            let (variant, _) = self.project_downcast_named(dest, sym::None)?;
            return self.write_discriminant(variant, dest);
        };
        let (variant, variant_place) = self.project_downcast_named(dest, sym::Some)?;
        let field_place = self.project_field(&variant_place, FieldIdx::ZERO)?;
        let ptr = self.allocate_bytes_dedup(value.as_str().as_bytes())?;
        self.write_immediate(
            Immediate::ScalarPair(
                Scalar::from_pointer(ptr, self),
                Scalar::from_target_usize(value.as_str().len() as u64, self),
            ),
            &field_place,
        )?;
        self.write_discriminant(variant, dest)
    }

    pub(super) fn write_opt_type_id(
        &mut self,
        dest: &impl Writeable<'tcx, CtfeProvenance>,
        value: Option<Ty<'tcx>>,
    ) -> InterpResult<'tcx> {
        let Some(ty) = value else {
            let (variant, _) = self.project_downcast_named(dest, sym::None)?;
            return self.write_discriminant(variant, dest);
        };
        let (variant, variant_place) = self.project_downcast_named(dest, sym::Some)?;
        let field_place = self.project_field(&variant_place, FieldIdx::ZERO)?;
        self.write_type_id(ty, &field_place)?;
        self.write_discriminant(variant, dest)
    }

    /// Non-generic ADTs of the local crate with the given attribute, in definition order.
    pub(super) fn write_crate_types_with_attr(
        &mut self,
        dest: &impl Writeable<'tcx, CtfeProvenance>,
        attr: &str,
    ) -> InterpResult<'tcx> {
        let tcx = *self.tcx;
        let tys: Vec<Ty<'tcx>> = tcx
            .hir_crate_items(())
            .definitions()
            .filter(|&def_id| {
                matches!(tcx.def_kind(def_id), DefKind::Struct | DefKind::Enum | DefKind::Union)
                    && tcx.generics_of(def_id).count() == 0
                    && self.def_has_attr(def_id.to_def_id(), attr)
            })
            .map(|def_id| Ty::new_adt(tcx, tcx.adt_def(def_id), ty::List::empty()))
            .collect();
        self.allocate_fill_and_write_slice_ptr(dest, tys.len() as u64, |this, i, place| {
            this.write_type_id(tys[i as usize], &place)
        })
    }

    /// Non-generic functions of the local crate with the given attribute, in definition order.
    pub(super) fn write_crate_functions_with_attr(
        &mut self,
        dest: &impl Writeable<'tcx, CtfeProvenance>,
        attr: &str,
    ) -> InterpResult<'tcx> {
        let tcx = *self.tcx;
        let tys: Vec<Ty<'tcx>> = tcx
            .hir_crate_items(())
            .definitions()
            .filter(|&def_id| {
                matches!(tcx.def_kind(def_id), DefKind::Fn | DefKind::AssocFn)
                    && tcx.generics_of(def_id).count() == 0
                    && self.def_has_attr(def_id.to_def_id(), attr)
            })
            .map(|def_id| tcx.type_of(def_id).instantiate_identity().skip_norm_wip())
            .collect();
        self.allocate_fill_and_write_slice_ptr(dest, tys.len() as u64, |this, i, place| {
            this.write_type_id(tys[i as usize], &place)
        })
    }

    /// Non-generic functions with the given attribute in the local crate and all loaded crates.
    pub(super) fn write_all_functions_with_attr(
        &mut self,
        dest: &impl Writeable<'tcx, CtfeProvenance>,
        attr: &str,
    ) -> InterpResult<'tcx> {
        let tcx = *self.tcx;
        let is_attributed_function = |def_id: DefId| {
            matches!(tcx.def_kind(def_id), DefKind::Fn | DefKind::AssocFn)
                && tcx.generics_of(def_id).count() == 0
                && self.def_has_attr(def_id, attr)
        };

        let mut attributed_functions = Vec::new();
        {
            let mut seen = FxHashSet::default();
            let mut record_function = |def_id| {
                if is_attributed_function(def_id) && seen.insert(def_id) {
                    attributed_functions.push(def_id);
                }
            };

            for def_id in tcx.hir_crate_items(()).definitions() {
                record_function(def_id.to_def_id());
            }

            for &cnum in tcx.crates(()) {
                let root = DefId { krate: cnum, index: CRATE_DEF_INDEX };
                let mut modules = vec![root];
                let mut seen_modules = FxHashSet::default();

                while let Some(module) = modules.pop() {
                    if !seen_modules.insert(module) {
                        continue;
                    }

                    for child in tcx.module_children(module) {
                        let Some(def_id) = child.res.opt_def_id() else {
                            continue;
                        };

                        match tcx.def_kind(def_id) {
                            DefKind::Mod => modules.push(def_id),
                            DefKind::Fn | DefKind::AssocFn => record_function(def_id),
                            DefKind::Struct | DefKind::Union | DefKind::Enum => {
                                for &impl_id in tcx.inherent_impls(def_id) {
                                    for item in tcx.associated_items(impl_id).in_definition_order() {
                                        record_function(item.def_id);
                                    }
                                }
                            }
                            _ => {}
                        }
                    }
                }

                for &trait_id in tcx.traits(cnum) {
                    for item in tcx.associated_items(trait_id).in_definition_order() {
                        record_function(item.def_id);
                    }
                }

                for &impl_id in tcx.trait_impls_in_crate(cnum) {
                    for item in tcx.associated_items(impl_id).in_definition_order() {
                        record_function(item.def_id);
                    }
                }
            }
        }

        let tys: Vec<Ty<'tcx>> = attributed_functions
            .into_iter()
            .map(|def_id| tcx.type_of(def_id).instantiate_identity().skip_norm_wip())
            .collect();

        self.allocate_fill_and_write_slice_ptr(dest, tys.len() as u64, |this, i, place| {
            this.write_type_id(tys[i as usize], &place)
        })
    }
}

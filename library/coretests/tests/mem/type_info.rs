#![allow(dead_code)]

use std::any::{Any, TypeId};
use std::mem::offset_of;
use std::mem::type_info::{Abi, Const, Generic, GenericType, Type, TypeKind};

const fn path_ends_with(path: &str, suffix: &str) -> bool {
    let path = path.as_bytes();
    let suffix = suffix.as_bytes();
    if path.len() < suffix.len() {
        return false;
    }

    let offset = path.len() - suffix.len();
    let mut i = 0;
    while i < suffix.len() {
        if path[offset + i] != suffix[i] {
            return false;
        }
        i += 1;
    }
    true
}

const LOCAL_MODULES: &[&str] = std::mem::type_info::local_modules();

mod reflection_fixture {
    pub mod api {
        #[meta::http_get]
        pub fn get_user(id: u32) -> bool {
            id != 0
        }

        #[meta::http_post]
        pub fn create_user(id: u32) -> bool {
            id == 7
        }

        #[meta::generic_handler]
        pub fn identity<T: Copy>(value: T) -> T {
            value
        }

        #[meta::async_handler]
        pub async fn fetch() -> u32 {
            1
        }

        #[meta::const_handler]
        pub const fn policy() -> u32 {
            1
        }

        #[meta::unsafe_handler]
        pub unsafe extern "C" fn signal() {}

        pub struct Controller;

        impl Controller {
            #[meta::route]
            pub fn health() -> bool {
                true
            }
        }
    }

    pub mod jobs {
        #[meta::scheduled]
        pub fn prune_sessions() {}

        pub struct Runner;

        impl Runner {
            #[meta::background]
            pub fn refresh_cache() {}
        }
    }
}

#[derive(Copy, Clone)]
struct StaticRoute {
    method: &'static str,
    path: &'static str,
    module_path: &'static str,
    handler_name: &'static str,
    handler: fn(u32) -> bool,
}

const HTTP_ROUTE_COUNT: usize = const {
    let functions = std::mem::type_info::local_functions();
    let mut count = 0;
    let mut i = 0;
    while i < functions.len() {
        if functions[i].has_attr("meta::http_get") || functions[i].has_attr("meta::http_post") {
            count += 1;
        }
        i += 1;
    }
    count
};

static HTTP_ROUTER: [StaticRoute; HTTP_ROUTE_COUNT] = const {
    let empty_route = StaticRoute {
        method: "",
        path: "",
        module_path: "",
        handler_name: "",
        handler: reflection_fixture::api::get_user,
    };
    let mut routes = [empty_route; HTTP_ROUTE_COUNT];
    let mut count = 0;
    let functions = std::mem::type_info::local_functions();
    let mut i = 0;
    while i < functions.len() {
        let function = functions[i];
        let method = if function.has_attr("meta::http_get") {
            Some("GET")
        } else if function.has_attr("meta::http_post") {
            Some("POST")
        } else {
            None
        };
        if let Some(method) = method {
            let Some(module_path) = function.function_module_path() else { panic!() };
            let Some(handler_name) = function.function_name() else { panic!() };
            let (path, handler) = if handler_name == "get_user" {
                ("/api/users/{id}", reflection_fixture::api::get_user as fn(u32) -> bool)
            } else if handler_name == "create_user" {
                ("/api/users", reflection_fixture::api::create_user as fn(u32) -> bool)
            } else {
                panic!()
            };
            routes[count] = StaticRoute { method, path, module_path, handler_name, handler };
            count += 1;
        }
        i += 1;
    }
    assert!(count == HTTP_ROUTE_COUNT);
    routes
};

fn dispatch_http_route(method: &str, path: &str, id: u32) -> Option<bool> {
    let mut i = 0;
    while i < HTTP_ROUTER.len() {
        let route = HTTP_ROUTER[i];
        if route.method == method && route.path == path {
            return Some((route.handler)(id));
        }
        i += 1;
    }
    None
}

#[test]
fn test_attributes() {
    #[meta::record]
    struct SampleType;

    #[meta::nested]
    struct NestedSampleType {
        sample: SampleType,
    }

    #[meta::outer]
    struct OuterSampleType {
        nested: NestedSampleType,
    }

    struct UntaggedSampleType;

    #[repr(C)]
    struct ParsedSampleType;

    #[rustfmt::skip]
    struct RustfmtSkippedSampleType;

    #[meta::record]
    fn attribute_function() {}

    const fn type_id_of_value<T: 'static>(_: &T) -> TypeId {
        TypeId::of::<T>()
    }

    const {
        let sample = TypeId::of::<SampleType>();
        assert!(TypeId::of::<RustfmtSkippedSampleType>().has_attr("rustfmt::skip"));
        assert!(sample.has_attr("meta::record"));
        assert!(type_id_of_value(&attribute_function).has_attr("meta::record"));
        let nested = TypeId::of::<OuterSampleType>().field(0, 0).type_id();
        assert!(nested.has_attr("meta::nested"));
        assert!(!nested.has_attr("meta::outer"));
        assert!(!sample.has_attr("meta::missing"));
        assert!(!TypeId::of::<UntaggedSampleType>().has_attr("meta::record"));
        assert!(!TypeId::of::<ParsedSampleType>().has_attr("repr"));
        assert!(!TypeId::of::<u8>().has_attr("meta::record"));
    }
}

#[test]
fn test_local_reflected_items() {
    assert!(HTTP_ROUTER.len() == 2);
    assert!(HTTP_ROUTER.iter().any(|route| {
        route.method == "GET"
            && route.path == "/api/users/{id}"
            && path_ends_with(route.module_path, "api")
            && route.handler_name == "get_user"
    }));
    assert!(HTTP_ROUTER.iter().any(|route| {
        route.method == "POST"
            && route.path == "/api/users"
            && path_ends_with(route.module_path, "api")
            && route.handler_name == "create_user"
    }));

    const {
        let mut http_gets = 0;
        let mut http_posts = 0;
        let mut routes = 0;
        let mut scheduled_jobs = 0;
        let mut background_jobs = 0;
        let mut found_get_signature = false;
        let mut found_generic_signature = false;
        let mut found_async_function = false;
        let mut found_const_function = false;
        let mut found_unsafe_c_function = false;
        let mut api_routes = 0;
        let mut job_functions = 0;
        let functions = std::mem::type_info::local_functions();
        let modules = std::mem::type_info::local_modules();
        let mut i = 0;

        while i < functions.len() {
            let function = functions[i];
            if function.has_attr("meta::http_get") {
                http_gets += 1;
                let signature = function.function_ptr().unwrap();
                found_get_signature = signature.inputs().len() == 1
                    && signature.inputs()[0] == TypeId::of::<u32>()
                    && signature.output() == TypeId::of::<bool>()
                    && function.function_name() == Some("get_user");
            }
            if function.has_attr("meta::http_post") {
                http_posts += 1;
            }
            if function.has_attr("meta::route") {
                routes += 1;
            }
            if function.has_attr("meta::scheduled") {
                scheduled_jobs += 1;
            }
            if function.has_attr("meta::background") {
                background_jobs += 1;
            }
            if function.has_attr("meta::route")
                || function.has_attr("meta::scheduled")
                || function.has_attr("meta::background")
            {
                if let Some(path) = function.function_module_path() {
                    let mut m = 0;
                    while m < modules.len() {
                        if modules[m] == path {
                            if function.has_attr("meta::route") {
                                api_routes += 1;
                            }
                            if function.has_attr("meta::scheduled")
                                || function.has_attr("meta::background")
                            {
                                job_functions += 1;
                            }
                        }
                        m += 1;
                    }
                }
            }
            if function.has_attr("meta::generic_handler") {
                let generics = function.generic_parameters();
                let signature = function.function_ptr().unwrap();
                if let [Generic::Type(GenericType { ty, .. })] = generics {
                    found_generic_signature = function.function_name() == Some("identity")
                        && signature.inputs().len() == 1
                        && signature.inputs()[0] == *ty
                        && signature.output() == *ty;
                }
            }
            if function.has_attr("meta::async_handler") {
                found_async_function = function.function_name() == Some("fetch")
                    && function.is_async_fn()
                    && function.function_ptr().is_some();
            }
            if function.has_attr("meta::const_handler") {
                found_const_function = function.function_name() == Some("policy")
                    && function.is_const_fn();
            }
            if function.has_attr("meta::unsafe_handler") {
                let signature = function.function_ptr().unwrap();
                found_unsafe_c_function = function.function_name() == Some("signal")
                    && signature.is_unsafe()
                    && matches!(signature.abi(), Abi::ExternC);
            }
            i += 1;
        }

        assert!(http_gets == 1);
        assert!(http_posts == 1);
        assert!(routes == 1);
        assert!(scheduled_jobs == 1);
        assert!(background_jobs == 1);
        assert!(found_get_signature);
        assert!(found_generic_signature);
        assert!(found_async_function);
        assert!(found_const_function);
        assert!(found_unsafe_c_function);

        assert!(api_routes == 1);
        assert!(job_functions == 2);
    }

    assert!(LOCAL_MODULES.iter().any(|path| path_ends_with(path, "api")));
    assert!(LOCAL_MODULES.iter().any(|path| path_ends_with(path, "jobs")));
}

#[test]
fn test_reflected_router_routes_and_invokes_handlers() {
    assert_eq!(dispatch_http_route("GET", "/api/users/{id}", 42), Some(true));
    assert_eq!(dispatch_http_route("GET", "/api/users/{id}", 0), Some(false));
    assert_eq!(dispatch_http_route("POST", "/api/users", 7), Some(true));
    assert_eq!(dispatch_http_route("POST", "/api/users", 42), Some(false));
    assert_eq!(dispatch_http_route("POST", "/api/users/{id}", 42), None);
    assert_eq!(dispatch_http_route("DELETE", "/api/users", 7), None);
}

#[test]
fn test_arrays() {
    // Normal array.
    assert!(matches!(Type::of::<[u16; 4]>().kind, TypeKind::Array));
    const {
        let ty_id = TypeId::of::<[u16; 4]>();
        assert!(ty_id.element_ty() == Some(TypeId::of::<u16>()));
        assert!(ty_id.array_len() == 4);
    }

    // Zero-length array.
    assert!(matches!(Type::of::<[bool; 0]>().kind, TypeKind::Array));
    const {
        let ty_id = TypeId::of::<[bool; 0]>();
        assert!(ty_id.element_ty() == Some(TypeId::of::<bool>()));
        assert!(ty_id.array_len() == 0);
    }
}

#[test]
fn test_slices() {
    assert!(matches!(Type::of::<[usize]>().kind, TypeKind::Slice));
    const {
        assert!(TypeId::of::<[usize]>().element_ty() == Some(TypeId::of::<usize>()));
    }
}

#[test]
fn test_tuples() {
    const {
        let ty_id = TypeId::of::<()>();
        assert!(ty_id.size() == Some(size_of::<()>()));
        assert!(ty_id.variants() == 1);
        assert!(ty_id.fields(0) == 0);

        let ty_id = TypeId::of::<(u8,)>();
        assert!(ty_id.size() == Some(size_of::<(u8,)>()));
        assert!(ty_id.variants() == 1);
        assert!(ty_id.fields(0) == 1);
        assert!(ty_id.field(0, 0).type_id() == TypeId::of::<u8>());

        let ty_id = TypeId::of::<(u8, u16)>();
        assert!(ty_id.size() == Some(size_of::<(u8, u16)>()));
        assert!(ty_id.variants() == 1);
        assert!(ty_id.fields(0) == 2);
        assert!(ty_id.field(0, 0).type_id() == TypeId::of::<u8>());
        assert!(ty_id.field(0, 1).type_id() == TypeId::of::<u16>());
    }
}

#[test]
fn test_structs() {
    const {
        struct TestStruct {
            first: u8,
            second: u16,
            reference: &'static u16,
        }

        let ty_id = TypeId::of::<TestStruct>();
        assert!(!ty_id.non_exhaustive());
        assert!(ty_id.size() == Some(size_of::<TestStruct>()));
        assert!(ty_id.variants() == 1);
        assert!(ty_id.fields(0) == 3);
        assert!(ty_id.field(0, 0).type_id() == TypeId::of::<u8>());
        assert!(ty_id.field(0, 0).offset() == offset_of!(TestStruct, first));
        assert!(ty_id.field(0, 0).name() == "first");
        assert!(ty_id.field(0, 1).type_id() == TypeId::of::<u16>());
        assert!(ty_id.field(0, 1).offset() == offset_of!(TestStruct, second));
        assert!(ty_id.field(0, 1).name() == "second");
        assert!(ty_id.field(0, 2).type_id() == TypeId::of::<&u16>());
        assert!(ty_id.field(0, 2).offset() == offset_of!(TestStruct, reference));
        assert!(ty_id.field(0, 2).name() == "reference");
    }

    const {
        #[non_exhaustive]
        struct NonExhaustive {
            a: u8,
        }

        assert!(TypeId::of::<NonExhaustive>().non_exhaustive());
    }

    const {
        struct TupleStruct(u8, u16);

        let ty_id = TypeId::of::<TupleStruct>();
        assert!(ty_id.size() == Some(size_of::<TupleStruct>()));
        assert!(ty_id.variants() == 1);
        assert!(ty_id.fields(0) == 2);
        assert!(ty_id.field(0, 0).type_id() == TypeId::of::<u8>());
        assert!(ty_id.field(0, 0).name() == "0");
        assert!(ty_id.field(0, 1).type_id() == TypeId::of::<u16>());
        assert!(ty_id.field(0, 1).name() == "1");
    }

    const {
        struct Generics<'a, T, const C: u64> {
            a: &'a T,
        }

        let ty_id = TypeId::of::<Generics<'static, i32, 1_u64>>();
        assert!(ty_id.fields(0) == 1);
        assert!(ty_id.generics().len() == 3);

        let Generic::Lifetime(_) = ty_id.generics()[0] else { panic!() };
        let Generic::Type(GenericType { ty: generic_ty, .. }) = ty_id.generics()[1] else {
            panic!()
        };
        assert!(generic_ty == TypeId::of::<i32>());
        let Generic::Const(Const { ty: const_ty, .. }) = ty_id.generics()[2] else { panic!() };
        assert!(const_ty == TypeId::of::<u64>());
    }
}

#[test]
fn test_unions() {
    const {
        union TestUnion {
            first: i16,
            second: u16,
        }

        let ty_id = TypeId::of::<TestUnion>();
        assert!(ty_id.size() == Some(size_of::<TestUnion>()));
        assert!(ty_id.variants() == 1);
        assert!(ty_id.fields(0) == 2);
        assert!(ty_id.field(0, 0).name() == "first");
        assert!(ty_id.field(0, 0).type_id() == TypeId::of::<i16>());
        assert!(ty_id.field(0, 1).name() == "second");
        assert!(ty_id.field(0, 1).type_id() == TypeId::of::<u16>());
    }

    const {
        union Generics<'a, T: Copy, const C: u64> {
            a: T,
            z: &'a (),
        }

        let ty_id = TypeId::of::<Generics<'static, i32, 1_u64>>();
        assert!(ty_id.fields(0) == 2);
        assert!(ty_id.field(0, 1).offset() == offset_of!(Generics<'static, i32, 1_u64>, a));
        assert!(ty_id.field(0, 1).offset() == offset_of!(Generics<'static, i32, 1_u64>, z));

        assert!(ty_id.generics().len() == 3);
        let Generic::Lifetime(_) = ty_id.generics()[0] else { panic!() };
        let Generic::Type(GenericType { ty: generic_ty, .. }) = ty_id.generics()[1] else {
            panic!()
        };
        assert!(generic_ty == TypeId::of::<i32>());
        let Generic::Const(Const { ty: const_ty, .. }) = ty_id.generics()[2] else { panic!() };
        assert!(const_ty == TypeId::of::<u64>());
    }
}

#[test]
fn test_enums() {
    const {
        enum E {
            Some(u32),
            None,
            #[non_exhaustive]
            Foomp {
                a: (),
                b: &'static str,
            },
        }

        let ty_id = TypeId::of::<E>();
        assert!(!ty_id.non_exhaustive());
        assert!(ty_id.size() == Some(size_of::<E>()));
        assert!(ty_id.variants() == 3);

        assert!(ty_id.variant(0).name() == "Some");
        assert!(!ty_id.variant(0).non_exhaustive());
        assert!(ty_id.fields(0) == 1);
        assert!(ty_id.field(0, 0).type_id() == TypeId::of::<u32>());

        assert!(ty_id.variant(1).name() == "None");
        assert!(!ty_id.variant(1).non_exhaustive());
        assert!(ty_id.fields(1) == 0);

        assert!(ty_id.variant(2).name() == "Foomp");
        assert!(ty_id.variant(2).non_exhaustive());
        assert!(ty_id.fields(2) == 2);
        assert!(ty_id.field(2, 0).type_id() == TypeId::of::<()>());
        assert!(ty_id.field(2, 1).type_id() == TypeId::of::<&str>());
        assert!(ty_id.field(2, 1).name() == "b");

        assert!(ty_id.generics().is_empty());
    }

    const {
        let ty_id = TypeId::of::<Option<i32>>();
        assert!(ty_id.variants() == 2);
        assert!(ty_id.generics().len() == 1);

        let Generic::Type(GenericType { ty: generic_ty, .. }) = ty_id.generics()[0] else {
            panic!()
        };
        assert!(generic_ty == TypeId::of::<i32>());

        let ty_id = TypeId::of::<Option<i32>>();
        assert!(ty_id.size() == Some(size_of::<Option<i32>>()));
        assert!(ty_id.variants() == 2);
        assert!(ty_id.fields(0) == 0);
        assert!(ty_id.fields(1) == 1);
        assert!(ty_id.field(1, 0).type_id() == TypeId::of::<i32>());
    }
}

#[test]
fn test_primitives() {
    use TypeKind::*;

    const {
        let Type { kind: Bool, .. } = (const { Type::of::<bool>() }) else { panic!() };
        let ty_id = TypeId::of::<bool>();
        assert!(ty_id.size() == Some(size_of::<bool>()));
        assert!(ty_id.variants() == 1);

        let Type { kind: Char, .. } = (const { Type::of::<char>() }) else { panic!() };
        let ty_id = TypeId::of::<char>();
        assert!(ty_id.size() == Some(size_of::<char>()));
        assert!(ty_id.variants() == 1);

        let Type { kind: Int, .. } = (const { Type::of::<i32>() }) else { panic!() };
        let ty_id = TypeId::of::<i32>();
        assert!(ty_id.is_signed());
        assert!(ty_id.size() == Some(size_of::<i32>()));
        assert!(ty_id.variants() == 1);

        let Type { kind: Int, .. } = (const { Type::of::<isize>() }) else { panic!() };
        let ty_id = TypeId::of::<isize>();
        assert!(ty_id.is_signed());
        assert!(ty_id.size() == Some(size_of::<isize>()));
        assert!(ty_id.variants() == 1);

        let Type { kind: Int, .. } = (const { Type::of::<u32>() }) else { panic!() };
        let ty_id = TypeId::of::<u32>();
        assert!(!ty_id.is_signed());
        assert!(ty_id.size() == Some(size_of::<u32>()));
        assert!(ty_id.variants() == 1);

        let Type { kind: Int, .. } = (const { Type::of::<usize>() }) else { panic!() };
        let ty_id = TypeId::of::<usize>();
        assert!(!ty_id.is_signed());
        assert!(ty_id.size() == Some(size_of::<usize>()));
        assert!(ty_id.variants() == 1);

        let Type { kind: Float, .. } = (const { Type::of::<f32>() }) else { panic!() };
        let ty_id = TypeId::of::<f32>();
        assert!(ty_id.size() == Some(size_of::<f32>()));
        assert!(ty_id.variants() == 1);

        let Type { kind: Str(_ty), .. } = (const { Type::of::<str>() }) else { panic!() };
        let ty_id = TypeId::of::<str>();
        assert!(ty_id.size() == None);
        assert!(ty_id.variants() == 1);
    }
}

#[test]
fn test_references() {
    use TypeKind::Reference;

    // Immutable reference.
    let Type { kind: Reference, .. } = Type::of::<&u8>() else { panic!() };
    const {
        let ty = TypeId::of::<&u8>();
        assert!(ty.points_to() == Some(TypeId::of::<u8>()));
        assert!(!ty.points_mutably());
    }

    // Mutable references.
    let Type { kind: Reference, .. } = Type::of::<&mut u64>() else { panic!() };
    const {
        let ty = TypeId::of::<&mut u64>();
        assert!(ty.points_to() == Some(TypeId::of::<u64>()));
        assert!(ty.points_mutably());
    }

    // Wide references.
    let Type { kind: Reference, .. } = Type::of::<&dyn Any>() else { panic!() };
    const {
        let ty = TypeId::of::<&dyn Any>();
        assert!(ty.points_to() == Some(TypeId::of::<dyn Any>()));
        assert!(!ty.points_mutably());
    }
}

#[test]
fn test_pointers() {
    use TypeKind::Pointer;

    // Immutable pointer.
    let Type { kind: Pointer, .. } = Type::of::<*const u8>() else { panic!() };
    const {
        let ty = TypeId::of::<*const u8>();
        assert!(ty.points_to() == Some(TypeId::of::<u8>()));
        assert!(!ty.points_mutably());
    }

    // Mutable pointer.
    let Type { kind: Pointer, .. } = Type::of::<*mut u64>() else { panic!() };
    const {
        let ty = TypeId::of::<*mut u64>();
        assert!(ty.points_to() == Some(TypeId::of::<u64>()));
        assert!(ty.points_mutably());
    }

    // Wide pointer.
    let Type { kind: Pointer, .. } = Type::of::<*const dyn Any>() else { panic!() };
    const {
        let ty = TypeId::of::<*const dyn Any>();
        assert!(ty.points_to() == Some(TypeId::of::<dyn Any>()));
        assert!(!ty.points_mutably());
    }
}

#[test]
fn test_dynamic_traits() {
    use std::collections::HashSet;
    use std::mem::type_info::DynTraitPredicate;
    trait A<T> {}

    trait B<const CONST_NUM: i32> {
        type Foo;
    }

    trait FooTrait<'a, 'b, const CONST_NUM: i32> {}

    trait ProjectorTrait<'a, 'b> {}

    fn preds_of<T: ?Sized + 'static>() -> &'static [DynTraitPredicate] {
        match const { Type::of::<T>() }.kind {
            TypeKind::DynTrait(d) => d.predicates,
            _ => unreachable!(),
        }
    }

    fn pred<'a>(preds: &'a [DynTraitPredicate], want: TypeId) -> &'a DynTraitPredicate {
        preds
            .iter()
            .find(|p| p.trait_ty.ty == want)
            .unwrap_or_else(|| panic!("missing predicate for {want:?}"))
    }

    fn assert_typeid_set_eq(actual: &[TypeId], expected: &[TypeId]) {
        let actual_set: HashSet<TypeId> = actual.iter().copied().collect();
        let expected_set: HashSet<TypeId> = expected.iter().copied().collect();
        assert_eq!(actual.len(), actual_set.len(), "duplicates present: {actual:?}");
        assert_eq!(
            actual_set, expected_set,
            "unexpected ids.\nactual: {actual:?}\nexpected: {expected:?}"
        );
    }

    fn assert_predicates_exact(preds: &[DynTraitPredicate], expected_pred_ids: &[TypeId]) {
        let actual_pred_ids: Vec<TypeId> = preds.iter().map(|p| p.trait_ty.ty).collect();
        assert_typeid_set_eq(&actual_pred_ids, expected_pred_ids);
    }

    // dyn Send
    {
        let preds = preds_of::<dyn Send>();
        assert_predicates_exact(preds, &[TypeId::of::<dyn Send>()]);

        let p = pred(preds, TypeId::of::<dyn Send>());
        assert!(p.trait_ty.is_auto);
    }

    // dyn A<i32>
    {
        let preds = preds_of::<dyn A<i32>>();
        assert_predicates_exact(preds, &[TypeId::of::<dyn A<i32>>()]);

        let p = pred(preds, TypeId::of::<dyn A<i32>>());
        assert!(!p.trait_ty.is_auto);
    }

    // dyn B<5, Foo = i32>
    {
        let preds = preds_of::<dyn B<5, Foo = i32>>();
        assert_predicates_exact(preds, &[TypeId::of::<dyn B<5, Foo = i32>>()]);

        let e = pred(preds, TypeId::of::<dyn B<5, Foo = i32>>());
        assert!(!e.trait_ty.is_auto);
    }

    // dyn for<'a> FooTrait<'a, 'a, 7>
    {
        let preds = preds_of::<dyn for<'a> FooTrait<'a, 'a, 7>>();
        assert_predicates_exact(preds, &[TypeId::of::<dyn for<'a> FooTrait<'a, 'a, 7>>()]);

        let foo = pred(preds, TypeId::of::<dyn for<'a> FooTrait<'a, 'a, 7>>());
        assert!(!foo.trait_ty.is_auto);
    }

    // dyn FooTrait<'static, 'static, 7>
    {
        let preds = preds_of::<dyn FooTrait<'static, 'static, 7>>();
        assert_predicates_exact(preds, &[TypeId::of::<dyn FooTrait<'static, 'static, 7>>()]);

        let foo = pred(preds, TypeId::of::<dyn FooTrait<'static, 'static, 7>>());
        assert!(!foo.trait_ty.is_auto);
    }

    // dyn for<'a, 'b> FooTrait<'a, 'b, 7>
    {
        let preds = preds_of::<dyn for<'a, 'b> FooTrait<'a, 'b, 7>>();
        assert_predicates_exact(preds, &[TypeId::of::<dyn for<'a, 'b> FooTrait<'a, 'b, 7>>()]);

        let foo = pred(preds, TypeId::of::<dyn for<'a, 'b> FooTrait<'a, 'b, 7>>());
        assert!(!foo.trait_ty.is_auto);
    }

    // dyn for<'a, 'b> ProjectorTrait<'a, 'b>
    {
        let preds = preds_of::<dyn for<'a, 'b> ProjectorTrait<'a, 'b>>();
        assert_predicates_exact(preds, &[TypeId::of::<dyn for<'a, 'b> ProjectorTrait<'a, 'b>>()]);

        let proj = pred(preds, TypeId::of::<dyn for<'a, 'b> ProjectorTrait<'a, 'b>>());
        assert!(!proj.trait_ty.is_auto);
    }

    // dyn for<'a> FooTrait<'a, 'a, 7> + Send + Sync
    {
        let preds = preds_of::<dyn for<'a> FooTrait<'a, 'a, 7> + Send + Sync>();
        assert_predicates_exact(
            preds,
            &[
                TypeId::of::<dyn for<'a> FooTrait<'a, 'a, 7>>(),
                TypeId::of::<dyn Send>(),
                TypeId::of::<dyn Sync>(),
            ],
        );

        let foo = pred(preds, TypeId::of::<dyn for<'a> FooTrait<'a, 'a, 7>>());
        assert!(!foo.trait_ty.is_auto);

        let send = pred(preds, TypeId::of::<dyn Send>());
        assert!(send.trait_ty.is_auto);

        let sync = pred(preds, TypeId::of::<dyn Sync>());
        assert!(sync.trait_ty.is_auto);
    }
}

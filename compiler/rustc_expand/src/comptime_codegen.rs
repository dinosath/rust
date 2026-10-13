//! Internal AST materialization for compile-time item generation prototypes.

use rustc_ast::{self as ast, AttrVec, Item, ItemKind, ModKind};
use rustc_span::{Ident, Span};
use smallvec::SmallVec;
use rustc_span::Symbol;

use crate::expand::{AstFragment, MacroExpander};

/// A compiler-owned request for a function or static item.
///
/// Types and expressions are supplied as Rust AST nodes, never as source text.
/// This is an AST insertion primitive, not yet an API that comptime functions
/// can construct.
pub enum ItemRequest {
    Function {
        ident: Ident,
        visibility: ast::Visibility,
        signature: ast::FnSig,
        generics: ast::Generics,
        body: Box<ast::Block>,
        span: Span,
    },
    Static {
        ident: Ident,
        visibility: ast::Visibility,
        ty: Box<ast::Ty>,
        initializer: Box<ast::Expr>,
        safety: ast::Safety,
        mutability: ast::Mutability,
        span: Span,
    },
}

impl ItemRequest {
    fn into_item(self) -> Box<Item> {
        let (visibility, span, kind) = match self {
            Self::Function { ident, visibility, signature, generics, body, span } => (
                visibility,
                span,
                ItemKind::Fn(Box::new(ast::Fn {
                    defaultness: ast::Defaultness::Implicit,
                    ident,
                    generics,
                    sig: signature,
                    contract: None,
                    define_opaque: None,
                    body: Some(body),
                    eii_impl: None,
                })),
            ),
            Self::Static {
                ident,
                visibility,
                ty,
                initializer,
                safety,
                mutability,
                span,
            } => (
                visibility,
                span,
                ItemKind::Static(Box::new(ast::StaticItem {
                    ident,
                    ty,
                    safety,
                    mutability,
                    expr: Some(initializer),
                    define_opaque: None,
                    eii_impl: None,
                })),
            ),
        };

        Box::new(Item {
            attrs: AttrVec::new(),
            id: ast::DUMMY_NODE_ID,
            kind,
            vis: visibility,
            span,
            tokens: None,
        })
    }
}

/// Assigns node IDs and expands a sequence of generated item requests using
/// the active monotonic expander. The returned items are ready to be inserted
/// into their containing module before name resolution.
pub fn expand_item_requests(
    expander: &mut MacroExpander<'_, '_>,
    requests: impl IntoIterator<Item = ItemRequest>,
) -> SmallVec<[Box<Item>; 1]> {
    let items = requests.into_iter().map(ItemRequest::into_item).collect();
    expander.fully_expand_fragment(AstFragment::Items(items)).make_items()
}

/// Inserts generated items into a loaded module. An empty path selects the
/// crate root; non-empty paths select nested inline or out-of-line modules.
pub fn place_generated_items(
    krate: &mut ast::Crate,
    module_path: &[Symbol],
    items: SmallVec<[Box<Item>; 1]>,
) -> Result<(), ModuleNotFound> {
    let mut target = &mut krate.items;
    for segment in module_path {
        let Some(item) = target.iter_mut().find(|item| {
            item.kind.ident().is_some_and(|ident| ident.name == *segment)
        }) else {
            return Err(ModuleNotFound);
        };
        let ItemKind::Mod(_, _, ModKind::Loaded(items, _, _)) = &mut item.kind else {
            return Err(ModuleNotFound);
        };
        target = items;
    }
    target.extend(items);
    Ok(())
}

#[derive(Debug, PartialEq, Eq)]
pub struct ModuleNotFound;

#[cfg(test)]
mod tests {
    use rustc_ast::{BlockCheckMode, FnDecl, FnHeader, FnRetTy, Generics, Inline};
    use rustc_span::{DUMMY_SP, Ident};
    use thin_vec::ThinVec;

    use super::*;

    fn public_visibility() -> ast::Visibility {
        ast::Visibility { kind: ast::VisibilityKind::Public, span: DUMMY_SP }
    }

    fn generated_static(name: &str) -> Box<Item> {
        ItemRequest::Static {
            ident: Ident::from_str_and_span(name, DUMMY_SP),
            visibility: public_visibility(),
            ty: Box::new(ast::Ty {
                id: ast::DUMMY_NODE_ID,
                kind: ast::TyKind::Infer,
                span: DUMMY_SP,
            }),
            initializer: Box::new(ast::Expr::dummy()),
            safety: ast::Safety::Default,
            mutability: ast::Mutability::Not,
            span: DUMMY_SP,
        }
        .into_item()
    }

    fn module(name: &str, inline: Inline) -> Box<Item> {
        Box::new(Item {
            attrs: AttrVec::new(),
            id: ast::DUMMY_NODE_ID,
            kind: ItemKind::Mod(
                ast::Safety::Default,
                Ident::from_str_and_span(name, DUMMY_SP),
                ModKind::Loaded(ThinVec::new(), inline, ast::ModSpans::default()),
            ),
            vis: ast::Visibility { kind: ast::VisibilityKind::Inherited, span: DUMMY_SP },
            span: DUMMY_SP,
            tokens: None,
        })
    }

    #[test]
    fn materializes_function_and_static_requests() {
        rustc_span::create_session_if_not_set_then(
            rustc_span::edition::DEFAULT_EDITION,
            |_| {
                let function = ItemRequest::Function {
                    ident: Ident::from_str_and_span("generated_function", DUMMY_SP),
                    visibility: public_visibility(),
                    signature: ast::FnSig {
                        header: FnHeader::default(),
                        decl: Box::new(FnDecl {
                            inputs: ThinVec::new(),
                            output: FnRetTy::Default(DUMMY_SP),
                        }),
                        span: DUMMY_SP,
                    },
                    generics: Generics::default(),
                    body: Box::new(ast::Block {
                        stmts: ThinVec::new(),
                        id: ast::DUMMY_NODE_ID,
                        rules: BlockCheckMode::Default,
                        span: DUMMY_SP,
                    }),
                    span: DUMMY_SP,
                }
                .into_item();
                let static_item = ItemRequest::Static {
                    ident: Ident::from_str_and_span("GENERATED_STATIC", DUMMY_SP),
                    visibility: public_visibility(),
                    ty: Box::new(ast::Ty {
                        id: ast::DUMMY_NODE_ID,
                        kind: ast::TyKind::Infer,
                        span: DUMMY_SP,
                    }),
                    initializer: Box::new(ast::Expr::dummy()),
                    safety: ast::Safety::Default,
                    mutability: ast::Mutability::Not,
                    span: DUMMY_SP,
                }
                .into_item();

                assert!(matches!(function.kind, ItemKind::Fn(_)));
                assert_eq!(function.kind.ident().unwrap().name.as_str(), "generated_function");
                assert!(function.vis.kind.is_pub());
                assert!(matches!(static_item.kind, ItemKind::Static(_)));
                assert_eq!(static_item.kind.ident().unwrap().name.as_str(), "GENERATED_STATIC");
                assert!(static_item.vis.kind.is_pub());
            },
        );
    }

    #[test]
    fn places_items_in_root_inline_and_out_of_line_modules() {
        rustc_span::create_session_if_not_set_then(
            rustc_span::edition::DEFAULT_EDITION,
            |_| {
                let mut krate = ast::Crate {
                    id: ast::CRATE_NODE_ID,
                    attrs: AttrVec::new(),
                    items: [
                        module("inline_module", Inline::Yes),
                        module("out_of_line_module", Inline::No { had_parse_error: Ok(()) }),
                    ]
                    .into_iter()
                    .collect(),
                    spans: ast::ModSpans::default(),
                    is_placeholder: false,
                };
                let inline_module = krate.items[0].kind.ident().unwrap().name;
                let out_of_line_module = krate.items[1].kind.ident().unwrap().name;

                place_generated_items(
                    &mut krate,
                    &[],
                    smallvec::smallvec![generated_static("ROOT_ITEM")],
                )
                .unwrap();
                place_generated_items(
                    &mut krate,
                    &[inline_module],
                    smallvec::smallvec![generated_static("INLINE_ITEM")],
                )
                .unwrap();
                place_generated_items(
                    &mut krate,
                    &[out_of_line_module],
                    smallvec::smallvec![generated_static("OUT_OF_LINE_ITEM")],
                )
                .unwrap();

                assert_eq!(krate.items.len(), 3);
                let ItemKind::Mod(_, _, ModKind::Loaded(inline_items, _, _)) =
                    &krate.items[0].kind
                else {
                    panic!("expected inline module");
                };
                let ItemKind::Mod(_, _, ModKind::Loaded(out_of_line_items, _, _)) =
                    &krate.items[1].kind
                else {
                    panic!("expected out-of-line module");
                };
                assert_eq!(inline_items.len(), 1);
                assert_eq!(out_of_line_items.len(), 1);
            },
        );
    }
}
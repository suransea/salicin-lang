use crate::ast::{CallArg, Expr, PassMode};

use super::flow::{LoanKind, LowerCtx};
use super::hir::{
    AccessKind, HirArgument, HirExpr, HirExprKind, LayoutQueryKind, LocalCapability, ParamSig, Ty,
};
use super::lower::error_expr;
use super::Analyzer;

impl Analyzer {
    pub(super) fn lower_layout_query(
        &mut self,
        name: &str,
        groups: &[&[CallArg]],
        context: &LowerCtx,
    ) -> HirExpr {
        let [group] = groups else {
            self.error(format!("`{name}` expects exactly one type argument group"));
            return error_expr();
        };
        let Some(queried) = self.explicit_raw_pointee(name, group, context) else {
            return error_expr();
        };
        if matches!(queried, Ty::Function(_) | Ty::Error) {
            self.error(format!("`{name}` cannot query layout of `{queried}`"));
            return error_expr();
        }
        HirExpr {
            ty: Ty::U64,
            kind: HirExprKind::LayoutQuery {
                queried,
                kind: if name == "size_of" {
                    LayoutQueryKind::Size
                } else {
                    LayoutQueryKind::Align
                },
            },
        }
    }

    fn explicit_raw_pointee(
        &mut self,
        owner: &str,
        group: &[CallArg],
        context: &LowerCtx,
    ) -> Option<Ty> {
        if group.len() != 1 {
            self.error(format!(
                "compile-time argument group of `{owner}` expects exactly one type"
            ));
            return None;
        }
        if group[0].label.as_deref().is_some_and(|label| label != "T") {
            self.error(format!(
                "unknown compile-time parameter `{}` in `{owner}`; expected `T`",
                group[0].label.as_deref().unwrap_or_default()
            ));
            return None;
        }
        let source = self.type_argument_from_expr(&group[0].value, &context.type_substitutions)?;
        Some(self.lower_source_type(&source))
    }

    pub(super) fn lower_raw_alloc(
        &mut self,
        groups: &[&[CallArg]],
        expected: Option<&Ty>,
        context: &mut LowerCtx,
    ) -> HirExpr {
        if context.unsafe_depth == 0 {
            self.error("`raw_alloc` requires an `unsafe` block");
            return error_expr();
        }
        let (pointee, runtime) = match groups {
            [runtime] => {
                let Some(Ty::Pointer {
                    pointee,
                    mutable: true,
                }) = expected
                else {
                    self.error(
                        "cannot infer `raw_alloc` pointee type; use `raw_alloc(T)(size, align)` or provide an expected `MutPtr(T)` type",
                    );
                    return error_expr();
                };
                ((**pointee).clone(), *runtime)
            }
            [compile, runtime] => {
                let Some(pointee) = self.explicit_raw_pointee("raw_alloc", compile, context) else {
                    return error_expr();
                };
                (pointee, *runtime)
            }
            _ => {
                self.error(
                    "`raw_alloc` expects one runtime group and at most one compile-time type group",
                );
                return error_expr();
            }
        };
        let names = ["size".to_owned(), "align".to_owned()];
        let Some(arguments) = self.ordered_call_arguments("raw_alloc", 1, runtime, &names) else {
            return error_expr();
        };
        let size = self.lower_expr(&arguments[0].value, Some(&Ty::U64), context);
        let align = self.lower_expr(&arguments[1].value, Some(&Ty::U64), context);
        HirExpr {
            ty: Ty::Pointer {
                pointee: Box::new(pointee),
                mutable: true,
            },
            kind: HirExprKind::RawAlloc {
                size: Box::new(size),
                align: Box::new(align),
            },
        }
    }

    pub(super) fn lower_raw_dealloc(
        &mut self,
        groups: &[&[CallArg]],
        context: &mut LowerCtx,
    ) -> HirExpr {
        if context.unsafe_depth == 0 {
            self.error("`raw_dealloc` requires an `unsafe` block");
            return error_expr();
        }
        let (explicit, runtime) = match groups {
            [runtime] => (None, *runtime),
            [compile, runtime] => {
                let Some(pointee) = self.explicit_raw_pointee("raw_dealloc", compile, context)
                else {
                    return error_expr();
                };
                (Some(pointee), *runtime)
            }
            _ => {
                self.error(
                    "`raw_dealloc` expects one runtime group and at most one compile-time type group",
                );
                return error_expr();
            }
        };
        let names = ["pointer".to_owned(), "size".to_owned(), "align".to_owned()];
        let Some(arguments) = self.ordered_call_arguments("raw_dealloc", 1, runtime, &names) else {
            return error_expr();
        };
        let pointer_expected = explicit.as_ref().map(|pointee| Ty::Pointer {
            pointee: Box::new(pointee.clone()),
            mutable: true,
        });
        let pointer = self.lower_expr(&arguments[0].value, pointer_expected.as_ref(), context);
        let Ty::Pointer {
            pointee,
            mutable: true,
        } = &pointer.ty
        else {
            self.error(format!(
                "`raw_dealloc` requires a `MutPtr(T)`, found `{}`",
                pointer.ty
            ));
            return error_expr();
        };
        if let Some(explicit) = &explicit {
            if explicit != pointee.as_ref() {
                self.error(format!(
                    "`raw_dealloc` explicit pointee `{explicit}` does not match pointer type `{}`",
                    pointer.ty
                ));
                return error_expr();
            }
        }
        let size = self.lower_expr(&arguments[1].value, Some(&Ty::U64), context);
        let align = self.lower_expr(&arguments[2].value, Some(&Ty::U64), context);
        HirExpr {
            ty: Ty::Unit,
            kind: HirExprKind::RawDealloc {
                pointer: Box::new(pointer),
                size: Box::new(size),
                align: Box::new(align),
            },
        }
    }

    pub(super) fn lower_raw_init(
        &mut self,
        groups: &[&[CallArg]],
        context: &mut LowerCtx,
    ) -> HirExpr {
        if context.unsafe_depth == 0 {
            self.error("`raw_init` requires an `unsafe` block");
            return error_expr();
        }
        let [runtime] = groups else {
            self.error("`raw_init` expects exactly one runtime argument group");
            return error_expr();
        };
        let names = ["pointer".to_owned(), "value".to_owned()];
        let Some(arguments) = self.ordered_call_arguments("raw_init", 1, runtime, &names) else {
            return error_expr();
        };
        let pointer = self.lower_expr(&arguments[0].value, None, context);
        let Ty::Pointer {
            pointee,
            mutable: true,
        } = &pointer.ty
        else {
            self.error(format!(
                "`raw_init` requires a `MutPtr(T)`, found `{}`",
                pointer.ty
            ));
            return error_expr();
        };
        let pointee = (**pointee).clone();
        let mut temporary_loans = Vec::new();
        let mut temporary_bindings = Vec::new();
        let value = self.lower_call_argument(
            &arguments[1].value,
            &ParamSig {
                name: "value".to_owned(),
                ty: pointee,
                mode: PassMode::Move,
            },
            context,
            &mut temporary_loans,
            &mut temporary_bindings,
        );
        debug_assert!(temporary_bindings.is_empty());
        self.release_loans(&temporary_loans, context);
        let HirArgument::Move(value) = value else {
            unreachable!("an explicit move parameter lowers to a move argument")
        };
        HirExpr {
            ty: Ty::Unit,
            kind: HirExprKind::RawInit {
                pointer: Box::new(pointer),
                value: Box::new(value),
            },
        }
    }

    pub(super) fn lower_raw_take(
        &mut self,
        groups: &[&[CallArg]],
        context: &mut LowerCtx,
    ) -> HirExpr {
        if context.unsafe_depth == 0 {
            self.error("`raw_take` requires an `unsafe` block");
            return error_expr();
        }
        let [runtime] = groups else {
            self.error("`raw_take` expects exactly one runtime argument group");
            return error_expr();
        };
        let names = ["pointer".to_owned()];
        let Some(arguments) = self.ordered_call_arguments("raw_take", 1, runtime, &names) else {
            return error_expr();
        };
        let pointer = self.lower_expr(&arguments[0].value, None, context);
        let Ty::Pointer {
            pointee,
            mutable: true,
        } = &pointer.ty
        else {
            self.error(format!(
                "`raw_take` requires a `MutPtr(T)`, found `{}`",
                pointer.ty
            ));
            return error_expr();
        };
        if matches!(pointee.as_ref(), Ty::Never | Ty::Function(_) | Ty::Error) {
            self.error(format!(
                "`raw_take` cannot move a value without a concrete data representation: `{}`",
                self.diagnostic_type_name(pointee)
            ));
            return error_expr();
        }
        HirExpr {
            ty: (**pointee).clone(),
            kind: HirExprKind::RawTake(Box::new(pointer)),
        }
    }

    pub(super) fn lower_raw_offset(
        &mut self,
        groups: &[&[CallArg]],
        context: &mut LowerCtx,
    ) -> HirExpr {
        if context.unsafe_depth == 0 {
            self.error("`raw_offset` requires an `unsafe` block");
            return error_expr();
        }
        let [runtime] = groups else {
            self.error("`raw_offset` expects exactly one runtime argument group");
            return error_expr();
        };
        let names = ["pointer".to_owned(), "index".to_owned()];
        let Some(arguments) = self.ordered_call_arguments("raw_offset", 1, runtime, &names) else {
            return error_expr();
        };
        let pointer = self.lower_expr(&arguments[0].value, None, context);
        let Ty::Pointer { pointee, .. } = &pointer.ty else {
            self.error(format!(
                "`raw_offset` requires `Ptr(T)` or `MutPtr(T)`, found `{}`",
                pointer.ty
            ));
            return error_expr();
        };
        if matches!(pointee.as_ref(), Ty::Never | Ty::Function(_) | Ty::Error) {
            self.error(format!(
                "`raw_offset` cannot index a pointee without a concrete data representation: `{}`",
                self.diagnostic_type_name(pointee)
            ));
            return error_expr();
        }
        let index = self.lower_expr(&arguments[1].value, Some(&Ty::U64), context);
        HirExpr {
            ty: pointer.ty.clone(),
            kind: HirExprKind::RawOffset {
                pointer: Box::new(pointer),
                index: Box::new(index),
            },
        }
    }

    pub(super) fn lower_raw_borrow(
        &mut self,
        name: &str,
        groups: &[&[CallArg]],
        expected: Option<&Ty>,
        context: &mut LowerCtx,
    ) -> HirExpr {
        if context.unsafe_depth == 0 {
            self.error(format!("`{name}` requires an `unsafe` block"));
            return error_expr();
        }
        let (required_mutable, runtime) = match groups {
            [runtime] => (false, *runtime),
            [access, runtime] => {
                let [argument] = *access else {
                    self.error("`raw_borrow` access group expects exactly one argument");
                    return error_expr();
                };
                let mutable = match &argument.value {
                    Expr::Name(value) if value == "shared" => false,
                    Expr::Name(value) if value == "mut" => true,
                    _ => {
                        self.error("`raw_borrow` access argument must be `shared` or `mut`");
                        return error_expr();
                    }
                };
                (mutable, *runtime)
            }
            _ => {
                self.error(format!(
                    "`{name}` expects one runtime group and at most one access group"
                ));
                return error_expr();
            }
        };
        let names = ["pointer".to_owned(), "anchor".to_owned()];
        let Some(arguments) = self.ordered_call_arguments(name, 1, runtime, &names) else {
            return error_expr();
        };
        let pointer = self.lower_expr(&arguments[0].value, None, context);
        let Ty::Pointer { pointee, mutable } = &pointer.ty else {
            self.error(format!(
                "`{name}` requires `Ptr(T)` or `MutPtr(T)`, found `{}`",
                pointer.ty
            ));
            return error_expr();
        };
        if required_mutable && !mutable {
            self.error("mutable `raw_borrow` requires a `MutPtr(T)`");
            return error_expr();
        }
        if matches!(pointee.as_ref(), Ty::Never | Ty::Function(_) | Ty::Error) {
            self.error(format!(
                "`{name}` cannot borrow a pointee without a concrete data representation: `{}`",
                self.diagnostic_type_name(pointee)
            ));
            return error_expr();
        }
        let Expr::Borrow {
            mutable: anchor_mutable,
            value: anchor_value,
            ..
        } = &arguments[1].value
        else {
            self.error(format!(
                "`{name}` requires an explicit `{}borrow` anchor",
                if required_mutable { "mut " } else { "" }
            ));
            return error_expr();
        };
        if *anchor_mutable != required_mutable {
            self.error(format!(
                "`{name}` requires a {}borrow anchor",
                if required_mutable {
                    "mutable "
                } else {
                    "shared "
                }
            ));
            return error_expr();
        }
        let Some(mut anchor) = self.lower_place(anchor_value, context) else {
            return error_expr();
        };
        if required_mutable {
            self.ensure_writable(&anchor);
        }
        let loan = self.acquire_loan(
            &anchor,
            if required_mutable {
                LoanKind::Mutable
            } else {
                LoanKind::Shared
            },
            true,
            context,
        );
        anchor.capability = if required_mutable {
            LocalCapability::MutParam
        } else {
            LocalCapability::SharedParam
        };
        anchor.loan = loan;
        let source_region = context
            .borrowed_parameter_regions
            .get(&anchor.local)
            .and_then(|(region, _)| region.clone());
        let expected_region = expected.and_then(|expected| match expected {
            Ty::Reference {
                pointee: expected_pointee,
                mutable: expected_mutable,
                region,
            } if expected_pointee.as_ref() == pointee.as_ref()
                && *expected_mutable == required_mutable =>
            {
                region.clone()
            }
            _ => None,
        });
        HirExpr {
            ty: Ty::Reference {
                pointee: pointee.clone(),
                mutable: required_mutable,
                region: expected_region.or(source_region),
            },
            kind: HirExprKind::RawBorrow {
                pointer: Box::new(pointer),
                anchor,
            },
        }
    }

    pub(super) fn lower_raw_trap(
        &mut self,
        groups: &[&[CallArg]],
        context: &mut LowerCtx,
    ) -> HirExpr {
        if context.unsafe_depth == 0 {
            self.error("`raw_trap` requires an `unsafe` block");
            return error_expr();
        }
        if !matches!(groups, [arguments] if arguments.is_empty()) {
            self.error("`raw_trap` expects one empty runtime argument group");
            return error_expr();
        }
        HirExpr {
            ty: Ty::Never,
            kind: HirExprKind::RawTrap,
        }
    }

    pub(super) fn lower_forget(
        &mut self,
        groups: &[&[CallArg]],
        context: &mut LowerCtx,
    ) -> HirExpr {
        let [runtime] = groups else {
            self.error("`forget` expects exactly one runtime argument group");
            return error_expr();
        };
        let names = ["value".to_owned()];
        let Some(arguments) = self.ordered_call_arguments("forget", 1, runtime, &names) else {
            return error_expr();
        };
        let value = if let Some(place) =
            self.lower_place_without_diagnostic(&arguments[0].value, context)
        {
            self.access_place(place, AccessKind::Move, context)
        } else {
            self.lower_expr(&arguments[0].value, None, context)
        };
        HirExpr {
            ty: Ty::Unit,
            kind: HirExprKind::Forget(Box::new(value)),
        }
    }

    pub(super) fn lower_raw_pointer_constructor(
        &mut self,
        name: &str,
        groups: &[&[CallArg]],
        context: &mut LowerCtx,
    ) -> HirExpr {
        if groups.len() != 1 || groups[0].len() != 1 {
            self.error(format!(
                "`{name}` expects exactly one argument: `{name}({}borrow place)`",
                if name == "MutPtr" { "mut " } else { "" }
            ));
            return error_expr();
        }
        let argument = &groups[0][0];
        if argument.label.is_some() {
            self.error(format!("`{name}` does not accept a named argument"));
            return error_expr();
        }
        let required_mutable = name == "MutPtr";
        let Expr::Borrow { mutable, value, .. } = &argument.value else {
            self.error(format!(
                "`{name}` requires an explicit `{}borrow` argument",
                if required_mutable { "mut " } else { "" }
            ));
            return error_expr();
        };
        if *mutable != required_mutable {
            self.error(format!(
                "`{name}` requires `{}` borrowing",
                if required_mutable {
                    "borrow(mut)"
                } else {
                    "borrow"
                }
            ));
            return error_expr();
        }
        let Some(mut place) = self.lower_place(value, context) else {
            return error_expr();
        };
        if required_mutable {
            self.ensure_writable(&place);
        }
        let loan = self.acquire_loan(
            &place,
            if required_mutable {
                LoanKind::Mutable
            } else {
                LoanKind::Shared
            },
            true,
            context,
        );
        place.capability = if required_mutable {
            LocalCapability::MutParam
        } else {
            LocalCapability::SharedParam
        };
        place.loan = loan;
        HirExpr {
            ty: Ty::Pointer {
                pointee: Box::new(place.ty.clone()),
                mutable: required_mutable,
            },
            kind: HirExprKind::RawAddress { place },
        }
    }
}

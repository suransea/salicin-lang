use std::collections::{HashMap, HashSet};

use crate::ast::{CallArg, Expr, MatchArm, PassMode, Pattern, Stmt, Type, UnaryOp};
use crate::core::LangItemKind;

use super::defer;
use super::flow::{
    FlowState, InitializationStatus, InspectionBinding, LoanKind, LocalInfo, LowerCtx,
    RecursiveFrameCall,
};
use super::handlers::{collect_internal_recursion_tokens, is_internal_handler_closure_binding};
use super::hir::{
    AccessKind, AssignmentKind, CallableCaptureTy, CallableKind, CallableTy, ClosureCapture,
    ClosureCaptureMode, ClosureCapturePolicy, ClosureCaptureUse, ClosureEffectContext, ClosureInfo,
    ForwardedClosureCapture, FunctionTy, HirBinding, HirExpr, HirExprKind, HirFunction, HirParam,
    HirPlace, HirStmt, LocalCapability, ParamSig, PartialInfo, Ty,
};
use super::lower::{
    closure_info_for_callable, display_region, error_expr, flatten_call, integer_fits,
    integer_literal_bits, integer_literal_value, is_unconstrained_integer, negative_integer_fits,
    partial_info_for_callable, place_root_name, record_closure_capture,
    reference_value_types_compatible, TypeProbe,
};
use super::operators::unary_operator_trait;
use super::source_rewrite::collect_pattern_binding_names;
use super::Analyzer;

impl Analyzer {
    pub(super) fn lower_expr(
        &mut self,
        mut expression: &Expr,
        expected: Option<&Ty>,
        context: &mut LowerCtx,
    ) -> HirExpr {
        let previous = self.current_origin.clone();
        let mut located = false;
        while let Expr::Located {
            line,
            column,
            end_line,
            end_column,
            value,
        } = expression
        {
            located = true;
            if let Some(origin) = &mut self.current_origin {
                if let Some(source) = &mut origin.source {
                    source.line = *line;
                    source.column = *column;
                    source.end_line = *end_line;
                    source.end_column = *end_column;
                }
            }
            expression = value;
        }
        let lowered = self.lower_expr_unlocated(expression, expected, context);
        if located {
            self.current_origin = previous;
        }
        lowered
    }

    pub(super) fn lower_expr_unlocated(
        &mut self,
        expression: &Expr,
        expected: Option<&Ty>,
        context: &mut LowerCtx,
    ) -> HirExpr {
        let lowered = match expression {
            Expr::Located { .. } => unreachable!("source locations are lowered transparently"),
            Expr::Type(_) => {
                self.error("compile-time type expression cannot be used as a runtime value");
                error_expr()
            }
            Expr::Integer(value) => {
                let ty = match expected {
                    Some(ty) if ty.is_integer() => ty.clone(),
                    Some(Ty::Error) => Ty::Error,
                    Some(ty) => {
                        let ty = self.diagnostic_type_name(ty);
                        self.error(format!(
                            "integer literal cannot be used where `{ty}` is expected"
                        ));
                        Ty::Error
                    }
                    None => Ty::I32,
                };
                if ty.is_integer() && !integer_fits(*value, &ty) {
                    self.error(format!("integer literal `{value}` does not fit in `{ty}`"));
                }
                HirExpr {
                    ty,
                    kind: HirExprKind::Integer(integer_literal_bits(*value)),
                }
            }
            Expr::Bool(value) => HirExpr {
                ty: Ty::Bool,
                kind: HirExprKind::Bool(*value),
            },
            Expr::String(value) => {
                let Some(default_ty) = self.string_ty() else {
                    self.error("the core `string` type is unavailable");
                    return error_expr();
                };
                let ty = expected
                    .filter(|expected| **expected != Ty::Error)
                    .cloned()
                    .unwrap_or_else(|| default_ty.clone());
                if ty == default_ty {
                    let backing = Ty::Array(Box::new(Ty::U8), value.len() as u64);
                    self.require_literal_protocol_impl(
                        "core::literal::string_literal",
                        "from_string_literal",
                        &backing,
                        &ty,
                    );
                    self.lowering.string_literals.insert(value.clone());
                    return HirExpr {
                        ty,
                        kind: HirExprKind::String(value.clone()),
                    };
                }
                let bytes = value
                    .as_bytes()
                    .iter()
                    .map(|byte| HirExpr {
                        ty: Ty::U8,
                        kind: HirExprKind::Integer(i128::from(*byte)),
                    })
                    .collect::<Vec<_>>();
                let backing_ty = Ty::Array(Box::new(Ty::U8), bytes.len() as u64);
                self.lowering.array_types.insert(backing_ty.clone());
                self.ensure_array_trait_extensions(&backing_ty);
                let backing = HirExpr {
                    ty: backing_ty,
                    kind: HirExprKind::Array(bytes),
                };
                if ty == backing.ty {
                    self.require_literal_protocol_impl(
                        "core::literal::string_literal",
                        "from_string_literal",
                        &backing.ty,
                        &backing.ty,
                    );
                    return backing;
                }
                self.lower_literal_protocol_call(
                    "core::literal::string_literal",
                    "from_string_literal",
                    backing,
                    &ty,
                    context,
                )
            }
            Expr::Unit => HirExpr {
                ty: Ty::Unit,
                kind: HirExprKind::Unit,
            },
            Expr::Tuple(fields) => {
                let expected_fields = match expected {
                    Some(Ty::Tuple(expected_fields)) if expected_fields.len() == fields.len() => {
                        Some(expected_fields.as_slice())
                    }
                    Some(Ty::Error) | None => None,
                    Some(expected) => {
                        self.error(format!(
                            "tuple literal of length {} cannot be used where `{}` is expected",
                            fields.len(),
                            self.diagnostic_type_name(expected)
                        ));
                        None
                    }
                };
                let lowered = fields
                    .iter()
                    .enumerate()
                    .map(|(index, field)| {
                        self.lower_expr(
                            field,
                            expected_fields.and_then(|expected| expected.get(index)),
                            context,
                        )
                    })
                    .collect::<Vec<_>>();
                let ty = Ty::Tuple(lowered.iter().map(|field| field.ty.clone()).collect());
                self.lowering.tuple_types.insert(ty.clone());
                HirExpr {
                    ty,
                    kind: HirExprKind::Tuple(lowered),
                }
            }
            Expr::Array(elements) => self.lower_array_literal(elements, expected, context),
            Expr::Name(name) => {
                if let Some(local) = context.lookup(name).cloned() {
                    if matches!(expected, Some(Ty::Reference { .. }))
                        && matches!(local.ty, Ty::Reference { .. })
                    {
                        let place = HirPlace {
                            local: local.id,
                            root_ty: local.ty.clone(),
                            projections: Vec::new(),
                            dynamic_index: None,
                            ty: local.ty.clone(),
                            capability: LocalCapability::Owned,
                            root_mutable: local.mutable,
                            loan: None,
                            indirect: false,
                        };
                        let access = context.reference_value_access.unwrap_or(AccessKind::Auto);
                        self.access_place(place, access, context)
                    } else if local.partial.is_some() || local.closure.is_some() {
                        if matches!(&local.ty, Ty::Callable(callable) if callable.captures.iter().any(|capture| matches!(capture.mode, PassMode::Borrow | PassMode::MutBorrow)))
                        {
                            self.error(format!(
                                "local callable `{name}` cannot escape while it captures a borrow"
                            ));
                            error_expr()
                        } else {
                            let place = HirPlace {
                                local: local.id,
                                root_ty: local.ty.clone(),
                                projections: Vec::new(),
                                dynamic_index: None,
                                ty: local.ty.clone(),
                                capability: local.capability,
                                root_mutable: local.mutable,
                                loan: None,
                                indirect: false,
                            };
                            self.access_place(place, AccessKind::Auto, context)
                        }
                    } else {
                        let place = self
                            .lower_place(expression, context)
                            .expect("a resolved local name is a place");
                        self.access_place(place, AccessKind::Auto, context)
                    }
                } else if context.has_type_parameter(name) {
                    self.error(format!("type parameter `{name}` cannot be used as a value"));
                    error_expr()
                } else if name == "self" {
                    self.error("expression `self` is only available inside an extend member");
                    error_expr()
                } else if self.collection.globals.contains_key(name) {
                    HirExpr {
                        ty: self.global_type(name),
                        kind: HirExprKind::Global(name.clone()),
                    }
                } else if self.collection.function_overloads.contains_key(name) {
                    self.error(format!(
                        "overloaded function `{name}` must be selected by a call with named arguments"
                    ));
                    error_expr()
                } else if self.collection.functions.contains_key(name) {
                    HirExpr {
                        ty: self.function_type(name),
                        kind: HirExprKind::Function(name.clone()),
                    }
                } else if self.collection.function_templates.contains_key(name) {
                    self.error(format!(
                        "generic function `{name}` requires explicit type argument groups"
                    ));
                    error_expr()
                } else if let Some((enum_name, variant)) =
                    self.resolve_short_variant(name, expected, &context.origin)
                {
                    if self
                        .collection
                        .enum_layouts
                        .get(&enum_name)
                        .and_then(|layout| layout.variants.get(variant))
                        .is_some_and(|variant| variant.fields.is_empty())
                    {
                        HirExpr {
                            ty: Ty::Enum(enum_name.clone()),
                            kind: HirExprKind::ConstructEnum {
                                name: enum_name,
                                variant,
                                fields: Vec::new(),
                            },
                        }
                    } else {
                        self.error(format!("variant `{name}` requires constructor arguments"));
                        error_expr()
                    }
                } else {
                    self.error(format!("unknown name `{name}`"));
                    error_expr()
                }
            }
            Expr::Borrow { mutable, value, .. } => {
                if let Expr::Index { base, index } = value.as_ref() {
                    if !matches!(
                        self.probe_expr_ty(base, None, context),
                        TypeProbe::Known(Ty::Array(_, _))
                            | TypeProbe::KnownSource(Ty::Array(_, _), _)
                    ) {
                        return self.lower_protocol_index_reference(base, index, *mutable, context);
                    }
                }
                let Some(mut place) = self.lower_place(value, context) else {
                    return error_expr();
                };
                let returned_reference = expected.and_then(|expected| match expected {
                    Ty::Reference {
                        pointee,
                        mutable,
                        region,
                    } => Some(((**pointee).clone(), *mutable, region.clone())),
                    _ => None,
                });
                if let Some((pointee, expected_mutable, expected_region)) = &returned_reference {
                    let array_unsizes_to_slice = matches!(
                        (&place.ty, pointee),
                        (Ty::Array(actual, _), Ty::Slice(expected)) if actual == expected
                    );
                    if !array_unsizes_to_slice {
                        self.require_same_type(&place.ty, pointee, "returned borrow pointee");
                    }
                    if *expected_mutable && !*mutable {
                        self.error(if context.reference_value_depth > 0 {
                            "borrow kind mismatch: expected mutable borrow, found shared borrow"
                        } else {
                            "cannot return a shared borrow as a mutable borrow"
                        });
                    }
                    if context.reference_value_depth == 0 {
                        match context.borrowed_parameter_regions.get(&place.local) {
                            Some((source_region, source_mutable)) => {
                                if expected_region.is_some() && source_region != expected_region {
                                    self.error(format!(
                                        "returned borrow region mismatch: expected {}, found {}",
                                        display_region(expected_region.as_deref()),
                                        display_region(source_region.as_deref())
                                    ));
                                }
                                if *expected_mutable && !source_mutable {
                                    self.error(
                                        "cannot return a mutable borrow through a shared borrow parameter",
                                    );
                                }
                            }
                            None => self.error(
                                "cannot return a borrow of a local value; returned borrows must originate from a region-bound borrow parameter",
                            ),
                        }
                    }
                }
                if *mutable {
                    self.ensure_writable(&place);
                }
                let kind = if *mutable {
                    LoanKind::Mutable
                } else {
                    LoanKind::Shared
                };
                let loan = self.acquire_loan(&place, kind, true, context);
                place.capability = if *mutable {
                    LocalCapability::MutParam
                } else {
                    LocalCapability::SharedParam
                };
                place.loan = loan;
                HirExpr {
                    ty: returned_reference.map_or_else(
                        || place.ty.clone(),
                        |(pointee, mutable, region)| Ty::Reference {
                            pointee: Box::new(pointee),
                            mutable,
                            region,
                        },
                    ),
                    kind: HirExprKind::Borrow {
                        place,
                        mutable: *mutable,
                    },
                }
            }
            Expr::Unsafe(body) => {
                context.unsafe_depth += 1;
                let result = self.lower_expr(body, expected, context);
                context.unsafe_depth -= 1;
                result
            }
            Expr::Unary(operator, operand) => {
                if *operator == UnaryOp::Deref {
                    let pointer = self.lower_expr(operand, None, context);
                    if context.unsafe_depth == 0 {
                        self.error("raw pointer dereference requires an `unsafe` block");
                        return error_expr();
                    }
                    let Ty::Pointer { pointee, .. } = &pointer.ty else {
                        self.error(format!(
                            "unary `*` requires a raw pointer, found `{}`",
                            pointer.ty
                        ));
                        return error_expr();
                    };
                    let pointee = (**pointee).clone();
                    if !self.is_copy_type(&pointee) {
                        self.error(format!(
                            "raw pointer reads require a copyable pointee in the first version, found `{}`",
                            self.diagnostic_type_name(&pointee)
                        ));
                        return error_expr();
                    }
                    return HirExpr {
                        ty: pointee,
                        kind: HirExprKind::RawLoad(Box::new(pointer)),
                    };
                }
                if let Some(operator_trait) = unary_operator_trait(*operator) {
                    let operand_probe = self.probe_expr_ty(operand, None, context);
                    if let Some(receiver) = Self::nominal_ty_from_probe(&operand_probe) {
                        return self.lower_trait_unary(
                            operator_trait,
                            operand,
                            &receiver,
                            expected,
                            context,
                        );
                    }
                }
                if *operator == UnaryOp::Neg {
                    if let Expr::Integer(value) = operand.as_ref() {
                        let ty = match expected {
                            Some(ty) if ty.is_signed() => ty.clone(),
                            Some(Ty::Error) => Ty::Error,
                            Some(ty) => {
                                let ty = self.diagnostic_type_name(ty);
                                self.error(format!(
                                    "negative integer literal cannot be used where `{ty}` is expected"
                                ));
                                Ty::Error
                            }
                            None => Ty::I32,
                        };
                        if ty.is_signed() && !negative_integer_fits(*value, &ty) {
                            self.error(format!(
                                "negative integer literal `-{value}` does not fit in `{ty}`"
                            ));
                        }
                        let neg_name = self.lang_item_name(LangItemKind::Neg);
                        if ty != Ty::Error
                            && !self.collection.trait_impls.keys().any(|key| {
                                key.self_ty == ty
                                    && key.trait_ref.name == neg_name
                                    && key.trait_ref.arguments.is_empty()
                            })
                        {
                            self.error(format!(
                                "type `{}` does not implement `Neg` required by unary `-`",
                                self.diagnostic_type_name(&ty)
                            ));
                            return error_expr();
                        }
                        return HirExpr {
                            ty: ty.clone(),
                            kind: HirExprKind::Unary(
                                UnaryOp::Neg,
                                Box::new(HirExpr {
                                    ty,
                                    kind: HirExprKind::Integer(integer_literal_bits(*value)),
                                }),
                            ),
                        };
                    }
                }
                let operand_expected = match operator {
                    UnaryOp::Not => Some(Ty::Bool),
                    UnaryOp::Neg => expected.filter(|ty| ty.is_integer()).cloned(),
                    UnaryOp::Deref => unreachable!(),
                };
                let operand = self.lower_expr(operand, operand_expected.as_ref(), context);
                if let Some(operator_trait) = unary_operator_trait(*operator) {
                    let implemented = self.collection.trait_impls.keys().any(|key| {
                        key.self_ty == operand.ty
                            && key.trait_ref.name == self.lang_item_name(operator_trait.lang_item)
                            && key.trait_ref.arguments.is_empty()
                    });
                    if !implemented && operand.ty != Ty::Error {
                        self.error(format!(
                            "type `{}` does not implement `{}` required by unary operator",
                            self.diagnostic_type_name(&operand.ty),
                            operator_trait.lang_item.source_name(),
                        ));
                        return error_expr();
                    }
                }
                let ty = match operator {
                    UnaryOp::Not => {
                        self.require_same_type(&operand.ty, &Ty::Bool, "operand of `!`");
                        Ty::Bool
                    }
                    UnaryOp::Neg => {
                        if !operand.ty.is_integer() || !operand.ty.is_signed() {
                            self.error(format!(
                                "unary `-` requires a signed integer, found `{}`",
                                operand.ty
                            ));
                            Ty::Error
                        } else {
                            operand.ty.clone()
                        }
                    }
                    UnaryOp::Deref => unreachable!(),
                };
                HirExpr {
                    ty,
                    kind: HirExprKind::Unary(*operator, Box::new(operand)),
                }
            }
            Expr::Binary(left, operator, right) => {
                self.lower_binary(left, *operator, right, expected, context)
            }
            Expr::Coalesce(left, right) => self.lower_coalesce(left, right, expected, context),
            Expr::HandlerCoalesce {
                scrutinee,
                payload,
                success,
                fallback,
            } => self
                .lower_handler_coalesce(scrutinee, payload, success, fallback, expected, context),
            Expr::HandlerChainCall(chain) => self.lower_handler_chain_call(
                &chain.scrutinee,
                &chain.payload,
                &chain.error,
                &chain.member,
                &chain.groups,
                &chain.success,
                &chain.residual,
                expected,
                context,
            ),
            Expr::Try(value) => self.lower_try(value, expected, context),
            Expr::DoBlock { body } => self.lower_do_block(body, expected, context),
            Expr::Async { body } => self.lower_async_expression(body, context),
            Expr::Await(_) => {
                self.error(
                    "`await` in this position is not lowered yet; linear suspension and `if` or `match` branch suspension are supported, but loop suspension remains compiler work",
                );
                error_expr()
            }
            Expr::Throw(value) => self.lower_throw(value, context),
            Expr::Assign(place, value) => {
                if let Expr::Unary(UnaryOp::Deref, pointer) = place.as_ref() {
                    let pointer = self.lower_expr(pointer, None, context);
                    if context.unsafe_depth == 0 {
                        self.error("raw pointer assignment requires an `unsafe` block");
                        return error_expr();
                    }
                    let Ty::Pointer { pointee, mutable } = &pointer.ty else {
                        self.error(format!(
                            "raw pointer assignment requires `ptr(mut)(T)`, found `{}`",
                            pointer.ty
                        ));
                        return error_expr();
                    };
                    if !*mutable {
                        self.error("cannot assign through an immutable `ptr(T)`");
                        return error_expr();
                    }
                    let pointee = (**pointee).clone();
                    if !self.is_copy_type(&pointee) {
                        self.error(format!(
                            "raw pointer writes require a copyable pointee in the first version, found `{}`",
                            self.diagnostic_type_name(&pointee)
                        ));
                        return error_expr();
                    }
                    let value = self.lower_expr(value, Some(&pointee), context);
                    return HirExpr {
                        ty: Ty::Unit,
                        kind: HirExprKind::RawStore {
                            pointer: Box::new(pointer),
                            value: Box::new(value),
                        },
                    };
                }
                if let Expr::Index { base, index } = place.as_ref() {
                    if !matches!(
                        self.probe_expr_ty(base, None, context),
                        TypeProbe::Known(Ty::Array(_, _))
                            | TypeProbe::KnownSource(Ty::Array(_, _), _)
                    ) {
                        let loans = Self::loan_snapshot(context);
                        let reference =
                            self.lower_protocol_index_reference(base, index, true, context);
                        let Ty::Reference {
                            pointee,
                            mutable: true,
                            ..
                        } = &reference.ty
                        else {
                            self.release_loans_since(&loans, context);
                            return error_expr();
                        };
                        let value = self.lower_expr(value, Some(pointee), context);
                        self.release_loans_since(&loans, context);
                        return HirExpr {
                            ty: Ty::Unit,
                            kind: HirExprKind::ReferenceAssign {
                                reference: Box::new(reference),
                                value: Box::new(value),
                            },
                        };
                    }
                }
                let Some(place) = self.lower_place(place, context) else {
                    return error_expr();
                };
                self.ensure_writable(&place);
                self.ensure_no_conflicting_loan(&place, AccessKind::MutBorrow, context);
                // The right-hand side observes the pre-assignment state.  In
                // particular, `x = x` must not resurrect an unavailable `x`.
                let value = self.lower_expr(value, Some(&place.ty), context);
                let assignment = self.mark_initialized(&place, context);
                let mut root = place.clone();
                root.projections.clear();
                root.dynamic_index = None;
                root.ty = root.root_ty.clone();
                let root_initialized = context
                    .flow
                    .initialization_status(&self.place_leaf_keys(&root))
                    == InitializationStatus::Initialized;
                if assignment != AssignmentKind::Overwrite
                    && self.projected_place_crosses_custom_drop(&place)
                {
                    self.error(
                        "reinitializing a field through a type with custom droppable is not allowed because its destructor requires a complete value",
                    );
                }
                HirExpr {
                    ty: Ty::Unit,
                    kind: HirExprKind::Assign {
                        place,
                        value: Box::new(value),
                        assignment,
                        root_initialized,
                    },
                }
            }
            Expr::CompoundAssign(place, operator, value) => {
                self.lower_compound_assign(place, *operator, value, context)
            }
            Expr::Call(_, _) if defer::is_defer_call(expression) => {
                self.error("`defer` is only valid as a standalone statement in a lexical block");
                error_expr()
            }
            Expr::Call(_, _) if !self.lowering.deferred_handler_transforms.is_empty() => self
                .lower_deferred_handler_transform(expression, expected, context)
                .unwrap_or_else(|| {
                    self.lower_internal_async_loop_constructor(expression, context)
                        .unwrap_or_else(|| self.lower_call(expression, expected, context))
                }),
            Expr::Call(_, _) => self
                .lower_internal_async_loop_constructor(expression, context)
                .unwrap_or_else(|| self.lower_call(expression, expected, context)),
            Expr::StructLiteral {
                constructor,
                fields,
            } => self.lower_struct_literal(constructor, fields, expected, context),
            Expr::Member(base, field) => self.lower_member(base, field, expected, context),
            Expr::ChainMember(base, field) => {
                self.lower_chain(base, field, None, expected, context)
            }
            Expr::Index { base, index } => {
                if matches!(
                    self.probe_expr_ty(base, None, context),
                    TypeProbe::Known(Ty::Array(_, _)) | TypeProbe::KnownSource(Ty::Array(_, _), _)
                ) && integer_literal_value(index).is_some()
                    && self.lower_place_without_diagnostic(base, context).is_some()
                {
                    let Some(place) = self.lower_place(expression, context) else {
                        return error_expr();
                    };
                    self.access_place(place, AccessKind::Auto, context)
                } else {
                    self.lower_index(base, index, context)
                }
            }
            Expr::Block(statements, tail) => {
                context.push_scope();
                let mut lowered_statements = Vec::new();
                let mut source_statements = statements.clone();
                let mut source_tail = tail.as_deref().cloned();
                let mut statement_index = 0;
                while statement_index < source_statements.len() {
                    let statement = source_statements[statement_index].clone();
                    let previous_statement_origin = self.current_origin.clone();
                    if let Stmt::Let(binding) = &statement {
                        if let Some(span) = &binding.value_source {
                            if let Some(origin) = &mut self.current_origin {
                                if let Some(source) = &mut origin.source {
                                    source.line = span.line;
                                    source.column = span.column;
                                    source.end_line = span.end_line;
                                    source.end_column = span.end_column;
                                }
                            }
                        }
                    }
                    match &statement {
                        Stmt::Let(binding) => {
                            let specialized = if statement_index + 1 < source_statements.len() {
                                let next = match &mut source_statements[statement_index + 1] {
                                    Stmt::Let(next) => &mut next.value,
                                    Stmt::Expr(next) => next,
                                };
                                self.specialize_capturing_handler_action_binding(
                                    binding, next, context,
                                )
                            } else if let Some(tail) = source_tail.as_mut() {
                                self.specialize_capturing_handler_action_binding(
                                    binding, tail, context,
                                )
                            } else {
                                false
                            };
                            if specialized {
                                statement_index += 1;
                                continue;
                            }
                            let borrow_annotation = binding.annotation.as_ref().and_then(|ty| {
                                matches!(ty, Type::Borrow { .. })
                                    .then(|| self.lower_source_type(ty))
                            });
                            let annotation = binding
                                .annotation
                                .as_ref()
                                .map(|annotation| self.lower_source_type(annotation));
                            let callable_source = match binding.value.unlocated() {
                                Expr::Name(name) => context.lookup(name).cloned().filter(|local| {
                                    local.partial.is_some() || local.closure.is_some()
                                }),
                                _ => None,
                            };
                            let value = match binding.value.unlocated() {
                                Expr::Closure(params, body) => {
                                    let annotation_custom_effect_sources = binding
                                        .annotation
                                        .as_ref()
                                        .and_then(|annotation| match annotation {
                                            Type::Function { effects, .. } => Some(
                                                self.function_effects_custom_source_map(effects),
                                            ),
                                            _ => None,
                                        })
                                        .unwrap_or_default();
                                    let (declared_result, mut effects) = match annotation.as_ref() {
                                        Some(Ty::Function(function)) => (
                                            Some((*function.result).clone()),
                                            ClosureEffectContext {
                                                unsafe_depth: usize::from(function.unsafety),
                                                failure_error: function
                                                    .failure_error
                                                    .as_deref()
                                                    .cloned(),
                                                custom_effects: function
                                                    .custom_effects
                                                    .iter()
                                                    .cloned()
                                                    .collect(),
                                                custom_effect_sources:
                                                    annotation_custom_effect_sources,
                                                lexical_handler_effects: HashSet::new(),
                                                lexical_handler_effect_sources: HashMap::new(),
                                                infer_effects: false,
                                            },
                                        ),
                                        Some(other) => {
                                            self.error(format!(
                                                "closure binding `{}` requires a function type annotation, found `{other}`",
                                                binding.name
                                            ));
                                            (None, ClosureEffectContext::default())
                                        }
                                        None => (None, ClosureEffectContext::default()),
                                    };
                                    let capture_policy =
                                        if is_internal_handler_closure_binding(&binding.name) {
                                            ClosureCapturePolicy::HandlerOwned
                                        } else {
                                            ClosureCapturePolicy::Lexical
                                        };
                                    if capture_policy == ClosureCapturePolicy::HandlerOwned {
                                        effects.lexical_handler_effects =
                                            context.lexical_handler_effects.clone();
                                        effects.lexical_handler_effect_sources =
                                            context.lexical_handler_effect_sources.clone();
                                    }
                                    self.lower_local_closure(
                                        params,
                                        body,
                                        declared_result,
                                        effects,
                                        capture_policy,
                                        context,
                                    )
                                }
                                Expr::PatternClosure {
                                    pattern,
                                    guard,
                                    body,
                                } => {
                                    let annotation_custom_effect_sources = binding
                                        .annotation
                                        .as_ref()
                                        .and_then(|annotation| match annotation {
                                            Type::Function { effects, .. } => Some(
                                                self.function_effects_custom_source_map(effects),
                                            ),
                                            _ => None,
                                        })
                                        .unwrap_or_default();
                                    let Some(Ty::Function(function)) = annotation.as_ref() else {
                                        self.error(format!(
                                            "pattern closure binding `{}` requires a function type annotation",
                                            binding.name
                                        ));
                                        return error_expr();
                                    };
                                    self.lower_local_pattern_closure(
                                        pattern,
                                        guard.as_deref(),
                                        body,
                                        function,
                                        annotation_custom_effect_sources,
                                        context,
                                    )
                                }
                                Expr::Name(_) if callable_source.is_some() => {
                                    let source = callable_source
                                        .as_ref()
                                        .expect("callable source was resolved");
                                    let place = HirPlace {
                                        local: source.id,
                                        root_ty: source.ty.clone(),
                                        projections: Vec::new(),
                                        dynamic_index: None,
                                        ty: source.ty.clone(),
                                        capability: source.capability,
                                        root_mutable: source.mutable,
                                        loan: None,
                                        indirect: false,
                                    };
                                    self.access_place(place, AccessKind::Move, context)
                                }
                                _ if borrow_annotation.is_some() => self
                                    .lower_reference_value_expr(
                                        &binding.value,
                                        borrow_annotation
                                            .as_ref()
                                            .expect("borrow annotation was checked"),
                                        context,
                                    ),
                                _ => self.lower_expr(&binding.value, annotation.as_ref(), context),
                            };
                            if let Some(borrow_ty) = &borrow_annotation {
                                self.require_same_type(
                                    &value.ty,
                                    borrow_ty,
                                    format_args!("borrow value of local `{}`", binding.name),
                                );
                            }
                            let ty = if matches!(value.kind, HirExprKind::LocalClosure(_)) {
                                value.ty.clone()
                            } else {
                                annotation.unwrap_or_else(|| value.ty.clone())
                            };
                            let partial = match &value.kind {
                                HirExprKind::Partial { .. } => partial_info_for_callable(&ty),
                                HirExprKind::Function(function) => Some(PartialInfo {
                                    function: function.clone(),
                                    consumed_groups: 0,
                                    capture_count: 0,
                                    is_fn_mut: false,
                                    is_fn_once: false,
                                }),
                                HirExprKind::Read { .. } => callable_source
                                    .as_ref()
                                    .and_then(|source| source.partial.clone()),
                                _ => partial_info_for_callable(&ty),
                            };
                            let closure = match &value.kind {
                                HirExprKind::LocalClosure(closure) => Some(closure.clone()),
                                HirExprKind::Read { .. } => callable_source
                                    .as_ref()
                                    .and_then(|source| source.closure.clone()),
                                _ => closure_info_for_callable(&ty),
                            };
                            let (capability, alias) = match &value.kind {
                                HirExprKind::Borrow { mutable, .. }
                                    if matches!(ty, Ty::Reference { .. }) =>
                                {
                                    (
                                        if *mutable {
                                            LocalCapability::MutParam
                                        } else {
                                            LocalCapability::SharedParam
                                        },
                                        None,
                                    )
                                }
                                HirExprKind::Borrow { place, mutable } => (
                                    if *mutable {
                                        LocalCapability::MutParam
                                    } else {
                                        LocalCapability::SharedParam
                                    },
                                    Some(place.clone()),
                                ),
                                _ if matches!(value.ty, Ty::Reference { mutable: true, .. }) => {
                                    (LocalCapability::MutParam, None)
                                }
                                _ if matches!(value.ty, Ty::Reference { mutable: false, .. }) => {
                                    (LocalCapability::SharedParam, None)
                                }
                                _ => (LocalCapability::Owned, None),
                            };
                            let reference_origin =
                                self.reference_origin_for_hir_expr(&value, context);
                            let reference_loans =
                                self.reference_loans_for_hir_expr(&value, context);
                            if matches!(ty, Ty::Function(_))
                                && partial.is_none()
                                && closure.is_none()
                                && !matches!(&ty, Ty::Function(function) if function.custom_effects.iter().any(|effect| {
                                    context.active_custom_effects.contains(effect)
                                        && self.collection.effect_defs.get(effect.split('(').next().unwrap_or(effect)).is_some_and(|definition| !definition.operations.is_empty())
                                }))
                            {
                                self.error(format!(
                                    "function-valued local `{}` must be a direct partial application",
                                    binding.name
                                ));
                            }
                            if partial.as_ref().is_some_and(|partial| !partial.is_fn_mut)
                                && binding.mutable
                            {
                                self.error(format!(
                                    "local partial application `{}` must be immutable",
                                    binding.name
                                ));
                            }
                            if partial.as_ref().is_some_and(|partial| partial.is_fn_mut)
                                && !binding.mutable
                            {
                                self.error(format!(
                                    "fn_mut partial application `{}` requires a mutable binding (`let mut`)",
                                    binding.name
                                ));
                            }
                            if closure.as_ref().is_some_and(|closure| closure.is_fn_mut)
                                && !binding.mutable
                            {
                                self.error(format!(
                                    "fn_mut closure `{}` requires a mutable binding (`let mut`)",
                                    binding.name
                                ));
                            }
                            let duplicate = context
                                .scopes
                                .last()
                                .expect("block scope")
                                .names
                                .contains_key(&binding.name);
                            if duplicate {
                                self.error(format!(
                                    "duplicate binding `{}` in the same scope",
                                    binding.name
                                ));
                            }
                            let id = context.fresh_local();
                            if !duplicate {
                                if let Some(origin) = reference_origin {
                                    context.borrowed_parameter_regions.insert(id, origin);
                                }
                                if !reference_loans.is_empty() {
                                    context.reference_loans.insert(id, reference_loans);
                                }
                                if matches!(binding.value.unlocated(), Expr::Closure(_, _))
                                    && closure.is_some()
                                {
                                    context.source_closures.insert(id, binding.clone());
                                } else if let Some(source) = callable_source
                                    .as_ref()
                                    .and_then(|source| context.source_closures.get(&source.id))
                                    .cloned()
                                {
                                    context.source_closures.insert(id, source);
                                }
                                context.insert_local(
                                    binding.name.clone(),
                                    LocalInfo {
                                        id,
                                        ty: ty.clone(),
                                        mutable: binding.mutable,
                                        capability,
                                        alias,
                                        partial,
                                        closure,
                                    },
                                );
                            }
                            lowered_statements.push(HirStmt::Let(HirBinding {
                                id,
                                name: binding.name.clone(),
                                ty,
                                mutable: binding.mutable,
                                value,
                            }));
                        }
                        Stmt::Expr(expression) => {
                            lowered_statements
                                .push(HirStmt::Expr(self.lower_expr(expression, None, context)));
                        }
                    }
                    self.current_origin = previous_statement_origin;
                    statement_index += 1;
                }
                let lowered_tail = source_tail
                    .as_ref()
                    .map(|tail| Box::new(self.lower_expr(tail, expected, context)));
                let ty = lowered_tail
                    .as_ref()
                    .map_or(Ty::Unit, |tail| tail.ty.clone());
                let escaping_loans = lowered_tail.as_ref().map_or_else(Vec::new, |tail| {
                    self.reference_loans_for_hir_expr(tail, context)
                });
                if escaping_loans.is_empty() {
                    context.pop_scope();
                } else {
                    context.pop_scope_preserving_loans(&escaping_loans);
                }
                HirExpr {
                    ty,
                    kind: HirExprKind::Block(lowered_statements, lowered_tail),
                }
            }
            Expr::Closure(_, _) => {
                self.error("closures are not supported in M0");
                error_expr()
            }
            Expr::PatternClosure { .. } => {
                self.error("pattern closure requires a contextual partial-function type");
                error_expr()
            }
            Expr::If {
                condition,
                then_branch,
                else_branch,
            } => {
                let condition = self.lower_expr(condition, Some(&Ty::Bool), context);
                let entry_flow = context.flow.clone();
                let (mut then_branch, mut else_branch, exit_flows) = if let Some(else_ast) =
                    else_branch.as_ref()
                {
                    let (then_branch, then_flow, else_branch, else_flow) = if expected.is_some() {
                        let (then_branch, then_flow) =
                            self.lower_expr_from_flow(then_branch, expected, &entry_flow, context);
                        let (else_branch, else_flow) =
                            self.lower_expr_from_flow(else_ast, expected, &entry_flow, context);
                        (then_branch, then_flow, else_branch, else_flow)
                    } else if self.lowering.async_factory_depth > 0 {
                        let (then_branch, then_flow) =
                            self.lower_expr_from_flow(then_branch, None, &entry_flow, context);
                        let (else_branch, else_flow) =
                            self.lower_expr_from_flow(else_ast, None, &entry_flow, context);
                        (then_branch, then_flow, else_branch, else_flow)
                    } else if is_unconstrained_integer(then_branch)
                        && !is_unconstrained_integer(else_ast)
                    {
                        let (else_branch, else_flow) =
                            self.lower_expr_from_flow(else_ast, None, &entry_flow, context);
                        let branch_hint = if else_branch.ty == Ty::Error
                            || self.is_uninhabited_type(&else_branch.ty)
                        {
                            None
                        } else {
                            Some(&else_branch.ty)
                        };
                        let (then_branch, then_flow) = self.lower_expr_from_flow(
                            then_branch,
                            branch_hint,
                            &entry_flow,
                            context,
                        );
                        (then_branch, then_flow, else_branch, else_flow)
                    } else {
                        let (then_branch, then_flow) =
                            self.lower_expr_from_flow(then_branch, None, &entry_flow, context);
                        let branch_hint = if then_branch.ty == Ty::Error
                            || self.is_uninhabited_type(&then_branch.ty)
                        {
                            None
                        } else {
                            Some(&then_branch.ty)
                        };
                        let (else_branch, else_flow) =
                            self.lower_expr_from_flow(else_ast, branch_hint, &entry_flow, context);
                        (then_branch, then_flow, else_branch, else_flow)
                    };
                    (
                        then_branch,
                        Some(Box::new(else_branch)),
                        vec![then_flow, else_flow],
                    )
                } else {
                    let (then_branch, then_flow) = self.lower_expr_from_flow(
                        then_branch,
                        Some(&Ty::Unit),
                        &entry_flow,
                        context,
                    );
                    (then_branch, None, vec![then_flow, entry_flow])
                };
                context.flow = FlowState::join(&exit_flows);
                if let Some(else_value) = else_branch.as_mut() {
                    if then_branch.ty != else_value.ty {
                        if let Some(branch_name) = self.register_async_branch_future(&[
                            then_branch.ty.clone(),
                            else_value.ty.clone(),
                        ]) {
                            let branch_ty = Ty::Enum(branch_name.clone());
                            then_branch = HirExpr {
                                ty: branch_ty.clone(),
                                kind: HirExprKind::ConstructEnum {
                                    name: branch_name.clone(),
                                    variant: 0,
                                    fields: vec![(0, then_branch)],
                                },
                            };
                            **else_value = HirExpr {
                                ty: branch_ty,
                                kind: HirExprKind::ConstructEnum {
                                    name: branch_name,
                                    variant: 1,
                                    fields: vec![(0, (**else_value).clone())],
                                },
                            };
                        }
                    }
                }
                let ty = if let Some(else_branch) = &else_branch {
                    self.unify_types(&then_branch.ty, &else_branch.ty, "branches of `if`")
                } else {
                    self.require_same_type(
                        &then_branch.ty,
                        &Ty::Unit,
                        "then branch of `if` without `else`",
                    );
                    Ty::Unit
                };
                HirExpr {
                    ty,
                    kind: HirExprKind::If {
                        condition: Box::new(condition),
                        then_branch: Box::new(then_branch),
                        else_branch,
                    },
                }
            }
            Expr::Return(value) => {
                if context.function_name.is_none() {
                    self.error("`return` may only appear in a function body");
                }
                let boundary = context.return_boundary.clone();
                let declared_result = context.declared_result.clone();
                let value = if let Some(boundary) = &boundary {
                    Some(Box::new(match value {
                        Some(value) => self.lower_return_value(value, boundary, context),
                        None => self.finish_return_value(
                            HirExpr {
                                ty: Ty::Unit,
                                kind: HirExprKind::Unit,
                            },
                            boundary,
                        ),
                    }))
                } else {
                    value.as_ref().map(|value| {
                        Box::new(self.lower_expr(value, declared_result.as_ref(), context))
                    })
                };
                let returned_ty = value.as_ref().map_or(Ty::Unit, |value| value.ty.clone());
                context.returned_types.push(returned_ty);
                context.flow.reachable = false;
                HirExpr {
                    ty: Ty::Never,
                    kind: HirExprKind::Return(value),
                }
            }
            Expr::While {
                condition,
                body,
                post_test,
            } => self.lower_while(condition, body, *post_test, context),
            Expr::Loop { body } => self.lower_loop(body, expected, context),
            Expr::Break(value) => self.lower_break(value.as_deref(), context),
            Expr::Continue => self.lower_continue(context),
            Expr::Match { scrutinee, arms } => self.lower_match(scrutinee, arms, expected, context),
        };

        if self.is_uninhabited_type(&lowered.ty) {
            context.flow.reachable = false;
        }
        if let Some(expected) = expected {
            let array_unsizes_to_slice = matches!(
                (&lowered.ty, expected),
                (Ty::Array(actual, _), Ty::Slice(target)) if actual == target
            );
            if !array_unsizes_to_slice
                && (context.reference_value_depth == 0
                    || !reference_value_types_compatible(&lowered.ty, expected))
            {
                self.require_same_type(&lowered.ty, expected, "expression");
            }
        }
        lowered
    }

    pub(super) fn lower_expr_from_flow(
        &mut self,
        expression: &Expr,
        expected: Option<&Ty>,
        entry: &FlowState,
        context: &mut LowerCtx,
    ) -> (HirExpr, FlowState) {
        context.flow = entry.clone();
        let expression = self.lower_expr(expression, expected, context);
        (expression, context.flow.clone())
    }

    pub(super) fn lower_local_closure(
        &mut self,
        source_params: &[crate::ast::Param],
        body: &Expr,
        declared_result: Option<Ty>,
        mut effects: ClosureEffectContext,
        capture_policy: ClosureCapturePolicy,
        outer: &mut LowerCtx,
    ) -> HirExpr {
        let mut source_groups = vec![source_params];
        let mut body = body;
        while let Expr::Closure(params, nested_body) = body {
            source_groups.push(params);
            body = nested_body;
        }
        let deferred_handler_continuation = source_params.first().is_some_and(|parameter| {
            parameter.name.starts_with("$handler$resume$value$")
                || parameter
                    .name
                    .starts_with("$handler$call$continuation$value$")
                || parameter
                    .name
                    .starts_with("$handler$closure$continuation$value$")
        });

        let mut bound: HashSet<String> = source_groups
            .iter()
            .flat_map(|group| group.iter().map(|param| param.name.clone()))
            .collect();
        let mut capture_uses = Vec::new();
        if !self.scan_simple_closure_captures(body, &mut bound, outer, &mut capture_uses) {
            return error_expr();
        }
        if capture_policy == ClosureCapturePolicy::HandlerOwned {
            for capture in &mut capture_uses {
                let should_move = outer.lookup(&capture.name).is_some_and(|local| {
                    local.capability == LocalCapability::Owned && !self.is_copy_type(&local.ty)
                });
                if should_move && capture.mode != ClosureCaptureMode::Mutable {
                    capture.mode = ClosureCaptureMode::Move;
                }
            }
        } else if capture_policy == ClosureCapturePolicy::AsyncOwned {
            for capture in &mut capture_uses {
                if outer.lookup(&capture.name).is_some_and(|local| {
                    local.capability == LocalCapability::Owned && !self.is_copy_type(&local.ty)
                }) {
                    capture.mode = ClosureCaptureMode::Move;
                }
            }
        }
        let mut reconstructed_inspections = Vec::new();
        if deferred_handler_continuation {
            let mut retained_captures = Vec::with_capacity(capture_uses.len());
            for capture in capture_uses {
                let local = outer
                    .lookup(&capture.name)
                    .expect("capture scanner only records outer locals");
                let Some(inspection) = outer.inspection_bindings.get(&local.id).cloned() else {
                    if !retained_captures
                        .iter()
                        .any(|retained: &ClosureCaptureUse| retained.name == capture.name)
                    {
                        retained_captures.push(capture);
                    }
                    continue;
                };
                let root_name = outer
                    .scopes
                    .iter()
                    .rev()
                    .flat_map(|scope| scope.names.iter())
                    .find_map(|(name, local)| (local.id == inspection.root).then_some(name.clone()))
                    .expect("inspection root remains in scope");
                reconstructed_inspections.push((capture.name, root_name.clone(), inspection));
                if !retained_captures
                    .iter()
                    .any(|capture: &ClosureCaptureUse| capture.name == root_name)
                {
                    retained_captures.push(ClosureCaptureUse {
                        name: root_name,
                        mode: ClosureCaptureMode::Move,
                    });
                }
            }
            capture_uses = retained_captures;
        }
        if deferred_handler_continuation {
            for capture in &mut capture_uses {
                if capture.name.starts_with("$handler$match$inspect$input$") {
                    capture.mode = ClosureCaptureMode::Move;
                    continue;
                }
                let Some(closure) = outer
                    .lookup(&capture.name)
                    .and_then(|local| local.closure.as_ref())
                else {
                    continue;
                };
                capture.mode = if closure.is_fn_once {
                    ClosureCaptureMode::Move
                } else if closure.is_fn_mut {
                    ClosureCaptureMode::Mutable
                } else {
                    ClosureCaptureMode::Shared
                };
            }
        }

        let function = format!("__closure.{}", self.lowering.next_closure);
        self.lowering.next_closure += 1;
        let mut context =
            LowerCtx::for_function(&function, declared_result.clone(), outer.origin.clone());
        context.unsafe_depth = effects.unsafe_depth;
        context.active_failure_error = effects.failure_error.clone();
        context.active_custom_effects = effects.custom_effects.clone();
        context.active_custom_effect_sources = effects.custom_effect_sources.clone();
        context
            .active_custom_effects
            .extend(effects.lexical_handler_effects.iter().cloned());
        context
            .active_custom_effect_sources
            .extend(effects.lexical_handler_effect_sources.clone());
        context.lexical_handler_effects = effects.lexical_handler_effects.clone();
        context.lexical_handler_effect_sources = effects.lexical_handler_effect_sources.clone();
        context.infer_effects = effects.infer_effects;
        context.recursive_frame_calls = outer.recursive_frame_calls.clone();
        context.return_boundary = effects.failure_error.as_ref().and_then(|error| {
            declared_result
                .as_ref()
                .and_then(|result| self.failure_boundary_for_ty(result, error))
        });
        context.type_substitutions = outer.type_substitutions.clone();
        let mut hir_params = Vec::new();
        let mut captures = Vec::new();
        let mut capture_names = Vec::new();
        let mut recursive_capture_params = Vec::new();

        let is_fn_once = capture_uses
            .iter()
            .any(|capture| capture.mode == ClosureCaptureMode::Move);
        let is_fn_mut = !is_fn_once
            && capture_uses
                .iter()
                .any(|capture| capture.mode == ClosureCaptureMode::Mutable);
        for capture in capture_uses {
            let name = capture.name;
            let local = outer
                .lookup(&name)
                .cloned()
                .expect("capture scanner only records outer locals");
            if matches!(local.ty, Ty::Function(_))
                && local.partial.as_ref().is_some_and(|partial| {
                    partial.consumed_groups == 0 && partial.capture_count == 0
                })
            {
                let id = context.fresh_local();
                context.scopes[0].locals.push(id);
                context.scopes[0].names.insert(
                    name,
                    LocalInfo {
                        id,
                        ty: local.ty,
                        mutable: false,
                        capability: LocalCapability::Owned,
                        alias: None,
                        partial: local.partial,
                        closure: None,
                    },
                );
                continue;
            }
            let is_captured_partial = local
                .partial
                .as_ref()
                .is_some_and(|partial| partial.consumed_groups != 0 || partial.capture_count != 0);
            let captures_callable =
                local.closure.is_some() && capture.mode == ClosureCaptureMode::Move;
            if is_captured_partial
                || (local.closure.is_some() && !captures_callable && !deferred_handler_continuation)
            {
                self.error(format!("closure cannot capture local callable `{name}`"));
                continue;
            }
            let compatible_reborrow = matches!(
                (local.capability, capture.mode),
                (LocalCapability::SharedParam, ClosureCaptureMode::Shared)
                    | (LocalCapability::MutParam, ClosureCaptureMode::Shared)
                    | (LocalCapability::MutParam, ClosureCaptureMode::Mutable)
            );
            if local.alias.is_some()
                || (local.capability != LocalCapability::Owned && !compatible_reborrow)
            {
                self.error(format!(
                    "closure capture mode is incompatible with borrowed local `{name}`"
                ));
                continue;
            }
            let reborrows_borrowed_local =
                local.capability != LocalCapability::Owned && compatible_reborrow;
            match capture.mode {
                ClosureCaptureMode::Shared
                    if !(reborrows_borrowed_local
                        || self.is_copy_type(&local.ty)
                        || deferred_handler_continuation && local.closure.is_some()) =>
                {
                    if name.starts_with("$handler$match$input$")
                        || name.starts_with("$handler$match$inspect$input$")
                    {
                        self.error(
                            "an effectful match guard currently requires its match input to implement copyable",
                        );
                    } else {
                        self.error(format!(
                            "closure capture `{name}` must implement copyable for this capture mode"
                        ));
                    }
                    continue;
                }
                ClosureCaptureMode::Move
                    if !matches!(
                        local.ty,
                        Ty::Struct(_) | Ty::Enum(_) | Ty::Callable(_) | Ty::Continuation { .. }
                    ) && capture_policy != ClosureCapturePolicy::AsyncOwned =>
                {
                    self.error(format!(
                        "fn_once move capture `{name}` must be a nominal root local for now"
                    ));
                    continue;
                }
                _ => {}
            }

            let mut place = HirPlace {
                local: local.id,
                root_ty: local.ty.clone(),
                projections: Vec::new(),
                dynamic_index: None,
                ty: local.ty.clone(),
                capability: local.capability,
                root_mutable: local.mutable,
                loan: None,
                indirect: false,
            };
            let async_copy_capture = capture_policy == ClosureCapturePolicy::AsyncOwned
                && capture.mode == ClosureCaptureMode::Shared
                && local.capability == LocalCapability::Owned
                && self.is_copy_type(&local.ty);
            let (parameter_mode, capability, mutable, value) = match capture.mode {
                ClosureCaptureMode::Shared if async_copy_capture => (
                    PassMode::Copy,
                    LocalCapability::Owned,
                    false,
                    Some(Box::new(self.access_place(
                        place.clone(),
                        AccessKind::Copy,
                        outer,
                    ))),
                ),
                ClosureCaptureMode::Shared => {
                    if !deferred_handler_continuation {
                        place.loan = self.acquire_loan(&place, LoanKind::Shared, true, outer);
                    }
                    (PassMode::Borrow, LocalCapability::SharedParam, false, None)
                }
                ClosureCaptureMode::Mutable => {
                    self.ensure_writable(&place);
                    if !deferred_handler_continuation {
                        place.loan = self.acquire_loan(&place, LoanKind::Mutable, true, outer);
                    }
                    (PassMode::MutBorrow, LocalCapability::MutParam, true, None)
                }
                ClosureCaptureMode::Move => {
                    let value = self.access_place(place.clone(), AccessKind::Move, outer);
                    let keeps_mutability = deferred_handler_continuation
                        || matches!(
                            capture_policy,
                            ClosureCapturePolicy::HandlerOwned | ClosureCapturePolicy::AsyncOwned
                        );
                    (
                        PassMode::Move,
                        LocalCapability::Owned,
                        keeps_mutability && local.mutable,
                        Some(Box::new(value)),
                    )
                }
            };
            captures.push(ClosureCapture {
                place,
                mode: capture.mode,
                by_value: async_copy_capture,
                value,
                forwarded: None,
            });
            capture_names.push(name.clone());

            let id = context.fresh_local();
            let captured_closure = local.closure.clone().map(|mut closure| {
                for (index, capture) in closure.captures.iter_mut().enumerate() {
                    capture.forwarded = Some(ForwardedClosureCapture {
                        binding: id,
                        index,
                        callable_ty: local.ty.clone(),
                    });
                }
                closure
            });
            context.scopes[0].locals.push(id);
            context.scopes[0].names.insert(
                name.clone(),
                LocalInfo {
                    id,
                    ty: local.ty.clone(),
                    mutable,
                    capability,
                    alias: None,
                    partial: None,
                    closure: captured_closure,
                },
            );
            hir_params.push(HirParam {
                id,
                name: format!("capture.{name}"),
                ty: local.ty,
                mode: parameter_mode,
            });
            recursive_capture_params.push(ParamSig {
                name,
                ty: hir_params.last().expect("capture parameter").ty.clone(),
                mode: parameter_mode,
            });
        }

        for (name, root_name, inspection) in reconstructed_inspections {
            let root = context
                .lookup(&root_name)
                .cloned()
                .expect("inspection root capture exists");
            let id = context.fresh_local();
            let capability = match inspection.ty {
                Ty::Reference { mutable: true, .. } => LocalCapability::MutParam,
                _ => LocalCapability::SharedParam,
            };
            let place = HirPlace {
                local: root.id,
                root_ty: root.ty,
                projections: inspection.path.clone(),
                dynamic_index: None,
                ty: inspection.ty.clone(),
                capability,
                root_mutable: capability == LocalCapability::MutParam,
                loan: None,
                indirect: false,
            };
            context.scopes[0].names.insert(
                name,
                LocalInfo {
                    id,
                    ty: inspection.ty.clone(),
                    mutable: false,
                    capability,
                    alias: Some(place),
                    partial: None,
                    closure: None,
                },
            );
            context.inspection_bindings.insert(
                id,
                InspectionBinding {
                    root: root.id,
                    path: inspection.path,
                    ty: inspection.ty,
                },
            );
        }

        let mut groups = Vec::new();
        for source_group in source_groups {
            let mut group = Vec::new();
            for param in source_group {
                let ty = self.lower_source_type(&param.ty);
                if context.scopes[0].names.contains_key(&param.name) {
                    self.error(format!("duplicate closure parameter `{}`", param.name));
                    continue;
                }
                let id = context.fresh_local();
                if matches!(param.mode, PassMode::Borrow | PassMode::MutBorrow) {
                    context.borrowed_parameter_regions.insert(
                        id,
                        (param.region.clone(), param.mode == PassMode::MutBorrow),
                    );
                } else if let Ty::Reference {
                    mutable, region, ..
                } = &ty
                {
                    context
                        .borrowed_parameter_regions
                        .insert(id, (region.clone(), *mutable));
                }
                let (capability, mutable) = match (&param.mode, &ty) {
                    (PassMode::Borrow, _) => (LocalCapability::SharedParam, false),
                    (PassMode::MutBorrow, _) => (LocalCapability::MutParam, true),
                    (_, Ty::Reference { mutable, .. }) => (
                        if *mutable {
                            LocalCapability::MutParam
                        } else {
                            LocalCapability::SharedParam
                        },
                        *mutable,
                    ),
                    (PassMode::Inferred | PassMode::Copy | PassMode::Move, _) => {
                        (LocalCapability::Owned, false)
                    }
                };
                context.scopes[0].locals.push(id);
                context.scopes[0].names.insert(
                    param.name.clone(),
                    LocalInfo {
                        id,
                        ty: ty.clone(),
                        mutable,
                        capability,
                        alias: None,
                        partial: None,
                        closure: None,
                    },
                );
                group.push(ParamSig {
                    name: param.name.clone(),
                    ty: ty.clone(),
                    mode: param.mode,
                });
                hir_params.push(HirParam {
                    id,
                    name: param.name.clone(),
                    ty,
                    mode: param.mode,
                });
            }
            groups.push(group);
        }

        if let Some(result) = declared_result.clone() {
            let mut tokens = HashSet::new();
            collect_internal_recursion_tokens(body, &mut tokens);
            let parameters = groups.iter().flatten().cloned().collect::<Vec<_>>();
            for token in tokens {
                context
                    .recursive_frame_calls
                    .entry(token)
                    .or_insert_with(|| RecursiveFrameCall {
                        function: function.clone(),
                        captures: recursive_capture_params.clone(),
                        parameters: parameters.clone(),
                        result: result.clone(),
                    });
            }
        }

        let boundary = context.return_boundary.clone();
        let lowered_body = if let Some(boundary) = &boundary {
            self.lower_return_value(body, boundary, &mut context)
        } else {
            self.lower_expr(body, declared_result.as_ref(), &mut context)
        };
        if effects.infer_effects {
            if context.inferred_unsafety {
                effects.unsafe_depth = 1;
            }
            effects
                .custom_effects
                .extend(context.inferred_custom_effects.iter().cloned());
            effects
                .custom_effect_sources
                .extend(context.inferred_custom_effect_sources.clone());
        }
        let mut result = if let Some(declared) = declared_result {
            Some(declared)
        } else if self.is_uninhabited_type(&lowered_body.ty) {
            None
        } else {
            Some(lowered_body.ty.clone())
        };
        for returned in &context.returned_types {
            result = Some(match result {
                Some(current) => self.unify_types(
                    &current,
                    returned,
                    format!("return values in closure `{function}`"),
                ),
                None => returned.clone(),
            });
        }
        let result = result.unwrap_or(Ty::Unit);
        self.lowering.lifted_functions.push(HirFunction {
            name: function.clone(),
            params: hir_params,
            result: result.clone(),
            body: lowered_body,
        });

        let mut custom_effects = effects.custom_effects.into_iter().collect::<Vec<_>>();
        custom_effects.sort();
        let callable_ty = Ty::Callable(CallableTy {
            signature: FunctionTy {
                groups: groups
                    .iter()
                    .map(|group| group.iter().map(|param| param.ty.clone()).collect())
                    .collect(),
                unsafety: effects.unsafe_depth > 0,
                failure_error: effects.failure_error.clone().map(Box::new),
                custom_effects: custom_effects.clone(),
                result: Box::new(result.clone()),
            },
            captures: captures
                .iter()
                .map(|capture| CallableCaptureTy {
                    ty: capture.place.ty.clone(),
                    mode: match capture.mode {
                        ClosureCaptureMode::Shared => PassMode::Borrow,
                        ClosureCaptureMode::Mutable => PassMode::MutBorrow,
                        ClosureCaptureMode::Move => PassMode::Move,
                    },
                })
                .collect(),
            kind: CallableKind::Closure {
                function: function.clone(),
                is_fn_mut,
                is_fn_once,
            },
        });
        let info = ClosureInfo {
            function,
            groups: groups.clone(),
            unsafety: effects.unsafe_depth > 0,
            failure_error: effects.failure_error,
            custom_effects,
            result: result.clone(),
            captures,
            capture_names,
            is_fn_mut,
            is_fn_once,
        };
        HirExpr {
            ty: callable_ty,
            kind: HirExprKind::LocalClosure(info),
        }
    }

    pub(super) fn lower_local_pattern_closure(
        &mut self,
        pattern: &Pattern,
        guard: Option<&Expr>,
        body: &Expr,
        function: &FunctionTy,
        custom_effect_sources: HashMap<String, Type>,
        outer: &mut LowerCtx,
    ) -> HirExpr {
        let [input_group] = function.groups.as_slice() else {
            self.error("pattern closure type must contain exactly one parameter group");
            return error_expr();
        };
        let [input] = input_group.as_slice() else {
            self.error("pattern closure type must contain exactly one input parameter");
            return error_expr();
        };
        let Ty::Enum(attempt_name) = function.result.as_ref() else {
            self.error("pattern closure result must be `attempt(input)(output)`");
            return error_expr();
        };
        let attempt_template = self.lang_item_name(LangItemKind::Attempt);
        if self
            .collection
            .nominal_instances
            .get(attempt_name)
            .is_none_or(|instance| instance.key.template != attempt_template)
        {
            self.error("pattern closure result must use `core.control.Attempt`");
            return error_expr();
        }
        let Some(layout) = self.enum_layout_or_diagnostic(attempt_name) else {
            return error_expr();
        };
        let hit = layout.variants.iter().find(|variant| variant.name == "hit");
        let miss = layout
            .variants
            .iter()
            .find(|variant| variant.name == "miss");
        let (Some(hit), Some(miss)) = (hit, miss) else {
            self.error("pattern closure result must provide `hit(output)` and `miss(input)`");
            return error_expr();
        };
        if hit.fields.len() != 1 || miss.fields.len() != 1 || miss.fields[0].ty != *input {
            self.error("pattern closure result must be `attempt(input)(output)`");
            return error_expr();
        }

        let Some(input_source) = self.source_type_for_ty(input) else {
            self.error(format!(
                "pattern closure input type `{input}` cannot be represented in source"
            ));
            return error_expr();
        };
        let hidden_input = format!("$pattern$input${}", self.lowering.next_closure);
        let missed_input = format!("$pattern$miss${}", self.lowering.next_closure);
        let variant = |name: &str, value: Expr| {
            Expr::Call(
                Box::new(Expr::Member(
                    Box::new(Expr::Name(attempt_name.clone())),
                    name.to_owned(),
                )),
                vec![CallArg { label: None, value }],
            )
        };
        let match_body = Expr::Match {
            scrutinee: Box::new(Expr::Name(hidden_input.clone())),
            arms: vec![
                MatchArm {
                    pattern: pattern.clone(),
                    guard: guard.cloned(),
                    body: variant("hit", body.clone()),
                },
                MatchArm {
                    pattern: Pattern::Binding(missed_input.clone()),
                    guard: None,
                    body: variant("miss", Expr::Name(missed_input)),
                },
            ],
        };
        self.lower_local_closure(
            &[crate::ast::Param {
                mode: PassMode::Move,
                access: None,
                modifiers: Vec::new(),
                region: None,
                name: hidden_input,
                ty: input_source,
            }],
            &match_body,
            Some((*function.result).clone()),
            ClosureEffectContext {
                unsafe_depth: usize::from(function.unsafety),
                failure_error: function.failure_error.as_deref().cloned(),
                custom_effects: function.custom_effects.iter().cloned().collect(),
                custom_effect_sources,
                lexical_handler_effects: HashSet::new(),
                lexical_handler_effect_sources: HashMap::new(),
                infer_effects: false,
            },
            ClosureCapturePolicy::Lexical,
            outer,
        )
    }

    pub(super) fn scan_simple_closure_captures(
        &mut self,
        expression: &Expr,
        bound: &mut HashSet<String>,
        outer: &LowerCtx,
        captures: &mut Vec<ClosureCaptureUse>,
    ) -> bool {
        match expression {
            Expr::Type(_) | Expr::Unit | Expr::Integer(_) | Expr::Bool(_) | Expr::String(_) => true,
            Expr::Tuple(fields) => {
                let mut valid = true;
                for field in fields {
                    valid &= self.scan_simple_closure_captures(field, bound, outer, captures);
                }
                valid
            }
            Expr::Name(name) => {
                if !bound.contains(name) && outer.lookup(name).is_some() {
                    record_closure_capture(captures, name, ClosureCaptureMode::Shared);
                }
                true
            }
            Expr::Unary(_, operand)
            | Expr::Try(operand)
            | Expr::Throw(operand)
            | Expr::Async { body: operand }
            | Expr::Await(operand)
            | Expr::Unsafe(operand)
            | Expr::DoBlock { body: operand }
            | Expr::Borrow { value: operand, .. } => {
                self.scan_simple_closure_captures(operand, bound, outer, captures)
            }
            Expr::Binary(left, _, right) | Expr::Coalesce(left, right) => {
                self.scan_simple_closure_captures(left, bound, outer, captures)
                    & self.scan_simple_closure_captures(right, bound, outer, captures)
            }
            Expr::HandlerCoalesce {
                scrutinee,
                payload,
                success,
                fallback,
            } => {
                let mut valid =
                    self.scan_simple_closure_captures(scrutinee, bound, outer, captures);
                let saved = bound.clone();
                bound.insert(payload.clone());
                valid &= self.scan_simple_closure_captures(success, bound, outer, captures);
                *bound = saved;
                valid & self.scan_simple_closure_captures(fallback, bound, outer, captures)
            }
            Expr::HandlerChainCall(chain) => {
                let mut valid =
                    self.scan_simple_closure_captures(&chain.scrutinee, bound, outer, captures);
                for argument in chain.groups.iter().flatten() {
                    valid &=
                        self.scan_simple_closure_captures(&argument.value, bound, outer, captures);
                }
                let saved = bound.clone();
                bound.insert(chain.payload.clone());
                valid &= self.scan_simple_closure_captures(&chain.success, bound, outer, captures);
                *bound = saved.clone();
                bound.insert(chain.error.clone());
                valid &= self.scan_simple_closure_captures(&chain.residual, bound, outer, captures);
                *bound = saved;
                valid
            }
            Expr::Array(elements) => elements.iter().fold(true, |valid, element| {
                self.scan_simple_closure_captures(element, bound, outer, captures) & valid
            }),
            Expr::StructLiteral { fields, .. } => fields.iter().fold(true, |valid, field| {
                self.scan_simple_closure_captures(&field.value, bound, outer, captures) & valid
            }),
            Expr::Index { base, index } => {
                self.scan_simple_closure_captures(base, bound, outer, captures)
                    & self.scan_simple_closure_captures(index, bound, outer, captures)
            }
            Expr::Assign(place, value) => {
                let mut valid = self.scan_simple_closure_captures(place, bound, outer, captures);
                valid &= self.scan_simple_closure_captures(value, bound, outer, captures);
                if let Some(name) = place_root_name(place) {
                    if !bound.contains(name) && outer.lookup(name).is_some() {
                        record_closure_capture(captures, name, ClosureCaptureMode::Mutable);
                    }
                }
                valid
            }
            Expr::CompoundAssign(place, _, value) => {
                let mut valid = self.scan_simple_closure_captures(place, bound, outer, captures);
                valid &= self.scan_simple_closure_captures(value, bound, outer, captures);
                if let Some(name) = place_root_name(place) {
                    if !bound.contains(name) && outer.lookup(name).is_some() {
                        record_closure_capture(captures, name, ClosureCaptureMode::Mutable);
                    }
                }
                valid
            }
            Expr::Member(base, _) | Expr::ChainMember(base, _) => {
                self.scan_simple_closure_captures(base, bound, outer, captures)
            }
            Expr::Call(_, _) => {
                let mut groups = Vec::new();
                let root = flatten_call(expression, &mut groups);
                if matches!(root, Expr::Name(name) if self.lowering.internal_async_loop_constructors.contains_key(name))
                {
                    return groups.iter().flat_map(|group| group.iter()).fold(
                        true,
                        |valid, argument| {
                            self.scan_simple_closure_captures(
                                &argument.value,
                                bound,
                                outer,
                                captures,
                            ) & valid
                        },
                    );
                }
                if matches!(root, Expr::Name(name) if name == "$async$copy$stored$borrow") {
                    return groups.iter().flat_map(|group| group.iter()).fold(
                        true,
                        |valid, argument| {
                            self.scan_simple_closure_captures(
                                &argument.value,
                                bound,
                                outer,
                                captures,
                            ) & valid
                        },
                    );
                }
                if matches!(root, Expr::Name(name) if name.starts_with("$handler$tail$")) {
                    return groups.iter().flat_map(|group| group.iter()).fold(
                        true,
                        |valid, argument| {
                            self.scan_simple_closure_captures(
                                &argument.value,
                                bound,
                                outer,
                                captures,
                            ) & valid
                        },
                    );
                }
                if matches!(
                    root,
                    Expr::Name(name)
                        if matches!(
                            name.as_str(),
                            "$handler$chain$wrap$success"
                                | "$handler$chain$wrap$residual"
                        )
                ) {
                    return groups.iter().flat_map(|group| group.iter()).fold(
                        true,
                        |valid, argument| {
                            self.scan_simple_closure_captures(
                                &argument.value,
                                bound,
                                outer,
                                captures,
                            ) & valid
                        },
                    );
                }
                if matches!(root, Expr::Name(name) if name == "$handler$invoke$continuation") {
                    let arguments = groups
                        .iter()
                        .flat_map(|group| group.iter())
                        .collect::<Vec<_>>();
                    if let Some(CallArg {
                        value: Expr::Name(name),
                        ..
                    }) = arguments.first()
                    {
                        if !bound.contains(name) && outer.lookup(name).is_some() {
                            record_closure_capture(captures, name, ClosureCaptureMode::Move);
                        }
                    }
                    return arguments.iter().skip(1).fold(true, |valid, argument| {
                        self.scan_simple_closure_captures(&argument.value, bound, outer, captures)
                            & valid
                    });
                }
                if matches!(root, Expr::Name(name) if name == "$handler$invoke$effect$callable") {
                    let arguments = groups
                        .iter()
                        .flat_map(|group| group.iter())
                        .collect::<Vec<_>>();
                    for index in [0, 2] {
                        if let Some(CallArg {
                            value: Expr::Name(name),
                            ..
                        }) = arguments.get(index)
                        {
                            if !bound.contains(name) && outer.lookup(name).is_some() {
                                record_closure_capture(captures, name, ClosureCaptureMode::Move);
                            }
                        }
                    }
                    return arguments
                        .iter()
                        .skip(1)
                        .take(1)
                        .fold(true, |valid, argument| {
                            self.scan_simple_closure_captures(
                                &argument.value,
                                bound,
                                outer,
                                captures,
                            ) & valid
                        });
                }
                if matches!(root, Expr::Name(name) if name == "$handler$erase$continuation") {
                    if let Some(CallArg {
                        value: Expr::Name(name),
                        ..
                    }) = groups.iter().flat_map(|group| group.iter()).next()
                    {
                        if !bound.contains(name) && outer.lookup(name).is_some() {
                            record_closure_capture(captures, name, ClosureCaptureMode::Move);
                        }
                    }
                    return true;
                }
                if matches!(root, Expr::Name(name) if name == "$handler$erase$effect$callable") {
                    if let Some(CallArg {
                        value: Expr::Name(name),
                        ..
                    }) = groups.iter().flat_map(|group| group.iter()).next()
                    {
                        if !bound.contains(name) && outer.lookup(name).is_some() {
                            record_closure_capture(captures, name, ClosureCaptureMode::Move);
                        }
                    }
                    return true;
                }
                if matches!(root, Expr::Name(name) if name.starts_with("$handler$recursive$")) {
                    if let Expr::Name(name) = root {
                        if let Some(frame) = outer.recursive_frame_calls.get(name) {
                            for capture in &frame.captures {
                                if outer.lookup(&capture.name).is_some()
                                    && !bound.contains(&capture.name)
                                {
                                    let mode = match self
                                        .borrow_channel_mode(capture.mode, &capture.ty)
                                        .unwrap_or(capture.mode)
                                    {
                                        PassMode::Borrow => ClosureCaptureMode::Shared,
                                        PassMode::MutBorrow => ClosureCaptureMode::Mutable,
                                        PassMode::Move => ClosureCaptureMode::Move,
                                        PassMode::Inferred | PassMode::Copy => {
                                            ClosureCaptureMode::Shared
                                        }
                                    };
                                    record_closure_capture(captures, &capture.name, mode);
                                }
                            }
                        }
                    }
                    return groups.iter().flat_map(|group| group.iter()).fold(
                        true,
                        |valid, argument| {
                            self.scan_simple_closure_captures(
                                &argument.value,
                                bound,
                                outer,
                                captures,
                            ) & valid
                        },
                    );
                }
                if let Expr::Name(name) = root {
                    if !bound.contains(name)
                        && outer
                            .lookup(name)
                            .is_some_and(|local| local.closure.is_some())
                    {
                        record_closure_capture(captures, name, ClosureCaptureMode::Move);
                        return groups.iter().flat_map(|group| group.iter()).fold(
                            true,
                            |valid, argument| {
                                self.scan_simple_closure_captures(
                                    &argument.value,
                                    bound,
                                    outer,
                                    captures,
                                ) & valid
                            },
                        );
                    }
                }
                if matches!(root, Expr::Name(name) if bound.contains(name)) {
                    let modes = match root {
                        Expr::Name(name) => self
                            .lowering
                            .handler_frame_parameter_modes
                            .get(name)
                            .cloned(),
                        _ => None,
                    };
                    let mut valid = true;
                    for (index, argument) in
                        groups.iter().flat_map(|group| group.iter()).enumerate()
                    {
                        if let Expr::Name(name) = &argument.value {
                            if !bound.contains(name) && outer.lookup(name).is_some() {
                                let mode = modes
                                    .as_ref()
                                    .and_then(|modes| modes.get(index))
                                    .cloned()
                                    .unwrap_or(PassMode::Inferred);
                                let capture = match mode {
                                    PassMode::Borrow => ClosureCaptureMode::Shared,
                                    PassMode::MutBorrow => ClosureCaptureMode::Mutable,
                                    PassMode::Move => ClosureCaptureMode::Move,
                                    PassMode::Inferred | PassMode::Copy => {
                                        ClosureCaptureMode::Shared
                                    }
                                };
                                record_closure_capture(captures, name, capture);
                                continue;
                            }
                        }
                        valid &= self.scan_simple_closure_captures(
                            &argument.value,
                            bound,
                            outer,
                            captures,
                        );
                    }
                    return valid;
                }
                if let Expr::Name(name) = root {
                    if !bound.contains(name)
                        && outer
                            .lookup(name)
                            .is_some_and(|local| local.closure.is_some())
                    {
                        record_closure_capture(captures, name, ClosureCaptureMode::Move);
                        return groups.iter().flat_map(|group| group.iter()).fold(
                            true,
                            |valid, argument| {
                                self.scan_simple_closure_captures(
                                    &argument.value,
                                    bound,
                                    outer,
                                    captures,
                                ) & valid
                            },
                        );
                    }
                }
                let captured_function = match root {
                    Expr::Name(function)
                        if !bound.contains(function)
                            && outer
                                .lookup(function)
                                .is_some_and(|local| matches!(local.ty, Ty::Function(_))) =>
                    {
                        Some(function.as_str())
                    }
                    _ => None,
                };
                if let Some(function) = captured_function {
                    record_closure_capture(captures, function, ClosureCaptureMode::Shared);
                    groups
                        .iter()
                        .flat_map(|group| group.iter())
                        .fold(true, |valid, argument| {
                            self.scan_simple_closure_captures(
                                &argument.value,
                                bound,
                                outer,
                                captures,
                            ) & valid
                        })
                } else if let Expr::Member(base, _) = root {
                    if let Expr::Name(name) = base.as_ref() {
                        if name.starts_with("$handler$chain$payload$")
                            && !bound.contains(name)
                            && outer.lookup(name).is_some()
                        {
                            record_closure_capture(captures, name, ClosureCaptureMode::Move);
                            return groups.iter().flat_map(|group| group.iter()).fold(
                                true,
                                |valid, argument| {
                                    self.scan_simple_closure_captures(
                                        &argument.value,
                                        bound,
                                        outer,
                                        captures,
                                    ) & valid
                                },
                            );
                        }
                    }
                    let mut valid = self.scan_simple_closure_captures(base, bound, outer, captures);
                    for argument in groups.iter().flat_map(|group| group.iter()) {
                        valid &= self.scan_simple_closure_captures(
                            &argument.value,
                            bound,
                            outer,
                            captures,
                        );
                    }
                    valid
                } else if let Expr::ChainMember(base, _) = root {
                    let mut valid = self.scan_simple_closure_captures(base, bound, outer, captures);
                    for argument in groups.iter().flat_map(|group| group.iter()) {
                        valid &= self.scan_simple_closure_captures(
                            &argument.value,
                            bound,
                            outer,
                            captures,
                        );
                    }
                    valid
                } else if matches!(
                    root,
                    Expr::Name(name)
                        if self.collection.struct_layouts.contains_key(name)
                            || self.collection.struct_templates.contains_key(name)
                            || self.collection.enum_defs.contains_key(name)
                            || self.collection.enum_templates.contains_key(name)
                ) {
                    groups
                        .iter()
                        .flat_map(|group| group.iter())
                        .fold(true, |valid, argument| {
                            self.scan_simple_closure_captures(
                                &argument.value,
                                bound,
                                outer,
                                captures,
                            ) & valid
                        })
                } else {
                    self.scan_direct_move_closure_call(expression, bound, outer, captures)
                }
            }
            Expr::Block(statements, tail) => {
                let saved = bound.clone();
                let mut valid = true;
                for statement in statements {
                    match statement {
                        Stmt::Let(binding) => {
                            valid &= self.scan_simple_closure_captures(
                                &binding.value,
                                bound,
                                outer,
                                captures,
                            );
                            bound.insert(binding.name.clone());
                        }
                        Stmt::Expr(expression) => {
                            valid &= self
                                .scan_simple_closure_captures(expression, bound, outer, captures);
                        }
                    }
                }
                if let Some(tail) = tail {
                    valid &= self.scan_simple_closure_captures(tail, bound, outer, captures);
                }
                *bound = saved;
                valid
            }
            Expr::Closure(parameters, body) => {
                let saved = bound.clone();
                bound.extend(parameters.iter().map(|parameter| parameter.name.clone()));
                let valid = self.scan_simple_closure_captures(body, bound, outer, captures);
                *bound = saved;
                valid
            }
            Expr::While {
                condition, body, ..
            } => {
                self.scan_simple_closure_captures(condition, bound, outer, captures)
                    & self.scan_simple_closure_captures(body, bound, outer, captures)
            }
            Expr::If {
                condition,
                then_branch,
                else_branch,
            } => {
                let mut valid =
                    self.scan_simple_closure_captures(condition, bound, outer, captures);
                valid &= self.scan_simple_closure_captures(then_branch, bound, outer, captures);
                if let Some(else_branch) = else_branch {
                    valid &= self.scan_simple_closure_captures(else_branch, bound, outer, captures);
                }
                valid
            }
            Expr::Loop { body } => self.scan_simple_closure_captures(body, bound, outer, captures),
            Expr::Break(value) => value.as_ref().is_none_or(|value| {
                self.scan_simple_closure_captures(value, bound, outer, captures)
            }),
            Expr::Return(value) => value.as_ref().is_none_or(|value| {
                self.scan_simple_closure_captures(value, bound, outer, captures)
            }),
            Expr::Match { scrutinee, arms } => {
                let mut valid =
                    self.scan_simple_closure_captures(scrutinee, bound, outer, captures);
                for arm in arms {
                    let saved = bound.clone();
                    collect_pattern_binding_names(&arm.pattern, bound);
                    if let Some(guard) = &arm.guard {
                        valid &= self.scan_simple_closure_captures(guard, bound, outer, captures);
                    }
                    valid &= self.scan_simple_closure_captures(&arm.body, bound, outer, captures);
                    *bound = saved;
                }
                valid
            }
            Expr::PatternClosure {
                pattern,
                guard,
                body,
            } => {
                let saved = bound.clone();
                collect_pattern_binding_names(pattern, bound);
                let mut valid = guard.as_ref().is_none_or(|guard| {
                    self.scan_simple_closure_captures(guard, bound, outer, captures)
                });
                valid &= self.scan_simple_closure_captures(body, bound, outer, captures);
                *bound = saved;
                valid
            }
            Expr::Continue => true,
            Expr::Located { value, .. } => {
                self.scan_simple_closure_captures(value, bound, outer, captures)
            }
        }
    }

    pub(super) fn scan_direct_move_closure_call(
        &mut self,
        expression: &Expr,
        bound: &HashSet<String>,
        outer: &LowerCtx,
        captures: &mut Vec<ClosureCaptureUse>,
    ) -> bool {
        let mut groups = Vec::new();
        let root = flatten_call(expression, &mut groups);
        let Expr::Name(function) = root else {
            self.error("fn_once capture requires a direct named-function call");
            return false;
        };
        if outer.has_type_parameter(function) {
            self.error(format!(
                "type parameter `{function}` is not a top-level named function"
            ));
            return false;
        }
        let selected_overload = if self.collection.function_overloads.contains_key(function) {
            self.resolve_function_overload(function, &groups)
        } else {
            None
        };
        if self.collection.function_overloads.contains_key(function) && selected_overload.is_none()
        {
            return false;
        }
        let resolved_function = selected_overload.as_deref().unwrap_or(function);
        let (signature, runtime_groups) = if let Some(signature) =
            self.lowering.signatures.get(resolved_function)
        {
            (signature.clone(), groups.as_slice())
        } else if self
            .collection
            .function_templates
            .contains_key(resolved_function)
        {
            let Some((canonical, runtime_start)) = self.resolve_inferred_generic_function_instance(
                resolved_function,
                &groups,
                None,
                outer,
            ) else {
                return false;
            };
            (
                self.lowering.signatures[&canonical].clone(),
                &groups[runtime_start..],
            )
        } else {
            self.error(format!(
                "closure call `{function}` is not a top-level named function"
            ));
            return false;
        };
        if runtime_groups.len() != signature.groups.len() {
            self.error(format!(
                "named function `{function}` must be fully applied inside a closure"
            ));
            return false;
        }

        let mut valid = true;
        for (group_index, (arguments, parameters)) in
            runtime_groups.iter().zip(&signature.groups).enumerate()
        {
            if arguments.len() != parameters.len() {
                self.error(format!(
                    "argument count mismatch in closure call to `{function}`"
                ));
                valid = false;
            }
            let parameter_names = parameters
                .iter()
                .map(|parameter| parameter.name.clone())
                .collect::<Vec<_>>();
            let Some(ordered) =
                self.ordered_call_arguments(function, group_index + 1, arguments, &parameter_names)
            else {
                valid = false;
                continue;
            };
            for (argument, parameter) in ordered.into_iter().zip(parameters) {
                match &argument.value {
                    Expr::Name(name) if bound.contains(name) => {}
                    Expr::Name(name) => {
                        if let Some(local) = outer.lookup(name) {
                            let mode = self.effective_pass_mode(parameter.mode, &parameter.ty);
                            let borrow_capture = match &parameter.ty {
                                Ty::Reference {
                                    pointee, mutable, ..
                                } if pointee.as_ref() == &local.ty => Some(*mutable),
                                _ => None,
                            };
                            if let Some(mutable) = borrow_capture {
                                record_closure_capture(
                                    captures,
                                    name,
                                    if mutable {
                                        ClosureCaptureMode::Mutable
                                    } else {
                                        ClosureCaptureMode::Shared
                                    },
                                );
                            } else if matches!(mode, PassMode::Borrow | PassMode::MutBorrow)
                                && local.ty == parameter.ty
                            {
                                record_closure_capture(
                                    captures,
                                    name,
                                    if mode == PassMode::MutBorrow {
                                        ClosureCaptureMode::Mutable
                                    } else {
                                        ClosureCaptureMode::Shared
                                    },
                                );
                            } else if mode == PassMode::Move
                                && matches!(local.ty, Ty::Struct(_) | Ty::Enum(_))
                                && local.ty == parameter.ty
                            {
                                record_closure_capture(captures, name, ClosureCaptureMode::Move);
                            } else if mode == PassMode::Copy
                                && self.is_copy_type(&local.ty)
                                && local.ty == parameter.ty
                            {
                                record_closure_capture(captures, name, ClosureCaptureMode::Shared);
                            } else {
                                self.error(format!(
                                    "closure call capture `{name}` must match a copyable parameter or a nominal move parameter"
                                ));
                                valid = false;
                            }
                        }
                    }
                    Expr::Unit | Expr::Integer(_) | Expr::Bool(_) | Expr::String(_) => {}
                    _ => {
                        self.error(
                            "closure call arguments only support literals, closure parameters, or a nominal root move capture",
                        );
                        valid = false;
                    }
                }
            }
        }
        valid
    }
}

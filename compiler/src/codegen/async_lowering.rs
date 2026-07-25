use std::collections::HashMap;

use crate::ast::{
    BinaryOp, CallArg, Expr, Function, FunctionEffects, ItemOrigin, Param, PassMode, Stmt, Type,
    Visibility,
};
use crate::core::LangItemKind;

use super::compile_time::source_type_from_identity;
use super::effects::standard_throws_error_source;
use super::hir::{
    AccessBoundary, AssignmentKind, ClosureCapture, ClosureCaptureMode, ClosureCapturePolicy,
    ClosureEffectContext, EnumLayout, FieldLayout, FunctionSig, HirArgument, HirExpr, HirExprKind,
    HirFunction, HirMatchArm, HirMatcher, HirParam, HirPatternBinding, HirPlace, HirReadKind,
    HirStmt, LocalCapability, ParamSig, StructLayout, Ty, VariantLayout,
};
use super::lower::TypeProbe;
use super::names::trait_method_name;
use super::registry::{NominalKind, TraitImplInfo, TraitImplKey, TraitRefKey};
use super::source_rewrite::source_type_expression;
use super::Analyzer;

impl Analyzer {
    pub(super) fn lower_internal_async_stored_borrow_argument(
        &mut self,
        expression: &Expr,
        parameter: &ParamSig,
        context: &mut super::flow::LowerCtx,
    ) -> Option<HirArgument> {
        let Expr::Call(callee, arguments) = expression.unlocated() else {
            return None;
        };
        if !matches!(callee.unlocated(), Expr::Name(name) if name == "$async$copy$stored$borrow") {
            return None;
        }
        let [CallArg { label: None, value }] = arguments.as_slice() else {
            self.error("internal async stored-borrow argument received an invalid shape");
            return Some(HirArgument::Move(super::lower::error_expr()));
        };
        let place = self.lower_place(value, context)?;
        let Ty::Reference {
            pointee, mutable, ..
        } = &place.ty
        else {
            self.error("internal async stored-borrow argument requires a reference field");
            return Some(HirArgument::Move(super::lower::error_expr()));
        };
        if pointee.as_ref() != &parameter.ty {
            return None;
        }
        let required_mutable = parameter.mode == PassMode::MutBorrow;
        if !matches!(parameter.mode, PassMode::Borrow | PassMode::MutBorrow) {
            self.error("internal async stored-borrow argument requires a borrow parameter");
        }
        if required_mutable && !mutable {
            self.error("internal async stored shared borrow cannot satisfy a mutable borrow");
        }
        self.require_same_type(
            pointee,
            &parameter.ty,
            format_args!("argument for parameter `{}`", parameter.name),
        );
        Some(HirArgument::Copy(HirExpr {
            ty: place.ty.clone(),
            kind: HirExprKind::Read {
                place,
                kind: HirReadKind::Copy,
            },
        }))
    }

    pub(super) fn lower_async_expression(
        &mut self,
        body: &Expr,
        context: &mut super::flow::LowerCtx,
    ) -> HirExpr {
        let mut source_plan = multiple_await_recurring_loop_source(body, self.next_async_future)
            .or_else(|| simple_recurring_async_loop_source(body, self.next_async_future))
            .or_else(|| general_unit_recurring_loop_source(body, self.next_async_future))
            .unwrap_or_else(|| split_async_source(body));
        if !source_plan.has_await {
            if let Some(loop_source) = recurring_suspended_loop_source(body) {
                let suspension = match (loop_source.condition_suspends, loop_source.body_suspends) {
                    (true, true) => "condition and body",
                    (true, false) => "condition",
                    (false, true) => "body",
                    (false, false) => unreachable!("a suspended loop has a suspension source"),
                };
                let exits = [
                    loop_source.has_continue.then_some("`continue`"),
                    loop_source.has_fallthrough.then_some("fallthrough"),
                    loop_source
                        .has_value_break
                        .then_some("value-producing `break`"),
                ]
                .into_iter()
                .flatten()
                .collect::<Vec<_>>();
                let backedge = match exits.as_slice() {
                    [] => "loop backedges are".to_owned(),
                    [exit] => format!("loop backedge through {exit} is"),
                    _ => format!("loop backedges through {} are", exits.join(", ")),
                };
                self.error(format!(
                    "`await` in a recurring {} {suspension} requires reusable iteration-state lowering; {backedge} not lowered yet",
                    loop_source.kind.description(),
                ));
                return super::lower::error_expr();
            }
        }
        for retained in &source_plan.retained {
            if retained.borrowed
                && retained.referent.as_ref().is_some_and(|referent| {
                    source_plan
                        .retained
                        .iter()
                        .any(|candidate| &candidate.name == referent)
                })
            {
                self.error(format!(
                    "async local `{}` borrows `{}` stored in the same future across `await`; the generated state would be self-referential and cannot implement `Move`",
                    retained.name,
                    retained.referent.as_deref().expect("checked retained referent")
                ));
                return super::lower::error_expr();
            }
        }
        let mut loop_step = None;
        if let Some(source) = source_plan.loop_step.as_ref() {
            let mut output = source.output_hint.clone();
            if output.is_none() && !source.probe_awaits.is_empty() {
                let mut probe_context = context.clone();
                for (binding, child) in &source.probe_awaits {
                    let child_ty = match self.probe_expr_ty(child, None, &probe_context) {
                        TypeProbe::Known(ty)
                        | TypeProbe::KnownSource(ty, _)
                        | TypeProbe::Defaultable(ty) => ty,
                        TypeProbe::Unsupported => {
                            self.error(
                                "an await operand in a recurring async iteration has a type that cannot be inferred",
                            );
                            return super::lower::error_expr();
                        }
                    };
                    let Some(awaited) = self.resolve_awaited_future(&child_ty) else {
                        return super::lower::error_expr();
                    };
                    let id = probe_context.fresh_local();
                    probe_context.insert_local(
                        binding.clone(),
                        super::flow::LocalInfo {
                            id,
                            ty: awaited.output,
                            mutable: false,
                            capability: LocalCapability::Owned,
                            alias: None,
                            partial: None,
                            closure: None,
                        },
                    );
                }
                output = match self.probe_expr_ty(&source.break_value, None, &probe_context) {
                    TypeProbe::Known(ty)
                    | TypeProbe::KnownSource(ty, _)
                    | TypeProbe::Defaultable(ty) => Some(ty),
                    TypeProbe::Unsupported => {
                        self.error(
                            "value-producing `break` in a recurring async iteration has an output type that cannot be inferred",
                        );
                        return super::lower::error_expr();
                    }
                };
            }
            if let Some(mut output) = output {
                if output == Ty::Never {
                    output = Ty::Enum(self.lang_item_name(LangItemKind::Never).to_owned());
                }
                if source_plan.loop_condition.is_some() && output != Ty::Unit {
                    self.error(
                        "value-producing `break` is not allowed in a recurring async `while`",
                    );
                    return super::lower::error_expr();
                }
                loop_step = Some(self.register_async_loop_step(
                    Ty::Unit,
                    output,
                    source.continue_constructor.clone(),
                    source.break_constructor.clone(),
                ));
            }
        }
        self.async_factory_depth += 1;
        let lowered = self.lower_local_closure(
            &[],
            &source_plan.factory_body,
            None,
            ClosureEffectContext {
                infer_effects: true,
                ..ClosureEffectContext::default()
            },
            ClosureCapturePolicy::AsyncOwned,
            context,
        );
        self.async_factory_depth -= 1;
        let HirExprKind::LocalClosure(mut closure) = lowered.kind else {
            return lowered;
        };
        let async_effect = self.lang_item_name(LangItemKind::AsyncEffect).to_owned();
        let (awaited_ty, retained_types) = if source_plan.retained.is_empty() {
            (closure.result.clone(), Vec::new())
        } else {
            let Ty::Tuple(fields) = &closure.result else {
                self.error("internal async segment bundle did not produce a tuple");
                return super::lower::error_expr();
            };
            (fields[0].clone(), fields[1..].to_vec())
        };
        let mut awaited = if source_plan.has_await {
            let Some(mut awaited) = self.resolve_awaited_future(&awaited_ty) else {
                return super::lower::error_expr();
            };
            awaited.factory_output = closure.result.clone();
            awaited.retained_types = retained_types;
            awaited.retained_modes = awaited
                .retained_types
                .iter()
                .map(|ty| {
                    if self.is_copy_type(ty) {
                        PassMode::Copy
                    } else {
                        PassMode::Move
                    }
                })
                .collect();
            Some(awaited)
        } else {
            None
        };
        let mut loop_carry_names = Vec::new();
        let mut loop_carry_types = Vec::new();
        if loop_step.is_none() {
            if let Some(source) = source_plan.loop_step.as_ref() {
                for name in &source.carry_names {
                    let Some(local) = context.lookup(name) else {
                        continue;
                    };
                    if !self.is_copy_type(&local.ty) {
                        loop_carry_names.push(name.clone());
                        loop_carry_types.push(local.ty.clone());
                    }
                }
                if !loop_carry_names.is_empty() {
                    let carry =
                        Expr::Tuple(loop_carry_names.iter().cloned().map(Expr::Name).collect());
                    if let Some(continuation) = source_plan.continuation.as_mut() {
                        rewrite_async_loop_continue_carry(
                            &mut continuation.body,
                            &source.continue_constructor,
                            &carry,
                        );
                    }
                }
            }
        }
        if loop_step.is_none() {
            loop_step = match (&source_plan.loop_step, awaited.as_ref()) {
                (Some(source), Some(awaited)) => {
                    let mut probe_context = context.clone();
                    let id = probe_context.fresh_local();
                    probe_context.insert_local(
                        source.binding.clone(),
                        super::flow::LocalInfo {
                            id,
                            ty: awaited.output.clone(),
                            mutable: false,
                            capability: LocalCapability::Owned,
                            alias: None,
                            partial: None,
                            closure: None,
                        },
                    );
                    let output = match self.probe_expr_ty(&source.break_value, None, &probe_context)
                    {
                        TypeProbe::Known(ty)
                        | TypeProbe::KnownSource(ty, _)
                        | TypeProbe::Defaultable(ty) => ty,
                        TypeProbe::Unsupported => {
                            self.error(
                            "value-producing `break` in a recurring async loop has an output type that cannot be inferred",
                        );
                            return super::lower::error_expr();
                        }
                    };
                    if source_plan.loop_condition.is_some() && output != Ty::Unit {
                        self.error(
                            "value-producing `break` is not allowed in a recurring async `while`",
                        );
                        return super::lower::error_expr();
                    }
                    Some(self.register_async_loop_step(
                        if loop_carry_types.is_empty() {
                            Ty::Unit
                        } else {
                            Ty::Tuple(loop_carry_types.clone())
                        },
                        output,
                        source.continue_constructor.clone(),
                        source.break_constructor.clone(),
                    ))
                }
                _ => None,
            };
        }
        if let (Some(awaited), Some(loop_step)) = (awaited.as_mut(), loop_step.as_ref()) {
            awaited.loop_step = Some(loop_step.clone());
        }
        if loop_step.is_some()
            && closure
                .captures
                .iter()
                .map(capture_pass_mode)
                .any(|mode| mode == PassMode::Move)
        {
            self.error(
                "a recurring async loop with move-only factory state requires generated `Continue(Carry)` transfer, which is not lowered yet",
            );
            return super::lower::error_expr();
        }
        let mut loop_condition_captures = Vec::new();
        if let (Some(awaited), Some(condition)) = (awaited.as_mut(), source_plan.loop_condition) {
            let lowered = self.lower_local_closure(
                &[],
                &condition.expression,
                Some(Ty::Bool),
                ClosureEffectContext {
                    infer_effects: true,
                    ..ClosureEffectContext::default()
                },
                ClosureCapturePolicy::AsyncOwned,
                context,
            );
            let HirExprKind::LocalClosure(closure) = lowered.kind else {
                return lowered;
            };
            if closure.result != Ty::Bool {
                self.error("a recurring async `while` condition must produce `bool`");
                return super::lower::error_expr();
            }
            if closure
                .captures
                .iter()
                .map(capture_pass_mode)
                .any(|mode| mode == PassMode::Move)
            {
                self.error(
                    "a recurring async `while` with move-only condition state requires generated `Continue(Carry)` transfer, which is not lowered yet",
                );
                return super::lower::error_expr();
            }
            if closure.unsafe_effect
                || closure.throws_error.is_some()
                || !closure.custom_effects.is_empty()
            {
                self.error(
                    "a recurring async `while` condition with residual effects requires poll/resume handler specialization, which is not implemented yet",
                );
                return super::lower::error_expr();
            }
            awaited.loop_condition = Some(AsyncLoopConditionInfo {
                function: closure.function,
                post_test: condition.post_test,
                capture_modes: closure.captures.iter().map(capture_pass_mode).collect(),
                fields: Vec::new(),
                capture_types: Vec::new(),
            });
            loop_condition_captures = closure.captures;
        }
        let mut continuation_captures = Vec::new();
        let mut loop_carry_capture_indices = Vec::new();
        let mut continuation_residual_throws = None;
        let mut continuation_residual_custom = Vec::new();
        if let (Some(awaited), Some(continuation)) = (awaited.as_mut(), source_plan.continuation) {
            if continuation.mutable {
                self.error("an await result used after suspension cannot be mutable yet");
                return super::lower::error_expr();
            }
            let Some(source_ty) = self.source_type_for_ty(&awaited.output) else {
                self.error(format!(
                    "await output type `{}` cannot be represented in a continuation parameter",
                    awaited.output
                ));
                return super::lower::error_expr();
            };
            let mut parameters = Vec::new();
            for ((retained, retained_ty), mode) in source_plan
                .retained
                .iter()
                .zip(&awaited.retained_types)
                .zip(&awaited.retained_modes)
            {
                let Some(source_ty) = self.source_type_for_ty(retained_ty) else {
                    self.error(format!(
                        "async local `{}` of type `{retained_ty}` cannot be represented in a continuation parameter",
                        retained.name
                    ));
                    return super::lower::error_expr();
                };
                parameters.push(Param {
                    mode: *mode,
                    access: None,
                    modifiers: Vec::new(),
                    region: None,
                    name: retained.name.clone(),
                    ty: source_ty,
                });
            }
            parameters.push(Param {
                mode: PassMode::Inferred,
                access: None,
                modifiers: Vec::new(),
                region: None,
                name: continuation.name,
                ty: source_ty,
            });
            let continuation_has_await = split_async_source(&continuation.body).has_await;
            let continuation_source_body = continuation.body.clone();
            let continuation_body = if continuation_has_await {
                Expr::Async {
                    body: Box::new(continuation.body),
                }
            } else {
                continuation.body
            };
            let lowered = self.lower_local_closure(
                &parameters,
                &continuation_body,
                loop_step.as_ref().map(|step| step.ty.clone()),
                ClosureEffectContext {
                    infer_effects: true,
                    ..ClosureEffectContext::default()
                },
                ClosureCapturePolicy::AsyncOwned,
                context,
            );
            let HirExprKind::LocalClosure(closure) = lowered.kind else {
                return lowered;
            };
            if loop_step.is_some() {
                for (name, capture) in closure.capture_names.iter().zip(&closure.captures) {
                    if capture_pass_mode(capture) == PassMode::Move
                        && !loop_carry_names.contains(name)
                    {
                        self.error(format!(
                            "move-only async loop capture `{name}` is not available on every `continue` path"
                        ));
                        return super::lower::error_expr();
                    }
                }
                for name in &loop_carry_names {
                    let Some(index) = closure
                        .capture_names
                        .iter()
                        .position(|capture| capture == name)
                    else {
                        self.error(format!(
                            "move-only async loop carry `{name}` is not available to its continuation"
                        ));
                        return super::lower::error_expr();
                    };
                    loop_carry_capture_indices.push(index);
                }
            }
            let async_effect = self.lang_item_name(LangItemKind::AsyncEffect);
            let has_residual_continuation = closure.throws_error.is_some()
                || closure
                    .custom_effects
                    .iter()
                    .any(|effect| effect != async_effect);
            if has_residual_continuation && continuation_has_await {
                self.error(
                    "an async continuation that both suspends and retains residual Throws or algebraic effects requires later-child poll specialization, which is not implemented yet",
                );
                return super::lower::error_expr();
            }
            if has_residual_continuation {
                continuation_residual_throws = closure.throws_error.clone();
                continuation_residual_custom.extend(
                    closure
                        .custom_effects
                        .iter()
                        .filter(|effect| effect.as_str() != async_effect)
                        .cloned(),
                );
                let mut source_parameters = closure
                    .capture_names
                    .iter()
                    .zip(&closure.captures)
                    .map(|(name, capture)| {
                        let mode = capture_pass_mode(capture);
                        let ty = &capture.place.ty;
                        Some(Param {
                            mode,
                            access: None,
                            modifiers: Vec::new(),
                            region: None,
                            name: name.clone(),
                            ty: self.source_type_for_ty(
                                if matches!(mode, PassMode::Borrow | PassMode::MutBorrow) {
                                    match ty {
                                        Ty::Reference { pointee, .. } => pointee,
                                        ty => ty,
                                    }
                                } else {
                                    ty
                                },
                            )?,
                        })
                    })
                    .collect::<Option<Vec<_>>>();
                let Some(mut source_parameters) = source_parameters.take() else {
                    self.error(
                        "async continuation captures cannot be represented in a residual source helper",
                    );
                    return super::lower::error_expr();
                };
                source_parameters.extend(parameters.clone());
                let Some(result) = self.source_type_for_ty(&closure.result) else {
                    self.error(
                        "async continuation output cannot be represented in a residual source helper",
                    );
                    return super::lower::error_expr();
                };
                awaited.residual_continuation = Some(AsyncResidualContinuationInfo {
                    function: closure.function.clone(),
                    body: continuation_source_body,
                    parameters: source_parameters,
                    result,
                    effects: FunctionEffects {
                        unsafe_effect: closure.unsafe_effect,
                        throws: closure
                            .throws_error
                            .as_ref()
                            .and_then(|error| self.source_type_for_ty(error))
                            .map(Box::new),
                        custom: closure
                            .custom_effects
                            .iter()
                            .filter(|effect| effect.as_str() != async_effect)
                            .filter_map(|effect| source_type_from_identity(effect))
                            .collect(),
                        parameters: Vec::new(),
                    },
                });
            }
            awaited.continuation = Some(closure.function);
            awaited.continuation_output = Some(closure.result.clone());
            awaited.continuation_unsafe_effect = closure.unsafe_effect;
            if continuation_has_await {
                let Some(next) = self.resolve_awaited_future(&closure.result) else {
                    return super::lower::error_expr();
                };
                awaited.next = Some(Box::new(next));
            }
            continuation_captures = closure.captures;
        }
        if let Some(error) = continuation_residual_throws {
            if closure
                .throws_error
                .as_ref()
                .is_some_and(|existing| existing != &error)
            {
                self.error("async segments retain incompatible `Throws` error types in one future");
                return super::lower::error_expr();
            }
            closure.throws_error = Some(error);
        }
        for effect in continuation_residual_custom {
            if !closure.custom_effects.contains(&effect) {
                closure.custom_effects.push(effect);
            }
        }
        let unsupported_effects = closure
            .custom_effects
            .iter()
            .filter(|effect| *effect != &async_effect)
            .cloned()
            .collect::<Vec<_>>();
        let throws_name = self.lang_item_name(LangItemKind::ThrowsEffect);
        let residual_throws = unsupported_effects
            .iter()
            .filter_map(|effect| source_type_from_identity(effect))
            .filter_map(|effect| standard_throws_error_source(&effect, throws_name))
            .filter_map(|error| self.probe_source_ty(&error))
            .collect::<Vec<_>>();
        let algebraic_effects = unsupported_effects
            .iter()
            .filter(|effect| {
                source_type_from_identity(effect).is_none_or(|source| {
                    standard_throws_error_source(&source, throws_name).is_none()
                })
            })
            .cloned()
            .collect::<Vec<_>>();
        let has_residual_effects =
            closure.throws_error.is_some() || !unsupported_effects.is_empty();

        let supports_suspended_residual = awaited.as_ref().is_some_and(|awaited| {
            let heterogeneous_branch = matches!(
                &awaited.ty,
                Ty::Enum(name) if name.starts_with("$async$branch$")
            );
            awaited.loop_step.is_none()
                && awaited.loop_condition.is_none()
                && (!heterogeneous_branch
                    || match &awaited.ty {
                        Ty::Enum(name) => self.enum_layouts.get(name).is_some_and(|layout| {
                            heterogeneous_branch_factory(
                                &source_plan.factory_body,
                                layout.variants.len(),
                            )
                            .is_some()
                        }),
                        _ => false,
                    })
                && closure
                    .captures
                    .iter()
                    .map(capture_pass_mode)
                    .all(|mode| mode != PassMode::Inferred)
                && continuation_captures
                    .iter()
                    .map(capture_pass_mode)
                    .all(|mode| mode != PassMode::Inferred)
        });
        if has_residual_effects && source_plan.has_await && !supports_suspended_residual {
            if closure.throws_error.is_some() || !residual_throws.is_empty() {
                let errors = closure
                    .throws_error
                    .iter()
                    .chain(&residual_throws)
                    .map(|error| self.diagnostic_type_name(error))
                    .collect::<Vec<_>>();
                self.error(format!(
                    "async residual `Throws({})` requires poll/resume handler specialization for this suspension shape, which is not implemented yet",
                    errors.join(" | ")
                ));
            }
            if !algebraic_effects.is_empty() {
                self.error(format!(
                    "async residual algebraic effect{} `{}` {} poll/resume handler specialization for this suspension shape, which is not implemented yet",
                    if algebraic_effects.len() == 1 { "" } else { "s" },
                    algebraic_effects.join(", "),
                    if algebraic_effects.len() == 1 {
                        "requires"
                    } else {
                        "require"
                    }
                ));
            }
            return super::lower::error_expr();
        }

        let name = format!("$async$state${}", self.next_async_future);
        self.next_async_future += 1;
        let access = AccessBoundary {
            visibility: Visibility::Private,
            origin: self
                .current_origin
                .as_deref()
                .cloned()
                .unwrap_or_else(ItemOrigin::default),
        };
        let mut fields = vec![FieldLayout {
            name: "state".to_owned(),
            ty: Ty::I32,
            access: access.clone(),
        }];
        let mut values = vec![(
            0,
            HirExpr {
                ty: Ty::I32,
                kind: HirExprKind::Integer(0),
            },
        )];

        let direct_reference_captures =
            has_residual_effects && (!source_plan.has_await || supports_suspended_residual);
        for (index, capture) in closure.captures.iter().enumerate() {
            let (ty, value) = materialize_async_capture(capture, direct_reference_captures);
            fields.push(FieldLayout {
                name: format!("capture.{index}"),
                ty,
                access: access.clone(),
            });
            values.push((index + 1, value));
        }
        let mut continuation_fields = Vec::new();
        let mut continuation_capture_types = Vec::new();
        for (index, capture) in continuation_captures.iter().enumerate() {
            let (ty, value) = materialize_async_capture(capture, false);
            let field = fields.len();
            fields.push(FieldLayout {
                name: format!("continuation.capture.{index}"),
                ty: ty.clone(),
                access: access.clone(),
            });
            values.push((field, value));
            continuation_fields.push(field);
            continuation_capture_types.push(ty);
        }
        let mut loop_condition_fields = Vec::new();
        let mut loop_condition_capture_types = Vec::new();
        for (index, capture) in loop_condition_captures.iter().enumerate() {
            let (ty, value) = materialize_async_capture(capture, false);
            let field = fields.len();
            fields.push(FieldLayout {
                name: format!("loop.condition.capture.{index}"),
                ty: ty.clone(),
                access: access.clone(),
            });
            values.push((field, value));
            loop_condition_fields.push(field);
            loop_condition_capture_types.push(ty);
        }
        if let Some(awaited) = awaited.as_mut() {
            awaited.loop_carry_fields = loop_carry_capture_indices
                .iter()
                .map(|index| continuation_fields[*index])
                .collect();
            awaited.loop_carry_types = loop_carry_types.clone();
            awaited.continuation_capture_modes = continuation_captures
                .iter()
                .map(capture_pass_mode)
                .collect();
            awaited.continuation_fields = continuation_fields;
            awaited.continuation_capture_types = continuation_capture_types;
            if let Some(condition) = awaited.loop_condition.as_mut() {
                condition.fields = loop_condition_fields;
                condition.capture_types = loop_condition_capture_types;
            }
            for (index, ty) in awaited.retained_types.iter().enumerate() {
                let field = fields.len();
                fields.push(FieldLayout {
                    name: format!("retained.{index}"),
                    ty: ty.clone(),
                    access: access.clone(),
                });
                awaited.retained_fields.push(field);
            }
        }
        if let Some(next) = awaited.as_mut().and_then(|awaited| awaited.next.as_mut()) {
            let field = fields.len();
            fields.push(FieldLayout {
                name: "awaited.next".to_owned(),
                ty: next.ty.clone(),
                access: access.clone(),
            });
            next.field = field;
        }
        let awaited_field = awaited.as_ref().map(|awaited| {
            let field = fields.len();
            fields.push(FieldLayout {
                name: "awaited".to_owned(),
                ty: awaited.ty.clone(),
                access: access.clone(),
            });
            field
        });

        self.nominal_accesses.insert(name.clone(), access);
        self.struct_layouts.insert(
            name.clone(),
            StructLayout {
                name: name.clone(),
                source_name: name.clone(),
                representation: crate::ast::StructRepresentation::Salicin,
                fields,
            },
        );
        self.struct_order.push(name.clone());
        let output = awaited
            .as_ref()
            .map(|awaited| {
                awaited.loop_step.as_ref().map_or_else(
                    || {
                        awaited.next.as_ref().map_or_else(
                            || {
                                awaited
                                    .continuation_output
                                    .clone()
                                    .unwrap_or_else(|| awaited.output.clone())
                            },
                            |next| next.output.clone(),
                        )
                    },
                    |step| step.output.clone(),
                )
            })
            .unwrap_or_else(|| closure.result.clone());
        let unsafe_effect = closure.unsafe_effect
            || awaited.as_ref().is_some_and(|awaited| {
                awaited.unsafe_effect
                    || awaited.continuation_unsafe_effect
                    || awaited.next.as_ref().is_some_and(|next| next.unsafe_effect)
            });
        let resume_function = closure.function.clone();
        let resume_captures = closure
            .capture_names
            .iter()
            .cloned()
            .zip(&closure.captures)
            .map(|(name, capture)| (name, capture.place.ty.clone(), capture_pass_mode(capture)))
            .collect::<Vec<_>>();
        let metadata = AsyncFutureInfo {
            resume: closure.function,
            output,
            unsafe_effect,
            throws_error: closure.throws_error,
            custom_effects: closure.custom_effects,
            capture_modes: closure.captures.iter().map(capture_pass_mode).collect(),
            awaited: awaited.map(|mut awaited| {
                awaited.field = awaited_field.expect("awaited state has a field");
                awaited
            }),
        };
        self.async_futures.insert(name.clone(), metadata);
        self.register_future_poll(&name);
        if has_residual_effects && source_plan.has_await {
            self.register_suspended_async_handler_templates(
                &name,
                &source_plan.factory_body,
                &resume_function,
                &resume_captures,
            );
        } else if has_residual_effects {
            self.register_ready_async_handler_templates(
                &name,
                &source_plan.factory_body,
                &resume_function,
                &resume_captures,
            );
        }

        HirExpr {
            ty: Ty::Struct(name.clone()),
            kind: HirExprKind::ConstructStruct {
                name,
                fields: values,
            },
        }
    }

    fn resolve_awaited_future(&mut self, ty: &Ty) -> Option<AwaitedFutureInfo> {
        let origin = self
            .current_origin
            .as_deref()
            .cloned()
            .unwrap_or_else(ItemOrigin::default);
        let future_trait = self.lang_item_name(LangItemKind::Future);
        let mut candidates = self
            .trait_method_candidates(ty, "poll", &origin)
            .into_iter()
            .filter(|candidate| candidate.trait_ref.name == future_trait)
            .collect::<Vec<_>>();
        if candidates.len() != 1 {
            self.error(if candidates.is_empty() {
                format!("await operand of type `{ty}` does not implement `Future`")
            } else {
                format!(
                    "await operand of type `{ty}` has multiple `Future` implementations; the residual effect row is ambiguous"
                )
            });
            return None;
        }
        let candidate = candidates.pop().expect("one future candidate");
        let implementation = self
            .trait_impls
            .get(&candidate)
            .cloned()
            .expect("trait method candidate has an implementation");
        let output = implementation
            .associated_types
            .get("Output")
            .cloned()
            .expect("validated Future implementation has Output");
        let poll_function = implementation
            .methods
            .get("poll")
            .cloned()
            .expect("validated Future implementation has poll");
        let signature = self
            .signatures
            .get(&poll_function)
            .cloned()
            .expect("registered Future poll has a signature");
        if signature.throws_error.is_some() || !signature.custom_effects.is_empty() {
            self.error(
                "await residual Throws and algebraic effects require poll/resume handler specialization, which is not implemented yet",
            );
            return None;
        }
        let poll_ty = signature
            .result
            .clone()
            .expect("validated Future poll has a result");
        Some(AwaitedFutureInfo {
            ty: ty.clone(),
            factory_output: ty.clone(),
            output,
            poll_ty,
            poll_function,
            unsafe_effect: signature.unsafe_effect,
            field: 0,
            continuation: None,
            continuation_output: None,
            continuation_unsafe_effect: false,
            continuation_capture_modes: Vec::new(),
            continuation_fields: Vec::new(),
            continuation_capture_types: Vec::new(),
            retained_fields: Vec::new(),
            retained_types: Vec::new(),
            retained_modes: Vec::new(),
            next: None,
            loop_step: None,
            loop_condition: None,
            loop_carry_fields: Vec::new(),
            loop_carry_types: Vec::new(),
            residual_continuation: None,
        })
    }

    pub(super) fn register_async_branch_future(&mut self, branch_types: &[Ty]) -> Option<String> {
        if self.async_factory_depth == 0 || branch_types.len() < 2 {
            return None;
        }
        let origin = self
            .current_origin
            .as_deref()
            .cloned()
            .unwrap_or_else(ItemOrigin::default);
        let future_trait = self.lang_item_name(LangItemKind::Future).to_owned();
        if branch_types.iter().any(|ty| {
            self.trait_method_candidates(ty, "poll", &origin)
                .into_iter()
                .filter(|candidate| candidate.trait_ref.name == future_trait)
                .count()
                != 1
        }) {
            return None;
        }
        let futures = branch_types
            .iter()
            .map(|ty| self.resolve_awaited_future(ty))
            .collect::<Option<Vec<_>>>()?;
        let output = futures.first()?.output.clone();
        if futures.iter().any(|future| future.output != output) {
            self.error("control-flow await branches must produce the same `Future.Output` type");
            return None;
        }
        let poll_ty = futures.first()?.poll_ty.clone();
        if futures.iter().any(|future| future.poll_ty != poll_ty) {
            self.error("control-flow await branches resolved incompatible `Poll` result types");
            return None;
        }

        let name = format!("$async$branch${}", self.next_async_future);
        self.next_async_future += 1;
        let access = AccessBoundary {
            visibility: Visibility::Private,
            origin,
        };
        let mut payload_offset = 0;
        let variants = branch_types
            .iter()
            .enumerate()
            .map(|(index, ty)| {
                let variant = VariantLayout {
                    name: format!("Branch{index}"),
                    fields: vec![FieldLayout {
                        name: "future".to_owned(),
                        ty: ty.clone(),
                        access: access.clone(),
                    }],
                    payload_offset,
                    named: false,
                };
                payload_offset += 1;
                variant
            })
            .collect::<Vec<_>>();
        self.enum_layouts.insert(
            name.clone(),
            EnumLayout {
                name: name.clone(),
                variants: variants.clone(),
            },
        );
        self.enum_order.push(name.clone());
        self.nominal_accesses.insert(name.clone(), access.clone());

        let self_ty = Ty::Enum(name.clone());
        let self_reference = Ty::Reference {
            pointee: Box::new(self_ty.clone()),
            mutable: true,
            region: None,
        };
        let unsafe_effect = futures.iter().any(|future| future.unsafe_effect);
        let effect_row = Ty::EffectRow {
            unsafe_effect,
            throws_error: None,
            custom_effects: Vec::new(),
        };
        let trait_key = TraitImplKey {
            self_ty: self_ty.clone(),
            trait_ref: TraitRefKey {
                name: future_trait,
                arguments: vec![effect_row],
            },
        };
        let poll_function = trait_method_name(&trait_key, "poll");
        self.signatures.insert(
            poll_function.clone(),
            FunctionSig {
                groups: vec![
                    vec![ParamSig {
                        name: "self".to_owned(),
                        ty: self_reference.clone(),
                        mode: PassMode::Inferred,
                    }],
                    Vec::new(),
                ],
                unsafe_effect,
                throws_error: None,
                custom_effects: Vec::new(),
                result: Some(poll_ty.clone()),
            },
        );
        self.function_accesses
            .insert(poll_function.clone(), access.clone());
        self.inherent_members
            .entry(name.clone())
            .or_default()
            .methods
            .insert("poll".to_owned(), poll_function.clone());
        self.trait_impl_headers.insert(trait_key.clone());
        self.trait_methods_by_receiver
            .entry((self_ty.clone(), "poll".to_owned()))
            .or_default()
            .push(trait_key.clone());
        let output_source = self.source_type_for_ty(&output)?;
        self.trait_impls.insert(
            trait_key.clone(),
            TraitImplInfo {
                key: trait_key,
                associated_types: HashMap::from([("Output".to_owned(), output.clone())]),
                associated_type_sources: HashMap::from([("Output".to_owned(), output_source)]),
                methods: HashMap::from([("poll".to_owned(), poll_function.clone())]),
                access,
            },
        );

        let scrutinee_place = HirPlace {
            local: 0,
            root_ty: self_ty.clone(),
            projections: Vec::new(),
            dynamic_index: None,
            ty: self_ty.clone(),
            capability: LocalCapability::MutParam,
            root_mutable: true,
            loan: None,
            indirect: true,
        };
        let arms = futures
            .iter()
            .zip(&variants)
            .enumerate()
            .map(|(index, (future, variant))| {
                let child_place = HirPlace {
                    local: 0,
                    root_ty: self_ty.clone(),
                    projections: vec![1 + variant.payload_offset],
                    dynamic_index: None,
                    ty: future.ty.clone(),
                    capability: LocalCapability::MutParam,
                    root_mutable: true,
                    loan: None,
                    indirect: true,
                };
                HirMatchArm {
                    matcher: HirMatcher::Variant(index),
                    bindings: Vec::new(),
                    guard: None,
                    body: HirExpr {
                        ty: poll_ty.clone(),
                        kind: HirExprKind::Call {
                            function: future.poll_function.clone(),
                            arguments: vec![HirArgument::Copy(HirExpr {
                                ty: Ty::Reference {
                                    pointee: Box::new(future.ty.clone()),
                                    mutable: true,
                                    region: None,
                                },
                                kind: HirExprKind::Borrow {
                                    place: child_place,
                                    mutable: true,
                                },
                            })],
                            consumed_callable: None,
                            diverges: false,
                        },
                    },
                }
            })
            .collect();
        self.lifted_functions.push(HirFunction {
            name: poll_function,
            params: vec![HirParam {
                id: 0,
                name: "self".to_owned(),
                ty: self_reference.clone(),
                mode: PassMode::Inferred,
            }],
            result: poll_ty.clone(),
            body: HirExpr {
                ty: poll_ty,
                kind: HirExprKind::Match {
                    scrutinee: Box::new(HirExpr {
                        ty: self_ty,
                        kind: HirExprKind::Read {
                            place: scrutinee_place,
                            kind: HirReadKind::Inspect,
                        },
                    }),
                    arms,
                },
            },
        });
        Some(name)
    }

    pub(super) fn register_async_loop_step(
        &mut self,
        carry: Ty,
        output: Ty,
        continue_constructor: String,
        break_constructor: String,
    ) -> AsyncLoopStepInfo {
        let name = format!("$async$loop$step${}", self.next_async_future);
        self.next_async_future += 1;
        let access = AccessBoundary {
            visibility: Visibility::Private,
            origin: self
                .current_origin
                .as_deref()
                .cloned()
                .unwrap_or_else(ItemOrigin::default),
        };
        self.enum_layouts.insert(
            name.clone(),
            EnumLayout {
                name: name.clone(),
                variants: vec![
                    VariantLayout {
                        name: "Continue".to_owned(),
                        fields: vec![FieldLayout {
                            name: "carry".to_owned(),
                            ty: carry.clone(),
                            access: access.clone(),
                        }],
                        payload_offset: 0,
                        named: false,
                    },
                    VariantLayout {
                        name: "Break".to_owned(),
                        fields: vec![FieldLayout {
                            name: "output".to_owned(),
                            ty: output.clone(),
                            access: access.clone(),
                        }],
                        payload_offset: 1,
                        named: false,
                    },
                ],
            },
        );
        self.enum_order.push(name.clone());
        self.nominal_accesses.insert(name.clone(), access);
        let ty = Ty::Enum(name.clone());
        self.internal_async_loop_constructors.insert(
            continue_constructor.clone(),
            InternalAsyncLoopConstructor {
                name: name.clone(),
                ty: ty.clone(),
                variant: 0,
                field: carry.clone(),
            },
        );
        self.internal_async_loop_constructors.insert(
            break_constructor.clone(),
            InternalAsyncLoopConstructor {
                name: name.clone(),
                ty: ty.clone(),
                variant: 1,
                field: output.clone(),
            },
        );
        AsyncLoopStepInfo { ty, carry, output }
    }

    pub(super) fn lower_internal_async_loop_constructor(
        &mut self,
        expression: &Expr,
        context: &mut super::flow::LowerCtx,
    ) -> Option<HirExpr> {
        let Expr::Call(callee, arguments) = expression.unlocated() else {
            return None;
        };
        let Expr::Name(constructor) = callee.unlocated() else {
            return None;
        };
        let constructor = self
            .internal_async_loop_constructors
            .get(constructor)
            .cloned()?;
        let [argument] = arguments.as_slice() else {
            self.error("internal async loop step constructor received an invalid argument shape");
            return Some(super::lower::error_expr());
        };
        if argument.label.is_some() {
            self.error("internal async loop step constructor received a labeled argument");
            return Some(super::lower::error_expr());
        }
        let value = self.lower_expr(&argument.value, Some(&constructor.field), context);
        Some(HirExpr {
            ty: constructor.ty,
            kind: HirExprKind::ConstructEnum {
                name: constructor.name,
                variant: constructor.variant,
                fields: vec![(0, value)],
            },
        })
    }

    fn register_future_poll(&mut self, name: &str) {
        let Some(future) = self.async_futures.get(name).cloned() else {
            return;
        };
        let Some(output_source) = self.source_type_for_ty(&future.output) else {
            self.error(format!(
                "async output type `{}` cannot be represented in the source type system",
                future.output
            ));
            return;
        };
        let poll_template = self.lang_item_name(LangItemKind::Poll).to_owned();
        let Some(poll_name) = self.ensure_nominal_instance(
            NominalKind::Enum,
            &poll_template,
            vec![output_source.clone()],
            vec![future.output.clone()],
        ) else {
            return;
        };
        let poll_ty = Ty::Enum(poll_name.clone());
        let self_ty = Ty::Struct(name.to_owned());
        let self_reference = Ty::Reference {
            pointee: Box::new(self_ty.clone()),
            mutable: true,
            region: None,
        };
        let async_effect = self.lang_item_name(LangItemKind::AsyncEffect);
        let residual_custom_effects = future
            .custom_effects
            .iter()
            .filter(|effect| effect.as_str() != async_effect)
            .cloned()
            .collect::<Vec<_>>();
        let effect_row = Ty::EffectRow {
            unsafe_effect: future.unsafe_effect,
            throws_error: future.throws_error.clone().map(Box::new),
            custom_effects: residual_custom_effects.clone(),
        };
        let trait_key = TraitImplKey {
            self_ty: self_ty.clone(),
            trait_ref: TraitRefKey {
                name: self.lang_item_name(LangItemKind::Future).to_owned(),
                arguments: vec![effect_row],
            },
        };
        let poll_function = trait_method_name(&trait_key, "poll");
        let receiver = ParamSig {
            name: "self".to_owned(),
            ty: self_reference.clone(),
            mode: PassMode::Inferred,
        };
        self.signatures.insert(
            poll_function.clone(),
            FunctionSig {
                groups: vec![vec![receiver], Vec::new()],
                unsafe_effect: future.unsafe_effect,
                throws_error: future.throws_error.clone(),
                custom_effects: residual_custom_effects,
                result: Some(poll_ty.clone()),
            },
        );
        self.function_accesses
            .insert(poll_function.clone(), self.nominal_accesses[name].clone());
        self.inherent_members
            .entry(name.to_owned())
            .or_default()
            .methods
            .insert("poll".to_owned(), poll_function.clone());
        self.trait_impl_headers.insert(trait_key.clone());
        self.trait_methods_by_receiver
            .entry((self_ty.clone(), "poll".to_owned()))
            .or_default()
            .push(trait_key.clone());
        self.trait_impls.insert(
            trait_key.clone(),
            TraitImplInfo {
                key: trait_key,
                associated_types: HashMap::from([("Output".to_owned(), future.output.clone())]),
                associated_type_sources: HashMap::from([("Output".to_owned(), output_source)]),
                methods: HashMap::from([("poll".to_owned(), poll_function.clone())]),
                access: self.nominal_accesses[name].clone(),
            },
        );

        let layout = self.struct_layouts[name].clone();
        let arguments = future
            .capture_modes
            .iter()
            .enumerate()
            .map(|(capture, mode)| {
                let field = capture + 1;
                let ty = layout.fields[field].ty.clone();
                let place = async_field_place(0, self_ty.clone(), field, ty.clone());
                match mode {
                    PassMode::Borrow | PassMode::MutBorrow | PassMode::Copy => {
                        HirArgument::Copy(HirExpr {
                            ty,
                            kind: HirExprKind::Read {
                                place,
                                kind: HirReadKind::Copy,
                            },
                        })
                    }
                    PassMode::Move => HirArgument::Move(HirExpr {
                        ty: ty.clone(),
                        kind: HirExprKind::RawTake(Box::new(HirExpr {
                            ty: Ty::Pointer {
                                pointee: Box::new(ty),
                                mutable: true,
                            },
                            kind: HirExprKind::RawAddress { place },
                        })),
                    }),
                    PassMode::Inferred => {
                        unreachable!("async capture modes are normalized while materializing state")
                    }
                }
            })
            .collect::<Vec<_>>();
        let resume = HirExpr {
            ty: future.awaited.as_ref().map_or_else(
                || future.output.clone(),
                |awaited| awaited.factory_output.clone(),
            ),
            kind: HirExprKind::Call {
                function: future.resume.clone(),
                arguments,
                consumed_callable: None,
                diverges: future.awaited.is_none() && future.output == Ty::Never,
            },
        };
        let loop_condition = future.awaited.as_ref().and_then(|awaited| {
            awaited.loop_condition.as_ref().map(|condition| {
                let arguments = condition
                    .fields
                    .iter()
                    .zip(&condition.capture_types)
                    .zip(&condition.capture_modes)
                    .map(|((&field, ty), mode)| {
                        let place = async_field_place(0, self_ty.clone(), field, ty.clone());
                        match mode {
                            PassMode::Borrow | PassMode::MutBorrow | PassMode::Copy => {
                                HirArgument::Copy(HirExpr {
                                    ty: ty.clone(),
                                    kind: HirExprKind::Read {
                                        place,
                                        kind: HirReadKind::Copy,
                                    },
                                })
                            }
                            PassMode::Move => {
                                unreachable!("move-only async loop condition captures are rejected")
                            }
                            PassMode::Inferred => {
                                unreachable!("async condition capture modes are normalized")
                            }
                        }
                    })
                    .collect();
                HirExpr {
                    ty: Ty::Bool,
                    kind: HirExprKind::Call {
                        function: condition.function.clone(),
                        arguments,
                        consumed_callable: None,
                        diverges: false,
                    },
                }
            })
        });
        let body = if let Some(awaited) = &future.awaited {
            suspended_poll_body(
                &self_ty,
                &poll_ty,
                &poll_name,
                &future.output,
                awaited,
                resume,
                loop_condition,
            )
        } else {
            ready_poll_body(&self_ty, &poll_ty, &poll_name, &future.output, resume)
        };
        self.lifted_functions.push(HirFunction {
            name: poll_function,
            params: vec![HirParam {
                id: 0,
                name: "self".to_owned(),
                ty: self_reference,
                mode: PassMode::Inferred,
            }],
            result: poll_ty,
            body,
        });
    }

    fn register_ready_async_handler_templates(
        &mut self,
        name: &str,
        resume_body: &Expr,
        resume_function: &str,
        resume_captures: &[(String, Ty, PassMode)],
    ) {
        let future = self.async_futures[name].clone();
        debug_assert!(future.awaited.is_none());
        debug_assert!(future
            .capture_modes
            .iter()
            .all(|mode| *mode != PassMode::Inferred));
        let Some(output_source) = self.source_type_for_ty(&future.output) else {
            return;
        };
        let Some(resume_parameters) = resume_captures
            .iter()
            .map(|(name, ty, mode)| {
                Some(Param {
                    mode: *mode,
                    access: None,
                    modifiers: Vec::new(),
                    region: None,
                    name: name.clone(),
                    ty: self.source_type_for_ty(
                        if matches!(mode, PassMode::Borrow | PassMode::MutBorrow) {
                            match ty {
                                Ty::Reference { pointee, .. } => pointee,
                                ty => ty,
                            }
                        } else {
                            ty
                        },
                    )?,
                })
            })
            .collect::<Option<Vec<_>>>()
        else {
            return;
        };
        let effects = self.async_source_effects(&future);
        let origin = self.nominal_accesses[name].origin.clone();
        self.functions.insert(
            resume_function.to_owned(),
            Function {
                name: resume_function.to_owned(),
                foreign: None,
                builtin: false,
                compile_groups: Vec::new(),
                groups: vec![resume_parameters],
                return_type: Some(output_source.clone()),
                effects: effects.clone(),
                where_predicates: Vec::new(),
                body: Some(resume_body.clone()),
            },
        );
        self.function_origins
            .insert(resume_function.to_owned(), origin.clone());

        let effect_row = Ty::EffectRow {
            unsafe_effect: future.unsafe_effect,
            throws_error: future.throws_error.clone().map(Box::new),
            custom_effects: future
                .custom_effects
                .iter()
                .filter(|effect| effect.as_str() != self.lang_item_name(LangItemKind::AsyncEffect))
                .cloned()
                .collect(),
        };
        let trait_key = TraitImplKey {
            self_ty: Ty::Struct(name.to_owned()),
            trait_ref: TraitRefKey {
                name: self.lang_item_name(LangItemKind::Future).to_owned(),
                arguments: vec![effect_row],
            },
        };
        let poll_function = trait_method_name(&trait_key, "poll");
        self.lifted_functions
            .retain(|function| function.name != resume_function && function.name != poll_function);
        let resume = Expr::Call(
            Box::new(Expr::Name(resume_function.to_owned())),
            resume_captures
                .iter()
                .enumerate()
                .map(|(index, (_, _, mode))| {
                    let field = Expr::Member(
                        Box::new(Expr::Name("self".to_owned())),
                        format!("capture.{index}"),
                    );
                    CallArg {
                        label: None,
                        value: if matches!(mode, PassMode::Borrow | PassMode::MutBorrow) {
                            Expr::Call(
                                Box::new(Expr::Name("$async$copy$stored$borrow".to_owned())),
                                vec![CallArg {
                                    label: None,
                                    value: field,
                                }],
                            )
                        } else {
                            field
                        },
                    }
                })
                .collect(),
        );
        let poll_type = Expr::Call(
            Box::new(Expr::Name(
                self.lang_item_name(LangItemKind::Poll).to_owned(),
            )),
            vec![CallArg {
                label: None,
                value: source_type_expression(&output_source),
            }],
        );
        let ready = Expr::Call(
            Box::new(Expr::Member(Box::new(poll_type), "Ready".to_owned())),
            vec![CallArg {
                label: None,
                value: resume,
            }],
        );
        let self_value = || Expr::Name("self".to_owned());
        let state = || Expr::Member(Box::new(self_value()), "state".to_owned());
        let body = Expr::If {
            condition: Box::new(Expr::Binary(
                Box::new(state()),
                BinaryOp::Eq,
                Box::new(Expr::Integer(0)),
            )),
            then_branch: Box::new(Expr::Block(
                vec![Stmt::Expr(Expr::Assign(
                    Box::new(state()),
                    Box::new(Expr::Integer(1)),
                ))],
                Some(Box::new(ready)),
            )),
            else_branch: Some(Box::new(Expr::Loop {
                body: Box::new(Expr::Unit),
            })),
        };
        self.functions.insert(
            poll_function.clone(),
            Function {
                name: poll_function.clone(),
                foreign: None,
                builtin: false,
                compile_groups: Vec::new(),
                groups: vec![
                    vec![Param {
                        mode: PassMode::Inferred,
                        access: None,
                        modifiers: Vec::new(),
                        region: None,
                        name: "self".to_owned(),
                        ty: Type::Borrow {
                            mutable: true,
                            access: None,
                            region: None,
                            pointee: Box::new(Type::Named(name.to_owned(), Vec::new())),
                        },
                    }],
                    Vec::new(),
                ],
                return_type: Some(Type::Named(
                    self.lang_item_name(LangItemKind::Poll).to_owned(),
                    vec![output_source],
                )),
                effects,
                where_predicates: Vec::new(),
                body: Some(body),
            },
        );
        self.function_origins.insert(poll_function, origin);
    }

    fn register_suspended_async_handler_templates(
        &mut self,
        name: &str,
        resume_body: &Expr,
        resume_function: &str,
        resume_captures: &[(String, Ty, PassMode)],
    ) {
        let future = self.async_futures[name].clone();
        let awaited = future
            .awaited
            .as_ref()
            .expect("suspended async template has an awaited child");
        debug_assert!(awaited.loop_step.is_none());
        debug_assert!(awaited.loop_condition.is_none());

        let Some(output_source) = self.source_type_for_ty(&future.output) else {
            return;
        };
        let branch_factory = match &awaited.ty {
            Ty::Enum(branch_name) if branch_name.starts_with("$async$branch$") => {
                self.enum_layouts.get(branch_name).and_then(|layout| {
                    heterogeneous_branch_factory(resume_body, layout.variants.len()).and_then(
                        |(prefix, selection, retained)| {
                            if retained.len() != awaited.retained_types.len() {
                                return None;
                            }
                            Some((
                                prefix,
                                selection,
                                retained,
                                layout
                                    .variants
                                    .iter()
                                    .map(|variant| {
                                        variant
                                            .fields
                                            .first()
                                            .expect("async branch variant has a child")
                                            .ty
                                            .clone()
                                    })
                                    .collect::<Vec<_>>(),
                            ))
                        },
                    )
                })
            }
            _ => None,
        };
        let factory_output_source = self.source_type_for_ty(&awaited.factory_output);
        if branch_factory.is_none() && factory_output_source.is_none() {
            return;
        }
        let Some(resume_parameters) = resume_captures
            .iter()
            .map(|(name, ty, mode)| {
                Some(Param {
                    mode: *mode,
                    access: None,
                    modifiers: Vec::new(),
                    region: None,
                    name: name.clone(),
                    ty: self.source_type_for_ty(
                        if matches!(mode, PassMode::Borrow | PassMode::MutBorrow) {
                            match ty {
                                Ty::Reference { pointee, .. } => pointee,
                                ty => ty,
                            }
                        } else {
                            ty
                        },
                    )?,
                })
            })
            .collect::<Option<Vec<_>>>()
        else {
            return;
        };
        let effects = self.async_source_effects(&future);
        let origin = self.nominal_accesses[name].origin.clone();
        if branch_factory.is_none() {
            self.functions.insert(
                resume_function.to_owned(),
                Function {
                    name: resume_function.to_owned(),
                    foreign: None,
                    builtin: false,
                    compile_groups: Vec::new(),
                    groups: vec![resume_parameters.clone()],
                    return_type: Some(
                        factory_output_source
                            .clone()
                            .expect("ordinary async factory output has a source type"),
                    ),
                    effects: effects.clone(),
                    where_predicates: Vec::new(),
                    body: Some(resume_body.clone()),
                },
            );
            self.function_origins
                .insert(resume_function.to_owned(), origin.clone());
        }

        let effect_row = Ty::EffectRow {
            unsafe_effect: future.unsafe_effect,
            throws_error: future.throws_error.clone().map(Box::new),
            custom_effects: future
                .custom_effects
                .iter()
                .filter(|effect| effect.as_str() != self.lang_item_name(LangItemKind::AsyncEffect))
                .cloned()
                .collect(),
        };
        let trait_key = TraitImplKey {
            self_ty: Ty::Struct(name.to_owned()),
            trait_ref: TraitRefKey {
                name: self.lang_item_name(LangItemKind::Future).to_owned(),
                arguments: vec![effect_row],
            },
        };
        let poll_function = trait_method_name(&trait_key, "poll");
        let poll_ty = self.signatures[&poll_function]
            .result
            .clone()
            .expect("generated poll has an output");
        let Ty::Enum(poll_name) = &poll_ty else {
            unreachable!("generated poll output is a Poll enum");
        };
        let self_ty = Ty::Struct(name.to_owned());
        let mut machine_awaited = awaited.clone();
        let mut machine_poll_ty = poll_ty.clone();
        let mut machine_poll_name = poll_name.clone();
        let mut residual_transition = None;
        if let Some(residual) = &awaited.residual_continuation {
            let mut input_types = awaited.continuation_capture_types.clone();
            input_types.extend(awaited.retained_types.clone());
            input_types.push(awaited.output.clone());
            let input_ty = Ty::Tuple(input_types.clone());
            let Some(input_source) = self.source_type_for_ty(&input_ty) else {
                self.error(
                    "async residual continuation inputs cannot be represented in the source type system",
                );
                return;
            };
            let poll_template = self.lang_item_name(LangItemKind::Poll).to_owned();
            let Some(transition_poll_name) = self.ensure_nominal_instance(
                NominalKind::Enum,
                &poll_template,
                vec![input_source.clone()],
                vec![input_ty.clone()],
            ) else {
                return;
            };
            machine_poll_name = transition_poll_name.clone();
            machine_poll_ty = Ty::Enum(transition_poll_name);

            let pack_function = format!("{poll_function}$ready$pack");
            let mut input_modes = awaited.continuation_capture_modes.clone();
            input_modes.extend(awaited.retained_modes.clone());
            input_modes.push(PassMode::Move);
            let parameters = input_types
                .iter()
                .zip(&input_modes)
                .enumerate()
                .map(|(index, (ty, mode))| ParamSig {
                    name: format!("input.{index}"),
                    ty: ty.clone(),
                    mode: *mode,
                })
                .collect::<Vec<_>>();
            self.signatures.insert(
                pack_function.clone(),
                FunctionSig {
                    groups: vec![parameters],
                    unsafe_effect: false,
                    throws_error: None,
                    custom_effects: Vec::new(),
                    result: Some(input_ty.clone()),
                },
            );
            let pack_params = input_types
                .iter()
                .zip(&input_modes)
                .enumerate()
                .map(|(index, (ty, mode))| HirParam {
                    id: 60_000 + index,
                    name: format!("input.{index}"),
                    ty: ty.clone(),
                    mode: *mode,
                })
                .collect::<Vec<_>>();
            let pack_values = input_types
                .iter()
                .zip(&input_modes)
                .enumerate()
                .map(|(index, (ty, mode))| HirExpr {
                    ty: ty.clone(),
                    kind: HirExprKind::Read {
                        place: HirPlace {
                            local: 60_000 + index,
                            root_ty: ty.clone(),
                            projections: Vec::new(),
                            dynamic_index: None,
                            ty: ty.clone(),
                            capability: LocalCapability::Owned,
                            root_mutable: false,
                            loan: None,
                            indirect: false,
                        },
                        kind: if matches!(
                            mode,
                            PassMode::Borrow | PassMode::MutBorrow | PassMode::Copy
                        ) {
                            HirReadKind::Copy
                        } else {
                            HirReadKind::Move
                        },
                    },
                })
                .collect();
            self.lifted_functions.push(HirFunction {
                name: pack_function.clone(),
                params: pack_params,
                result: input_ty.clone(),
                body: HirExpr {
                    ty: input_ty.clone(),
                    kind: HirExprKind::Tuple(pack_values),
                },
            });
            machine_awaited.continuation = Some(pack_function);
            machine_awaited.continuation_output = Some(input_ty.clone());
            machine_awaited.next = None;
            machine_awaited.residual_continuation = None;
            residual_transition = Some((residual.clone(), input_ty, input_source, input_modes));
        }
        let machine_output_source = residual_transition
            .as_ref()
            .map_or_else(|| output_source.clone(), |(_, _, source, _)| source.clone());
        let bundle_value = HirExpr {
            ty: awaited.factory_output.clone(),
            kind: HirExprKind::Read {
                place: HirPlace {
                    local: 50_000,
                    root_ty: awaited.factory_output.clone(),
                    projections: Vec::new(),
                    dynamic_index: None,
                    ty: awaited.factory_output.clone(),
                    capability: LocalCapability::Owned,
                    root_mutable: false,
                    loan: None,
                    indirect: false,
                },
                kind: HirReadKind::Move,
            },
        };
        let full_body = suspended_poll_body(
            &self_ty,
            &machine_poll_ty,
            &machine_poll_name,
            residual_transition
                .as_ref()
                .map_or(&future.output, |(_, input, _, _)| input),
            &machine_awaited,
            bundle_value,
            None,
        );
        let HirExprKind::If {
            then_branch,
            else_branch: Some(waiting_branch),
            ..
        } = full_body.kind
        else {
            unreachable!("suspended poll body has cold and waiting branches");
        };

        let start_helper = format!("{poll_function}$start");
        let wait_helper = format!("{poll_function}$wait");
        let self_reference = Ty::Reference {
            pointee: Box::new(self_ty.clone()),
            mutable: true,
            region: None,
        };
        let self_parameter = ParamSig {
            name: "self".to_owned(),
            ty: self_reference.clone(),
            mode: PassMode::Inferred,
        };
        self.signatures.insert(
            start_helper.clone(),
            FunctionSig {
                groups: vec![vec![
                    ParamSig {
                        name: "segment".to_owned(),
                        ty: awaited.factory_output.clone(),
                        mode: PassMode::Move,
                    },
                    self_parameter.clone(),
                ]],
                unsafe_effect: false,
                throws_error: None,
                custom_effects: Vec::new(),
                result: Some(machine_poll_ty.clone()),
            },
        );
        self.signatures.insert(
            wait_helper.clone(),
            FunctionSig {
                groups: vec![vec![self_parameter.clone()]],
                unsafe_effect: false,
                throws_error: None,
                custom_effects: Vec::new(),
                result: Some(machine_poll_ty.clone()),
            },
        );
        let self_source = Type::Named(name.to_owned(), Vec::new());
        let self_source_reference = Type::Borrow {
            mutable: true,
            access: None,
            region: None,
            pointee: Box::new(self_source.clone()),
        };
        let pure_effects = FunctionEffects {
            unsafe_effect: false,
            throws: None,
            custom: Vec::new(),
            parameters: Vec::new(),
        };
        if let Some(factory_output_source) = factory_output_source {
            self.functions.insert(
                start_helper.clone(),
                Function {
                    name: start_helper.clone(),
                    foreign: None,
                    builtin: false,
                    compile_groups: Vec::new(),
                    groups: vec![vec![
                        Param {
                            mode: PassMode::Move,
                            access: None,
                            modifiers: Vec::new(),
                            region: None,
                            name: "segment".to_owned(),
                            ty: factory_output_source,
                        },
                        Param {
                            mode: PassMode::Inferred,
                            access: None,
                            modifiers: Vec::new(),
                            region: None,
                            name: "self".to_owned(),
                            ty: self_source_reference.clone(),
                        },
                    ]],
                    return_type: Some(Type::Named(
                        self.lang_item_name(LangItemKind::Poll).to_owned(),
                        vec![machine_output_source.clone()],
                    )),
                    effects: pure_effects.clone(),
                    where_predicates: Vec::new(),
                    body: None,
                },
            );
        }
        self.functions.insert(
            wait_helper.clone(),
            Function {
                name: wait_helper.clone(),
                foreign: None,
                builtin: false,
                compile_groups: Vec::new(),
                groups: vec![vec![Param {
                    mode: PassMode::Inferred,
                    access: None,
                    modifiers: Vec::new(),
                    region: None,
                    name: "self".to_owned(),
                    ty: self_source_reference.clone(),
                }]],
                return_type: Some(Type::Named(
                    self.lang_item_name(LangItemKind::Poll).to_owned(),
                    vec![machine_output_source.clone()],
                )),
                effects: pure_effects.clone(),
                where_predicates: Vec::new(),
                body: None,
            },
        );
        self.function_origins
            .insert(start_helper.clone(), origin.clone());
        self.function_origins
            .insert(wait_helper.clone(), origin.clone());
        self.function_accesses
            .insert(start_helper.clone(), self.nominal_accesses[name].clone());
        self.function_accesses
            .insert(wait_helper.clone(), self.nominal_accesses[name].clone());
        self.lifted_functions.push(HirFunction {
            name: start_helper.clone(),
            params: vec![
                HirParam {
                    id: 50_000,
                    name: "segment".to_owned(),
                    ty: awaited.factory_output.clone(),
                    mode: PassMode::Move,
                },
                HirParam {
                    id: 0,
                    name: "self".to_owned(),
                    ty: self_reference.clone(),
                    mode: PassMode::Inferred,
                },
            ],
            result: machine_poll_ty.clone(),
            body: *then_branch,
        });
        let mut branch_start_helpers = Vec::new();
        if let (Ty::Enum(branch_name), Some((_, _, _, branch_types))) =
            (&awaited.ty, branch_factory.as_ref())
        {
            for (variant, branch_ty) in branch_types.iter().enumerate() {
                let helper = format!("{start_helper}$branch${variant}");
                let local = 50_100 + variant;
                let retained_locals = awaited
                    .retained_types
                    .iter()
                    .enumerate()
                    .map(|(index, _)| 51_000 + variant * 100 + index)
                    .collect::<Vec<_>>();
                let mut signature_parameters = vec![ParamSig {
                    name: "segment".to_owned(),
                    ty: branch_ty.clone(),
                    mode: PassMode::Move,
                }];
                signature_parameters.extend(
                    awaited
                        .retained_types
                        .iter()
                        .zip(&awaited.retained_modes)
                        .enumerate()
                        .map(|(index, (ty, mode))| ParamSig {
                            name: format!("retained.{index}"),
                            ty: ty.clone(),
                            mode: *mode,
                        }),
                );
                signature_parameters.push(self_parameter.clone());
                self.signatures.insert(
                    helper.clone(),
                    FunctionSig {
                        groups: vec![signature_parameters],
                        unsafe_effect: false,
                        throws_error: None,
                        custom_effects: Vec::new(),
                        result: Some(machine_poll_ty.clone()),
                    },
                );
                let branch_source = self
                    .source_type_for_ty(branch_ty)
                    .expect("heterogeneous async branch has a source type");
                let mut source_parameters = vec![Param {
                    mode: PassMode::Move,
                    access: None,
                    modifiers: Vec::new(),
                    region: None,
                    name: "segment".to_owned(),
                    ty: branch_source,
                }];
                source_parameters.extend(
                    awaited
                        .retained_types
                        .iter()
                        .zip(&awaited.retained_modes)
                        .enumerate()
                        .map(|(index, (ty, mode))| Param {
                            mode: *mode,
                            access: None,
                            modifiers: Vec::new(),
                            region: None,
                            name: format!("retained.{index}"),
                            ty: self
                                .source_type_for_ty(ty)
                                .expect("retained async branch state has a source type"),
                        }),
                );
                source_parameters.push(Param {
                    mode: PassMode::Inferred,
                    access: None,
                    modifiers: Vec::new(),
                    region: None,
                    name: "self".to_owned(),
                    ty: self_source_reference.clone(),
                });
                self.functions.insert(
                    helper.clone(),
                    Function {
                        name: helper.clone(),
                        foreign: None,
                        builtin: false,
                        compile_groups: Vec::new(),
                        groups: vec![source_parameters],
                        return_type: Some(Type::Named(
                            self.lang_item_name(LangItemKind::Poll).to_owned(),
                            vec![machine_output_source.clone()],
                        )),
                        effects: pure_effects.clone(),
                        where_predicates: Vec::new(),
                        body: None,
                    },
                );
                self.function_origins.insert(helper.clone(), origin.clone());
                self.function_accesses
                    .insert(helper.clone(), self.nominal_accesses[name].clone());
                let segment = HirExpr {
                    ty: (*branch_ty).clone(),
                    kind: HirExprKind::Read {
                        place: HirPlace {
                            local,
                            root_ty: (*branch_ty).clone(),
                            projections: Vec::new(),
                            dynamic_index: None,
                            ty: (*branch_ty).clone(),
                            capability: LocalCapability::Owned,
                            root_mutable: false,
                            loan: None,
                            indirect: false,
                        },
                        kind: HirReadKind::Move,
                    },
                };
                let branch = HirExpr {
                    ty: awaited.ty.clone(),
                    kind: HirExprKind::ConstructEnum {
                        name: branch_name.clone(),
                        variant,
                        fields: vec![(0, segment)],
                    },
                };
                let retained = awaited
                    .retained_types
                    .iter()
                    .zip(&awaited.retained_modes)
                    .zip(&retained_locals)
                    .map(|((ty, mode), local)| HirExpr {
                        ty: ty.clone(),
                        kind: HirExprKind::Read {
                            place: HirPlace {
                                local: *local,
                                root_ty: ty.clone(),
                                projections: Vec::new(),
                                dynamic_index: None,
                                ty: ty.clone(),
                                capability: LocalCapability::Owned,
                                root_mutable: false,
                                loan: None,
                                indirect: false,
                            },
                            kind: if *mode == PassMode::Copy {
                                HirReadKind::Copy
                            } else {
                                HirReadKind::Move
                            },
                        },
                    })
                    .collect::<Vec<_>>();
                let bundle = if retained.is_empty() {
                    branch
                } else {
                    HirExpr {
                        ty: awaited.factory_output.clone(),
                        kind: HirExprKind::Tuple(std::iter::once(branch).chain(retained).collect()),
                    }
                };
                let self_value = HirExpr {
                    ty: self_reference.clone(),
                    kind: HirExprKind::Read {
                        place: HirPlace {
                            local: 0,
                            root_ty: self_reference.clone(),
                            projections: Vec::new(),
                            dynamic_index: None,
                            ty: self_reference.clone(),
                            capability: LocalCapability::MutParam,
                            root_mutable: false,
                            loan: None,
                            indirect: false,
                        },
                        kind: HirReadKind::Copy,
                    },
                };
                let mut params = vec![HirParam {
                    id: local,
                    name: "segment".to_owned(),
                    ty: (*branch_ty).clone(),
                    mode: PassMode::Move,
                }];
                params.extend(
                    awaited
                        .retained_types
                        .iter()
                        .zip(&awaited.retained_modes)
                        .zip(&retained_locals)
                        .enumerate()
                        .map(|(index, ((ty, mode), local))| HirParam {
                            id: *local,
                            name: format!("retained.{index}"),
                            ty: ty.clone(),
                            mode: *mode,
                        }),
                );
                params.push(HirParam {
                    id: 0,
                    name: "self".to_owned(),
                    ty: self_reference.clone(),
                    mode: PassMode::Inferred,
                });
                let mut arguments = vec![HirArgument::Move(bundle)];
                arguments.push(HirArgument::Copy(self_value));
                self.lifted_functions.push(HirFunction {
                    name: helper.clone(),
                    params,
                    result: machine_poll_ty.clone(),
                    body: HirExpr {
                        ty: machine_poll_ty.clone(),
                        kind: HirExprKind::Call {
                            function: start_helper.clone(),
                            arguments,
                            consumed_callable: None,
                            diverges: false,
                        },
                    },
                });
                branch_start_helpers.push(helper);
            }
        }
        self.lifted_functions.push(HirFunction {
            name: wait_helper.clone(),
            params: vec![HirParam {
                id: 0,
                name: "self".to_owned(),
                ty: self_reference.clone(),
                mode: PassMode::Inferred,
            }],
            result: machine_poll_ty.clone(),
            body: *waiting_branch,
        });

        let begin_helper = format!("{poll_function}$begin");
        self.signatures.insert(
            begin_helper.clone(),
            FunctionSig {
                groups: vec![vec![self_parameter.clone()]],
                unsafe_effect: false,
                throws_error: None,
                custom_effects: Vec::new(),
                result: Some(Ty::Unit),
            },
        );
        self.functions.insert(
            begin_helper.clone(),
            Function {
                name: begin_helper.clone(),
                foreign: None,
                builtin: false,
                compile_groups: Vec::new(),
                groups: vec![vec![Param {
                    mode: PassMode::Inferred,
                    access: None,
                    modifiers: Vec::new(),
                    region: None,
                    name: "self".to_owned(),
                    ty: self_source_reference.clone(),
                }]],
                return_type: Some(Type::Unit),
                effects: pure_effects.clone(),
                where_predicates: Vec::new(),
                body: None,
            },
        );
        self.function_origins
            .insert(begin_helper.clone(), origin.clone());
        self.function_accesses
            .insert(begin_helper.clone(), self.nominal_accesses[name].clone());
        self.lifted_functions.push(HirFunction {
            name: begin_helper.clone(),
            params: vec![HirParam {
                id: 0,
                name: "self".to_owned(),
                ty: self_reference.clone(),
                mode: PassMode::Inferred,
            }],
            result: Ty::Unit,
            body: set_state(&self_ty, 4),
        });

        let layout = self.struct_layouts[name].clone();
        let mut capture_helpers = Vec::new();
        for (index, mode) in future.capture_modes.iter().enumerate() {
            let field = index + 1;
            let ty = layout.fields[field].ty.clone();
            let Some(source_ty) = self.source_type_for_ty(&ty) else {
                return;
            };
            let helper = format!("{poll_function}$capture${index}");
            self.signatures.insert(
                helper.clone(),
                FunctionSig {
                    groups: vec![vec![self_parameter.clone()]],
                    unsafe_effect: false,
                    throws_error: None,
                    custom_effects: Vec::new(),
                    result: Some(ty.clone()),
                },
            );
            self.functions.insert(
                helper.clone(),
                Function {
                    name: helper.clone(),
                    foreign: None,
                    builtin: false,
                    compile_groups: Vec::new(),
                    groups: vec![vec![Param {
                        mode: PassMode::Inferred,
                        access: None,
                        modifiers: Vec::new(),
                        region: None,
                        name: "self".to_owned(),
                        ty: self_source_reference.clone(),
                    }]],
                    return_type: Some(source_ty),
                    effects: pure_effects.clone(),
                    where_predicates: Vec::new(),
                    body: None,
                },
            );
            self.function_origins.insert(helper.clone(), origin.clone());
            self.function_accesses
                .insert(helper.clone(), self.nominal_accesses[name].clone());
            let place = async_field_place(0, self_ty.clone(), field, ty.clone());
            let body = match mode {
                PassMode::Borrow | PassMode::MutBorrow | PassMode::Copy => HirExpr {
                    ty: ty.clone(),
                    kind: HirExprKind::Read {
                        place,
                        kind: HirReadKind::Copy,
                    },
                },
                PassMode::Move => HirExpr {
                    ty: ty.clone(),
                    kind: HirExprKind::RawTake(Box::new(HirExpr {
                        ty: Ty::Pointer {
                            pointee: Box::new(ty.clone()),
                            mutable: true,
                        },
                        kind: HirExprKind::RawAddress { place },
                    })),
                },
                PassMode::Inferred => {
                    unreachable!("suspended residual capture modes are normalized")
                }
            };
            self.lifted_functions.push(HirFunction {
                name: helper.clone(),
                params: vec![HirParam {
                    id: 0,
                    name: "self".to_owned(),
                    ty: self_reference.clone(),
                    mode: PassMode::Inferred,
                }],
                result: ty,
                body,
            });
            capture_helpers.push(helper);
        }

        let self_value = || Expr::Name("self".to_owned());
        let call_helper = |helper: String, arguments: Vec<Expr>| {
            Expr::Call(
                Box::new(Expr::Name(helper)),
                arguments
                    .into_iter()
                    .map(|value| CallArg { label: None, value })
                    .collect(),
            )
        };
        let resume_arguments = || {
            capture_helpers
                .iter()
                .zip(&future.capture_modes)
                .enumerate()
                .map(|(index, (helper, mode))| CallArg {
                    label: None,
                    value: if matches!(mode, PassMode::Borrow | PassMode::MutBorrow) {
                        call_helper(
                            "$async$copy$stored$borrow".to_owned(),
                            vec![Expr::Member(
                                Box::new(self_value()),
                                format!("capture.{index}"),
                            )],
                        )
                    } else {
                        call_helper(helper.clone(), vec![self_value()])
                    },
                })
                .collect::<Vec<_>>()
        };
        let resume = Expr::Call(
            Box::new(Expr::Name(resume_function.to_owned())),
            resume_arguments(),
        );
        let branch_resume = branch_factory
            .as_ref()
            .map(|(prefix, selection, retained, _)| {
                let helper = format!("{resume_function}$branch$select");
                let mut parameters = resume_parameters.clone();
                parameters.push(Param {
                    mode: PassMode::Inferred,
                    access: None,
                    modifiers: Vec::new(),
                    region: None,
                    name: "self".to_owned(),
                    ty: self_source_reference.clone(),
                });
                let mut signature_parameters = resume_captures
                    .iter()
                    .map(|(name, ty, mode)| ParamSig {
                        name: name.clone(),
                        ty: ty.clone(),
                        mode: *mode,
                    })
                    .collect::<Vec<_>>();
                signature_parameters.push(self_parameter.clone());
                self.signatures.insert(
                    helper.clone(),
                    FunctionSig {
                        groups: vec![signature_parameters],
                        unsafe_effect: future.unsafe_effect,
                        throws_error: future.throws_error.clone(),
                        custom_effects: future
                            .custom_effects
                            .iter()
                            .filter(|effect| {
                                effect.as_str() != self.lang_item_name(LangItemKind::AsyncEffect)
                            })
                            .cloned()
                            .collect(),
                        result: Some(machine_poll_ty.clone()),
                    },
                );
                self.functions.insert(
                    helper.clone(),
                    Function {
                        name: helper.clone(),
                        foreign: None,
                        builtin: false,
                        compile_groups: Vec::new(),
                        groups: vec![parameters],
                        return_type: Some(Type::Named(
                            self.lang_item_name(LangItemKind::Poll).to_owned(),
                            vec![machine_output_source.clone()],
                        )),
                        effects: effects.clone(),
                        where_predicates: Vec::new(),
                        body: Some(wrap_heterogeneous_branch_factory(
                            prefix.clone(),
                            selection.clone(),
                            retained,
                            &branch_start_helpers,
                        )),
                    },
                );
                self.function_origins.insert(helper.clone(), origin.clone());
                helper
            });
        let state = || Expr::Member(Box::new(self_value()), "state".to_owned());
        let cold_poll = if let Some(branch_resume) = branch_resume {
            let mut arguments = resume_arguments();
            arguments.push(CallArg {
                label: None,
                value: self_value(),
            });
            Expr::Call(Box::new(Expr::Name(branch_resume)), arguments)
        } else {
            call_helper(start_helper, vec![resume, self_value()])
        };
        let machine_body = Expr::If {
            condition: Box::new(Expr::Binary(
                Box::new(state()),
                BinaryOp::Eq,
                Box::new(Expr::Integer(0)),
            )),
            then_branch: Box::new(Expr::Block(
                vec![Stmt::Expr(call_helper(begin_helper, vec![self_value()]))],
                Some(Box::new(cold_poll)),
            )),
            else_branch: Some(Box::new(call_helper(wait_helper, vec![self_value()]))),
        };
        let body = if let Some((residual, input_ty, input_source, input_modes)) =
            residual_transition
        {
            self.functions.insert(
                residual.function.clone(),
                Function {
                    name: residual.function.clone(),
                    foreign: None,
                    builtin: false,
                    compile_groups: Vec::new(),
                    groups: vec![residual.parameters.clone()],
                    return_type: Some(residual.result.clone()),
                    effects: residual.effects.clone(),
                    where_predicates: Vec::new(),
                    body: Some(residual.body.clone()),
                },
            );
            self.function_origins
                .insert(residual.function.clone(), origin.clone());

            let Ty::Tuple(input_types) = input_ty else {
                unreachable!("residual continuation transition input is a tuple");
            };
            let bindings = input_types
                .iter()
                .enumerate()
                .map(|(index, _)| format!("$async$ready$input${index}"))
                .collect::<Vec<_>>();
            let continuation_arguments = bindings
                .iter()
                .zip(&input_modes)
                .map(|(name, mode)| CallArg {
                    label: None,
                    value: if matches!(mode, PassMode::Borrow | PassMode::MutBorrow) {
                        call_helper(
                            "$async$copy$stored$borrow".to_owned(),
                            vec![Expr::Name(name.clone())],
                        )
                    } else {
                        Expr::Name(name.clone())
                    },
                })
                .collect();
            let continuation = Expr::Call(
                Box::new(Expr::Name(residual.function.clone())),
                continuation_arguments,
            );
            let parent_poll_type = Expr::Call(
                Box::new(Expr::Name(
                    self.lang_item_name(LangItemKind::Poll).to_owned(),
                )),
                vec![CallArg {
                    label: None,
                    value: source_type_expression(&output_source),
                }],
            );
            let pending = Expr::Member(Box::new(parent_poll_type.clone()), "Pending".to_owned());
            let ready = Expr::Call(
                Box::new(Expr::Member(Box::new(parent_poll_type), "Ready".to_owned())),
                vec![CallArg {
                    label: None,
                    value: continuation,
                }],
            );
            let _ = input_source;
            Expr::Match {
                scrutinee: Box::new(machine_body),
                arms: vec![
                    crate::ast::MatchArm {
                        pattern: crate::ast::Pattern::Constructor {
                            path: vec!["Pending".to_owned()],
                            fields: crate::ast::PatternFields::Unit,
                        },
                        guard: None,
                        body: pending,
                    },
                    crate::ast::MatchArm {
                        pattern: crate::ast::Pattern::Constructor {
                            path: vec!["Ready".to_owned()],
                            fields: crate::ast::PatternFields::Positional(vec![
                                crate::ast::Pattern::Tuple(
                                    bindings
                                        .iter()
                                        .cloned()
                                        .map(crate::ast::Pattern::Binding)
                                        .collect(),
                                ),
                            ]),
                        },
                        guard: None,
                        body: ready,
                    },
                ],
            }
        } else {
            machine_body
        };
        self.functions.insert(
            poll_function.clone(),
            Function {
                name: poll_function.clone(),
                foreign: None,
                builtin: false,
                compile_groups: Vec::new(),
                groups: vec![
                    vec![Param {
                        mode: PassMode::Inferred,
                        access: None,
                        modifiers: Vec::new(),
                        region: None,
                        name: "self".to_owned(),
                        ty: Type::Borrow {
                            mutable: true,
                            access: None,
                            region: None,
                            pointee: Box::new(Type::Named(name.to_owned(), Vec::new())),
                        },
                    }],
                    Vec::new(),
                ],
                return_type: Some(Type::Named(
                    self.lang_item_name(LangItemKind::Poll).to_owned(),
                    vec![output_source],
                )),
                effects,
                where_predicates: Vec::new(),
                body: Some(body),
            },
        );
        self.function_origins.insert(poll_function.clone(), origin);
        self.lifted_functions.retain(|function| {
            function.name != resume_function
                && function.name != poll_function
                && awaited
                    .residual_continuation
                    .as_ref()
                    .is_none_or(|residual| function.name != residual.function)
        });
    }

    fn async_source_effects(&self, future: &AsyncFutureInfo) -> FunctionEffects {
        FunctionEffects {
            unsafe_effect: future.unsafe_effect,
            throws: future
                .throws_error
                .as_ref()
                .and_then(|error| self.source_type_for_ty(error))
                .map(Box::new),
            custom: future
                .custom_effects
                .iter()
                .filter(|effect| effect.as_str() != self.lang_item_name(LangItemKind::AsyncEffect))
                .filter_map(|effect| super::compile_time::source_type_from_identity(effect))
                .collect(),
            parameters: Vec::new(),
        }
    }
}

fn materialize_async_capture(capture: &ClosureCapture, direct_reference: bool) -> (Ty, HirExpr) {
    if capture.by_value {
        return (
            capture.place.ty.clone(),
            capture
                .value
                .as_deref()
                .cloned()
                .expect("by-value async capture materializes its value"),
        );
    }
    if direct_reference
        && matches!(capture.place.ty, Ty::Reference { .. })
        && matches!(
            capture.mode,
            ClosureCaptureMode::Shared | ClosureCaptureMode::Mutable
        )
    {
        return (
            capture.place.ty.clone(),
            HirExpr {
                ty: capture.place.ty.clone(),
                kind: HirExprKind::Read {
                    place: capture.place.clone(),
                    kind: HirReadKind::Copy,
                },
            },
        );
    }
    match capture.mode {
        ClosureCaptureMode::Shared => {
            let ty = Ty::Reference {
                pointee: Box::new(capture.place.ty.clone()),
                mutable: false,
                region: None,
            };
            (
                ty.clone(),
                HirExpr {
                    ty,
                    kind: HirExprKind::Borrow {
                        place: capture.place.clone(),
                        mutable: false,
                    },
                },
            )
        }
        ClosureCaptureMode::Mutable => {
            let ty = Ty::Reference {
                pointee: Box::new(capture.place.ty.clone()),
                mutable: true,
                region: None,
            };
            (
                ty.clone(),
                HirExpr {
                    ty,
                    kind: HirExprKind::Borrow {
                        place: capture.place.clone(),
                        mutable: true,
                    },
                },
            )
        }
        ClosureCaptureMode::Move => (
            capture.place.ty.clone(),
            capture
                .value
                .as_deref()
                .cloned()
                .expect("move capture materializes its value"),
        ),
    }
}

fn capture_pass_mode(capture: &ClosureCapture) -> PassMode {
    if capture.by_value {
        return PassMode::Copy;
    }
    match capture.mode {
        ClosureCaptureMode::Shared => PassMode::Borrow,
        ClosureCaptureMode::Mutable => PassMode::MutBorrow,
        ClosureCaptureMode::Move => PassMode::Move,
    }
}

fn ready_poll_body(
    self_ty: &Ty,
    poll_ty: &Ty,
    poll_name: &str,
    output: &Ty,
    resume: HirExpr,
) -> HirExpr {
    let ready = poll_ready(poll_ty, poll_name, output, resume);
    HirExpr {
        ty: poll_ty.clone(),
        kind: HirExprKind::If {
            condition: Box::new(state_is(self_ty, 0)),
            then_branch: Box::new(HirExpr {
                ty: poll_ty.clone(),
                kind: HirExprKind::Block(
                    vec![HirStmt::Expr(set_state(self_ty, 1))],
                    Some(Box::new(ready)),
                ),
            }),
            else_branch: Some(Box::new(trap())),
        },
    }
}

fn suspended_poll_body(
    self_ty: &Ty,
    poll_ty: &Ty,
    poll_name: &str,
    output: &Ty,
    awaited: &AwaitedFutureInfo,
    resume: HirExpr,
    loop_condition: Option<HirExpr>,
) -> HirExpr {
    let child_place = async_field_place(0, self_ty.clone(), awaited.field, awaited.ty.clone());
    let bundle_local = 1_000;
    let bundle_binding = (!awaited.retained_fields.is_empty()).then(|| {
        HirStmt::Let(super::hir::HirBinding {
            id: bundle_local,
            name: "async.segment".to_owned(),
            ty: awaited.factory_output.clone(),
            mutable: false,
            value: resume.clone(),
        })
    });
    let bundle_field = |index: usize, ty: &Ty| HirExpr {
        ty: ty.clone(),
        kind: HirExprKind::Read {
            place: HirPlace {
                local: bundle_local,
                root_ty: awaited.factory_output.clone(),
                projections: vec![index],
                dynamic_index: None,
                ty: ty.clone(),
                capability: LocalCapability::Owned,
                root_mutable: false,
                loan: None,
                indirect: false,
            },
            kind: HirReadKind::Move,
        },
    };
    let initialize_field = |place: HirPlace, value: HirExpr| HirExpr {
        ty: Ty::Unit,
        kind: HirExprKind::RawInit {
            pointer: Box::new(HirExpr {
                ty: Ty::Pointer {
                    pointee: Box::new(place.ty.clone()),
                    mutable: true,
                },
                kind: HirExprKind::RawAddress { place },
            }),
            value: Box::new(value),
        },
    };
    let initialize_child = initialize_field(
        child_place.clone(),
        if awaited.retained_fields.is_empty() {
            resume.clone()
        } else {
            bundle_field(0, &awaited.ty)
        },
    );
    let initialize_retained = awaited
        .retained_fields
        .iter()
        .zip(&awaited.retained_types)
        .enumerate()
        .map(|(index, (field, ty))| {
            initialize_field(
                async_field_place(0, self_ty.clone(), *field, ty.clone()),
                bundle_field(index + 1, ty),
            )
        });
    let cold_poll = poll_awaited(
        self_ty,
        poll_ty,
        poll_name,
        output,
        awaited,
        AwaitPollState::new(1, 3, 2),
        Some(AsyncLoopPollContext {
            resume: &resume,
            condition: loop_condition.as_ref(),
        }),
    );
    let resumed_poll = poll_awaited(
        self_ty,
        poll_ty,
        poll_name,
        output,
        awaited,
        AwaitPollState::new(2, 4, 2),
        Some(AsyncLoopPollContext {
            resume: &resume,
            condition: loop_condition.as_ref(),
        }),
    );
    let waiting_branch = if let Some(next) = &awaited.next {
        HirExpr {
            ty: poll_ty.clone(),
            kind: HirExprKind::If {
                condition: Box::new(state_is(self_ty, 1)),
                then_branch: Box::new(resumed_poll),
                else_branch: Some(Box::new(HirExpr {
                    ty: poll_ty.clone(),
                    kind: HirExprKind::If {
                        condition: Box::new(state_is(self_ty, 2)),
                        then_branch: Box::new(poll_awaited(
                            self_ty,
                            poll_ty,
                            poll_name,
                            output,
                            next,
                            AwaitPollState::new(5, 6, 3),
                            None,
                        )),
                        else_branch: Some(Box::new(trap())),
                    },
                })),
            },
        }
    } else {
        HirExpr {
            ty: poll_ty.clone(),
            kind: HirExprKind::If {
                condition: Box::new(state_is(self_ty, 1)),
                then_branch: Box::new(resumed_poll),
                else_branch: Some(Box::new(trap())),
            },
        }
    };
    let cold_start = HirExpr {
        ty: poll_ty.clone(),
        kind: HirExprKind::Block(
            bundle_binding
                .into_iter()
                .chain(std::iter::once(HirStmt::Expr(initialize_child)))
                .chain(initialize_retained.map(HirStmt::Expr))
                .chain(std::iter::once(HirStmt::Expr(set_state(self_ty, 1))))
                .collect(),
            Some(Box::new(cold_poll)),
        ),
    };
    let cold_start = match (&awaited.loop_condition, loop_condition) {
        (Some(condition), Some(condition_call)) if !condition.post_test => HirExpr {
            ty: poll_ty.clone(),
            kind: HirExprKind::If {
                condition: Box::new(condition_call),
                then_branch: Box::new(cold_start),
                else_branch: Some(Box::new(HirExpr {
                    ty: poll_ty.clone(),
                    kind: HirExprKind::Block(
                        vec![HirStmt::Expr(set_state(self_ty, 2))],
                        Some(Box::new(poll_ready(
                            poll_ty,
                            poll_name,
                            output,
                            HirExpr {
                                ty: Ty::Unit,
                                kind: HirExprKind::Unit,
                            },
                        ))),
                    ),
                })),
            },
        },
        _ => cold_start,
    };
    HirExpr {
        ty: poll_ty.clone(),
        kind: HirExprKind::If {
            condition: Box::new(state_is(self_ty, 0)),
            then_branch: Box::new(cold_start),
            else_branch: Some(Box::new(waiting_branch)),
        },
    }
}

#[derive(Clone, Copy)]
struct AwaitPollState {
    output_local: usize,
    next_output_local: usize,
    completed: i128,
}

#[derive(Clone, Copy)]
struct AsyncLoopPollContext<'a> {
    resume: &'a HirExpr,
    condition: Option<&'a HirExpr>,
}

impl AwaitPollState {
    fn new(output_local: usize, next_output_local: usize, completed: i128) -> Self {
        Self {
            output_local,
            next_output_local,
            completed,
        }
    }
}

fn poll_awaited(
    self_ty: &Ty,
    poll_ty: &Ty,
    poll_name: &str,
    parent_output: &Ty,
    awaited: &AwaitedFutureInfo,
    state: AwaitPollState,
    loop_context: Option<AsyncLoopPollContext<'_>>,
) -> HirExpr {
    let child_place = async_field_place(0, self_ty.clone(), awaited.field, awaited.ty.clone());
    let child_reference = Ty::Reference {
        pointee: Box::new(awaited.ty.clone()),
        mutable: true,
        region: None,
    };
    let call = HirExpr {
        ty: awaited.poll_ty.clone(),
        kind: HirExprKind::Call {
            function: awaited.poll_function.clone(),
            arguments: vec![HirArgument::Copy(HirExpr {
                ty: child_reference,
                kind: HirExprKind::Borrow {
                    place: child_place.clone(),
                    mutable: true,
                },
            })],
            consumed_callable: None,
            diverges: false,
        },
    };
    let output_place = HirPlace {
        local: state.output_local,
        root_ty: awaited.output.clone(),
        projections: Vec::new(),
        dynamic_index: None,
        ty: awaited.output.clone(),
        capability: LocalCapability::Owned,
        root_mutable: false,
        loan: None,
        indirect: false,
    };
    let take_child = HirExpr {
        ty: awaited.ty.clone(),
        kind: HirExprKind::RawTake(Box::new(HirExpr {
            ty: Ty::Pointer {
                pointee: Box::new(awaited.ty.clone()),
                mutable: true,
            },
            kind: HirExprKind::RawAddress { place: child_place },
        })),
    };
    let awaited_output = HirExpr {
        ty: awaited.output.clone(),
        kind: HirExprKind::Read {
            place: output_place,
            kind: HirReadKind::Move,
        },
    };
    let mut continuation_arguments = Vec::new();
    for ((field, mode), ty) in awaited
        .continuation_fields
        .iter()
        .zip(&awaited.continuation_capture_modes)
        .zip(&awaited.continuation_capture_types)
    {
        let place = async_field_place(0, self_ty.clone(), *field, ty.clone());
        continuation_arguments.push(match mode {
            PassMode::Borrow | PassMode::MutBorrow | PassMode::Copy => HirArgument::Copy(HirExpr {
                ty: ty.clone(),
                kind: HirExprKind::Read {
                    place,
                    kind: HirReadKind::Copy,
                },
            }),
            PassMode::Move => HirArgument::Move(HirExpr {
                ty: ty.clone(),
                kind: HirExprKind::RawTake(Box::new(HirExpr {
                    ty: Ty::Pointer {
                        pointee: Box::new(ty.clone()),
                        mutable: true,
                    },
                    kind: HirExprKind::RawAddress { place },
                })),
            }),
            PassMode::Inferred => {
                unreachable!("async continuation capture modes are normalized")
            }
        });
    }
    for ((field, mode), ty) in awaited
        .retained_fields
        .iter()
        .zip(&awaited.retained_modes)
        .zip(&awaited.retained_types)
    {
        let place = async_field_place(0, self_ty.clone(), *field, ty.clone());
        continuation_arguments.push(match mode {
            PassMode::Copy => HirArgument::Copy(HirExpr {
                ty: ty.clone(),
                kind: HirExprKind::Read {
                    place,
                    kind: HirReadKind::Copy,
                },
            }),
            PassMode::Move => HirArgument::Move(HirExpr {
                ty: ty.clone(),
                kind: HirExprKind::RawTake(Box::new(HirExpr {
                    ty: Ty::Pointer {
                        pointee: Box::new(ty.clone()),
                        mutable: true,
                    },
                    kind: HirExprKind::RawAddress { place },
                })),
            }),
            PassMode::Borrow | PassMode::MutBorrow | PassMode::Inferred => {
                unreachable!("retained async locals use Copy or Move")
            }
        });
    }
    continuation_arguments.push(HirArgument::Move(awaited_output.clone()));
    let completed_value = match &awaited.continuation {
        Some(continuation) => HirExpr {
            ty: awaited
                .continuation_output
                .clone()
                .expect("async continuation has an output"),
            kind: HirExprKind::Call {
                function: continuation.clone(),
                arguments: continuation_arguments,
                consumed_callable: None,
                diverges: parent_output == &Ty::Never,
            },
        },
        None => awaited_output,
    };
    if let Some(step) = &awaited.loop_step {
        return poll_async_loop_step(
            self_ty,
            poll_ty,
            poll_name,
            parent_output,
            awaited,
            step,
            state,
            call,
            take_child,
            completed_value,
            loop_context
                .expect("async loop polling has a reusable factory")
                .resume,
            loop_context.and_then(|context| context.condition),
        );
    }
    let ready_body = if let Some(next) = &awaited.next {
        let next_place = async_field_place(0, self_ty.clone(), next.field, next.ty.clone());
        let initialize_next = HirExpr {
            ty: Ty::Unit,
            kind: HirExprKind::RawInit {
                pointer: Box::new(HirExpr {
                    ty: Ty::Pointer {
                        pointee: Box::new(next.ty.clone()),
                        mutable: true,
                    },
                    kind: HirExprKind::RawAddress { place: next_place },
                }),
                value: Box::new(completed_value),
            },
        };
        HirExpr {
            ty: poll_ty.clone(),
            kind: HirExprKind::Block(
                vec![
                    HirStmt::Expr(take_child),
                    HirStmt::Expr(initialize_next),
                    HirStmt::Expr(set_state(self_ty, 2)),
                ],
                Some(Box::new(poll_awaited(
                    self_ty,
                    poll_ty,
                    poll_name,
                    parent_output,
                    next,
                    AwaitPollState::new(state.next_output_local, state.next_output_local + 100, 3),
                    None,
                ))),
            ),
        }
    } else {
        HirExpr {
            ty: poll_ty.clone(),
            kind: HirExprKind::Block(
                vec![
                    HirStmt::Expr(take_child),
                    HirStmt::Expr(set_state(self_ty, state.completed)),
                ],
                Some(Box::new(poll_ready(
                    poll_ty,
                    poll_name,
                    parent_output,
                    completed_value,
                ))),
            ),
        }
    };
    HirExpr {
        ty: poll_ty.clone(),
        kind: HirExprKind::Match {
            scrutinee: Box::new(call),
            arms: vec![
                HirMatchArm {
                    matcher: HirMatcher::Variant(0),
                    bindings: Vec::new(),
                    guard: None,
                    body: poll_pending(poll_ty, poll_name),
                },
                HirMatchArm {
                    matcher: HirMatcher::Variant(1),
                    bindings: vec![HirPatternBinding {
                        id: state.output_local,
                        name: "awaited.output".to_owned(),
                        ty: awaited.output.clone(),
                        path: vec![1],
                        moves: true,
                    }],
                    guard: None,
                    body: ready_body,
                },
            ],
        },
    }
}

#[allow(clippy::too_many_arguments)]
fn poll_async_loop_step(
    self_ty: &Ty,
    poll_ty: &Ty,
    poll_name: &str,
    parent_output: &Ty,
    awaited: &AwaitedFutureInfo,
    step: &AsyncLoopStepInfo,
    state: AwaitPollState,
    call: HirExpr,
    take_child: HirExpr,
    completed_value: HirExpr,
    loop_resume: &HirExpr,
    loop_condition: Option<&HirExpr>,
) -> HirExpr {
    let continue_local = 20_000 + state.output_local * 2;
    let break_local = continue_local + 1;
    let child_place = async_field_place(0, self_ty.clone(), awaited.field, awaited.ty.clone());
    let initialize_child = HirExpr {
        ty: Ty::Unit,
        kind: HirExprKind::RawInit {
            pointer: Box::new(HirExpr {
                ty: Ty::Pointer {
                    pointee: Box::new(awaited.ty.clone()),
                    mutable: true,
                },
                kind: HirExprKind::RawAddress { place: child_place },
            }),
            value: Box::new(loop_resume.clone()),
        },
    };
    let initialize_carry = awaited
        .loop_carry_fields
        .iter()
        .zip(&awaited.loop_carry_types)
        .enumerate()
        .map(|(index, (field, ty))| {
            let value = HirExpr {
                ty: ty.clone(),
                kind: HirExprKind::Read {
                    place: HirPlace {
                        local: continue_local,
                        root_ty: step.carry.clone(),
                        projections: vec![index],
                        dynamic_index: None,
                        ty: ty.clone(),
                        capability: LocalCapability::Owned,
                        root_mutable: false,
                        loan: None,
                        indirect: false,
                    },
                    kind: HirReadKind::Move,
                },
            };
            HirStmt::Expr(HirExpr {
                ty: Ty::Unit,
                kind: HirExprKind::RawInit {
                    pointer: Box::new(HirExpr {
                        ty: Ty::Pointer {
                            pointee: Box::new(ty.clone()),
                            mutable: true,
                        },
                        kind: HirExprKind::RawAddress {
                            place: async_field_place(0, self_ty.clone(), *field, ty.clone()),
                        },
                    }),
                    value: Box::new(value),
                },
            })
        })
        .collect::<Vec<_>>();
    let break_value = HirExpr {
        ty: step.output.clone(),
        kind: HirExprKind::Read {
            place: HirPlace {
                local: break_local,
                root_ty: step.output.clone(),
                projections: Vec::new(),
                dynamic_index: None,
                ty: step.output.clone(),
                capability: LocalCapability::Owned,
                root_mutable: false,
                loan: None,
                indirect: false,
            },
            kind: HirReadKind::Move,
        },
    };
    let continue_poll = HirExpr {
        ty: Ty::Never,
        kind: HirExprKind::Block(
            initialize_carry
                .into_iter()
                .chain(std::iter::once(HirStmt::Expr(initialize_child)))
                .chain(std::iter::once(HirStmt::Expr(set_state(self_ty, 1))))
                .collect(),
            Some(Box::new(HirExpr {
                ty: Ty::Never,
                kind: HirExprKind::Continue,
            })),
        ),
    };
    let continue_poll = match loop_condition {
        Some(condition) => HirExpr {
            ty: Ty::Never,
            kind: HirExprKind::If {
                condition: Box::new(condition.clone()),
                then_branch: Box::new(continue_poll),
                else_branch: Some(Box::new(HirExpr {
                    ty: Ty::Never,
                    kind: HirExprKind::Block(
                        vec![HirStmt::Expr(set_state(self_ty, state.completed))],
                        Some(Box::new(HirExpr {
                            ty: Ty::Never,
                            kind: HirExprKind::Break(Some(Box::new(poll_ready(
                                poll_ty,
                                poll_name,
                                parent_output,
                                HirExpr {
                                    ty: Ty::Unit,
                                    kind: HirExprKind::Unit,
                                },
                            )))),
                        })),
                    ),
                })),
            },
        },
        None => continue_poll,
    };
    let step_match = HirExpr {
        ty: Ty::Never,
        kind: HirExprKind::Match {
            scrutinee: Box::new(completed_value),
            arms: vec![
                HirMatchArm {
                    matcher: HirMatcher::Variant(0),
                    bindings: vec![HirPatternBinding {
                        id: continue_local,
                        name: "async.loop.continue".to_owned(),
                        ty: step.carry.clone(),
                        path: vec![1],
                        moves: true,
                    }],
                    guard: None,
                    body: continue_poll,
                },
                HirMatchArm {
                    matcher: HirMatcher::Variant(1),
                    bindings: vec![HirPatternBinding {
                        id: break_local,
                        name: "async.loop.break".to_owned(),
                        ty: step.output.clone(),
                        path: vec![2],
                        moves: true,
                    }],
                    guard: None,
                    body: HirExpr {
                        ty: Ty::Never,
                        kind: HirExprKind::Block(
                            vec![HirStmt::Expr(set_state(self_ty, state.completed))],
                            Some(Box::new(HirExpr {
                                ty: Ty::Never,
                                kind: HirExprKind::Break(Some(Box::new(poll_ready(
                                    poll_ty,
                                    poll_name,
                                    parent_output,
                                    break_value,
                                )))),
                            })),
                        ),
                    },
                },
            ],
        },
    };
    let child_ready = HirExpr {
        ty: Ty::Never,
        kind: HirExprKind::Block(vec![HirStmt::Expr(take_child)], Some(Box::new(step_match))),
    };
    let iteration = HirExpr {
        ty: Ty::Never,
        kind: HirExprKind::Match {
            scrutinee: Box::new(call),
            arms: vec![
                HirMatchArm {
                    matcher: HirMatcher::Variant(0),
                    bindings: Vec::new(),
                    guard: None,
                    body: HirExpr {
                        ty: Ty::Never,
                        kind: HirExprKind::Return(Some(Box::new(poll_pending(poll_ty, poll_name)))),
                    },
                },
                HirMatchArm {
                    matcher: HirMatcher::Variant(1),
                    bindings: vec![HirPatternBinding {
                        id: state.output_local,
                        name: "awaited.output".to_owned(),
                        ty: awaited.output.clone(),
                        path: vec![1],
                        moves: true,
                    }],
                    guard: None,
                    body: child_ready,
                },
            ],
        },
    };
    HirExpr {
        ty: poll_ty.clone(),
        kind: HirExprKind::Loop {
            body: Box::new(iteration),
        },
    }
}

fn state_is(self_ty: &Ty, state: i128) -> HirExpr {
    HirExpr {
        ty: Ty::Bool,
        kind: HirExprKind::Binary(
            Box::new(HirExpr {
                ty: Ty::I32,
                kind: HirExprKind::Read {
                    place: async_field_place(0, self_ty.clone(), 0, Ty::I32),
                    kind: HirReadKind::Copy,
                },
            }),
            crate::ast::BinaryOp::Eq,
            Box::new(HirExpr {
                ty: Ty::I32,
                kind: HirExprKind::Integer(state),
            }),
        ),
    }
}

fn set_state(self_ty: &Ty, state: i128) -> HirExpr {
    HirExpr {
        ty: Ty::Unit,
        kind: HirExprKind::Assign {
            place: async_field_place(0, self_ty.clone(), 0, Ty::I32),
            value: Box::new(HirExpr {
                ty: Ty::I32,
                kind: HirExprKind::Integer(state),
            }),
            assignment: AssignmentKind::Overwrite,
            root_initialized: true,
        },
    }
}

fn poll_pending(poll_ty: &Ty, poll_name: &str) -> HirExpr {
    HirExpr {
        ty: poll_ty.clone(),
        kind: HirExprKind::ConstructEnum {
            name: poll_name.to_owned(),
            variant: 0,
            fields: Vec::new(),
        },
    }
}

fn poll_ready(poll_ty: &Ty, poll_name: &str, output: &Ty, value: HirExpr) -> HirExpr {
    HirExpr {
        ty: poll_ty.clone(),
        kind: HirExprKind::ConstructEnum {
            name: poll_name.to_owned(),
            variant: 1,
            fields: vec![(
                0,
                HirExpr {
                    ty: output.clone(),
                    ..value
                },
            )],
        },
    }
}

fn trap() -> HirExpr {
    HirExpr {
        ty: Ty::Never,
        kind: HirExprKind::RawTrap,
    }
}

fn async_field_place(local: usize, state: Ty, field: usize, ty: Ty) -> HirPlace {
    HirPlace {
        local,
        root_ty: state,
        projections: vec![field],
        dynamic_index: None,
        ty,
        capability: LocalCapability::MutParam,
        root_mutable: true,
        loan: None,
        indirect: true,
    }
}

#[derive(Debug, Clone)]
pub(super) struct AsyncFutureInfo {
    pub(super) resume: String,
    pub(super) output: Ty,
    pub(super) unsafe_effect: bool,
    pub(super) throws_error: Option<Ty>,
    pub(super) custom_effects: Vec<String>,
    pub(super) capture_modes: Vec<PassMode>,
    pub(super) awaited: Option<AwaitedFutureInfo>,
}

#[derive(Debug, Clone)]
pub(super) struct AwaitedFutureInfo {
    pub(super) ty: Ty,
    pub(super) factory_output: Ty,
    pub(super) output: Ty,
    pub(super) poll_ty: Ty,
    pub(super) poll_function: String,
    pub(super) unsafe_effect: bool,
    pub(super) field: usize,
    pub(super) continuation: Option<String>,
    pub(super) continuation_output: Option<Ty>,
    pub(super) continuation_unsafe_effect: bool,
    pub(super) continuation_capture_modes: Vec<PassMode>,
    pub(super) continuation_fields: Vec<usize>,
    pub(super) continuation_capture_types: Vec<Ty>,
    pub(super) retained_fields: Vec<usize>,
    pub(super) retained_types: Vec<Ty>,
    pub(super) retained_modes: Vec<PassMode>,
    pub(super) next: Option<Box<AwaitedFutureInfo>>,
    pub(super) loop_step: Option<AsyncLoopStepInfo>,
    pub(super) loop_condition: Option<AsyncLoopConditionInfo>,
    pub(super) loop_carry_fields: Vec<usize>,
    pub(super) loop_carry_types: Vec<Ty>,
    pub(super) residual_continuation: Option<AsyncResidualContinuationInfo>,
}

#[derive(Debug, Clone)]
pub(super) struct AsyncResidualContinuationInfo {
    pub(super) function: String,
    pub(super) body: Expr,
    pub(super) parameters: Vec<Param>,
    pub(super) result: Type,
    pub(super) effects: FunctionEffects,
}

#[derive(Debug, Clone)]
pub(super) struct AsyncLoopStepInfo {
    pub(super) ty: Ty,
    pub(super) carry: Ty,
    pub(super) output: Ty,
}

#[derive(Debug, Clone)]
pub(super) struct AsyncLoopConditionInfo {
    pub(super) function: String,
    pub(super) post_test: bool,
    pub(super) capture_modes: Vec<PassMode>,
    pub(super) fields: Vec<usize>,
    pub(super) capture_types: Vec<Ty>,
}

#[derive(Debug, Clone)]
pub(super) struct InternalAsyncLoopConstructor {
    pub(super) name: String,
    pub(super) ty: Ty,
    pub(super) variant: usize,
    pub(super) field: Ty,
}

struct AsyncSourcePlan {
    factory_body: Expr,
    has_await: bool,
    continuation: Option<AsyncContinuationSource>,
    retained: Vec<AsyncRetainedSource>,
    loop_step: Option<AsyncLoopStepSource>,
    loop_condition: Option<AsyncLoopConditionSource>,
}

struct AsyncContinuationSource {
    name: String,
    mutable: bool,
    body: Expr,
}

struct AsyncRetainedSource {
    name: String,
    referent: Option<String>,
    borrowed: bool,
}

struct AsyncLoopStepSource {
    binding: String,
    break_value: Expr,
    output_hint: Option<Ty>,
    probe_awaits: Vec<(String, Expr)>,
    carry_names: Vec<String>,
    continue_constructor: String,
    break_constructor: String,
}

struct AsyncLoopConditionSource {
    expression: Expr,
    post_test: bool,
}

#[derive(Clone, Copy)]
enum AsyncLoopKind {
    Loop,
    While,
    DoWhile,
}

impl AsyncLoopKind {
    fn description(self) -> &'static str {
        match self {
            Self::Loop => "`loop`",
            Self::While => "pre-test `while`",
            Self::DoWhile => "post-test `while`",
        }
    }
}

struct AsyncLoopSuspensionSource {
    kind: AsyncLoopKind,
    condition_suspends: bool,
    body_suspends: bool,
    has_continue: bool,
    has_fallthrough: bool,
    has_value_break: bool,
}

fn recurring_suspended_loop_source(expression: &Expr) -> Option<AsyncLoopSuspensionSource> {
    match expression.unlocated() {
        Expr::Loop { body } if terminating_loop_iteration(body).is_none() => {
            let body_suspends = split_async_source(body).has_await;
            body_suspends.then(|| async_loop_source(AsyncLoopKind::Loop, false, true, body))
        }
        Expr::While {
            condition,
            body,
            post_test,
        } if terminating_loop_iteration(body).is_none() => {
            let condition_suspends = split_async_source(condition).has_await;
            let body_suspends = split_async_source(body).has_await;
            (condition_suspends || body_suspends).then(|| {
                async_loop_source(
                    if *post_test {
                        AsyncLoopKind::DoWhile
                    } else {
                        AsyncLoopKind::While
                    },
                    condition_suspends,
                    body_suspends,
                    body,
                )
            })
        }
        Expr::Block(statements, tail) => statements
            .iter()
            .find_map(|statement| match statement {
                Stmt::Let(binding) => recurring_suspended_loop_source(&binding.value),
                Stmt::Expr(expression) => recurring_suspended_loop_source(expression),
            })
            .or_else(|| tail.as_deref().and_then(recurring_suspended_loop_source)),
        _ => None,
    }
}

fn async_loop_source(
    kind: AsyncLoopKind,
    condition_suspends: bool,
    body_suspends: bool,
    body: &Expr,
) -> AsyncLoopSuspensionSource {
    let recursive_name = "$async$loop$analysis$continue";
    let break_name = "$handler$loop$break$async-analysis";
    let mut rewritten = body.clone();
    super::handlers::rewrite_handler_loop_control(&mut rewritten, recursive_name, break_name, 0);
    let mut has_continue = false;
    let mut has_value_break = false;
    super::source_rewrite::visit_expr_mut(&mut rewritten, &mut |expression| {
        if matches!(expression.unlocated(), Expr::Name(name) if name == recursive_name) {
            has_continue = true;
        }
        if let Some((name, value)) =
            super::handlers::internal_handler_loop_break_argument(expression.unlocated())
        {
            if name == break_name && !matches!(value.unlocated(), Expr::Unit) {
                has_value_break = true;
            }
        }
    });
    AsyncLoopSuspensionSource {
        kind,
        condition_suspends,
        body_suspends,
        has_continue,
        has_fallthrough: !iteration_body_definitely_exits(body),
        has_value_break,
    }
}

fn iteration_body_definitely_exits(expression: &Expr) -> bool {
    match expression.unlocated() {
        Expr::Break(_) | Expr::Continue | Expr::Return(_) => true,
        Expr::Block(statements, tail) => tail.as_deref().map_or_else(
            || {
                statements.last().is_some_and(|statement| match statement {
                    Stmt::Expr(expression) => iteration_body_definitely_exits(expression),
                    Stmt::Let(_) => false,
                })
            },
            iteration_body_definitely_exits,
        ),
        Expr::If {
            then_branch,
            else_branch: Some(else_branch),
            ..
        } => {
            iteration_body_definitely_exits(then_branch)
                && iteration_body_definitely_exits(else_branch)
        }
        Expr::Match { arms, .. } if !arms.is_empty() => arms
            .iter()
            .all(|arm| iteration_body_definitely_exits(&arm.body)),
        _ => false,
    }
}

fn general_unit_recurring_loop_source(body: &Expr, id: usize) -> Option<AsyncSourcePlan> {
    let loop_expression = match body.unlocated() {
        Expr::Loop { .. } | Expr::While { .. } => body,
        Expr::Block(statements, Some(tail)) if statements.is_empty() => tail,
        Expr::Block(statements, None) => {
            let [Stmt::Expr(expression)] = statements.as_slice() else {
                return None;
            };
            expression
        }
        _ => return None,
    };
    let (loop_body, loop_condition) = match loop_expression.unlocated() {
        Expr::Loop { body } => (body.as_ref(), None),
        Expr::While {
            condition,
            body,
            post_test,
        } => (
            body.as_ref(),
            Some(AsyncLoopConditionSource {
                expression: (**condition).clone(),
                post_test: *post_test,
            }),
        ),
        _ => return None,
    };
    if !split_async_source(loop_body).has_await {
        return None;
    }

    let continue_constructor = format!("$async$loop$continue${id}");
    let break_constructor = format!("$async$loop$break${id}");
    let recursive_name = format!("$async$loop$rewrite$continue${id}");
    let handler_break_name = format!("$handler$loop$break$async-rewrite${id}");
    let handler_return_name = format!("$handler$return${recursive_name}");
    let construct = |name: &str, value: Expr| {
        Expr::Call(
            Box::new(Expr::Name(name.to_owned())),
            vec![crate::ast::CallArg { label: None, value }],
        )
    };
    let continue_step = construct(&continue_constructor, Expr::Unit);
    let mut iteration = loop_body.clone();
    super::handlers::rewrite_handler_loop_control(
        &mut iteration,
        &recursive_name,
        &handler_break_name,
        0,
    );
    let mut has_break = false;
    let mut non_unit_break = false;
    super::source_rewrite::visit_expr_mut(&mut iteration, &mut |expression| {
        let Expr::Call(callee, arguments) = expression.unlocated() else {
            return;
        };
        if !matches!(callee.unlocated(), Expr::Name(name) if name == &handler_return_name) {
            return;
        }
        let [argument] = arguments.as_slice() else {
            return;
        };
        let replacement = if matches!(
            argument.value.unlocated(),
            Expr::Call(inner, arguments)
                if matches!(inner.unlocated(), Expr::Name(name) if name == &recursive_name)
                    && arguments.is_empty()
        ) {
            Some(continue_step.clone())
        } else if let Some((name, value)) =
            super::handlers::internal_handler_loop_break_argument(argument.value.unlocated())
        {
            if name != handler_break_name {
                None
            } else {
                has_break = true;
                non_unit_break |= !matches!(value.unlocated(), Expr::Unit);
                Some(construct(&break_constructor, value))
            }
        } else {
            None
        };
        if let Some(replacement) = replacement {
            *expression = Expr::Return(Some(Box::new(replacement)));
        }
    });
    if non_unit_break {
        return None;
    }
    append_async_iteration_fallthrough(&mut iteration, &continue_step);
    Some(AsyncSourcePlan {
        factory_body: Expr::Async {
            body: Box::new(iteration),
        },
        has_await: true,
        continuation: None,
        retained: Vec::new(),
        loop_step: Some(AsyncLoopStepSource {
            binding: String::new(),
            break_value: Expr::Unit,
            output_hint: Some(if has_break || loop_condition.is_some() {
                Ty::Unit
            } else {
                Ty::Never
            }),
            probe_awaits: Vec::new(),
            carry_names: Vec::new(),
            continue_constructor,
            break_constructor,
        }),
        loop_condition,
    })
}

fn append_async_iteration_fallthrough(expression: &mut Expr, continue_step: &Expr) {
    match expression.unlocated_mut() {
        Expr::Return(_) => {}
        Expr::Block(_, Some(tail)) => {
            append_async_iteration_fallthrough(tail, continue_step);
        }
        Expr::Block(_, tail @ None) => {
            *tail = Some(Box::new(Expr::Return(Some(Box::new(
                continue_step.clone(),
            )))));
        }
        Expr::If {
            then_branch,
            else_branch,
            ..
        } => {
            append_async_iteration_fallthrough(then_branch, continue_step);
            if let Some(else_branch) = else_branch {
                append_async_iteration_fallthrough(else_branch, continue_step);
            } else {
                *else_branch = Some(Box::new(Expr::Return(Some(Box::new(
                    continue_step.clone(),
                )))));
            }
        }
        Expr::Match { arms, .. } => {
            for arm in arms {
                append_async_iteration_fallthrough(&mut arm.body, continue_step);
            }
        }
        _ => {
            let value = std::mem::replace(expression, Expr::Unit);
            *expression = Expr::Block(
                vec![Stmt::Expr(value)],
                Some(Box::new(Expr::Return(Some(Box::new(
                    continue_step.clone(),
                ))))),
            );
        }
    }
}

fn multiple_await_recurring_loop_source(body: &Expr, id: usize) -> Option<AsyncSourcePlan> {
    let loop_expression = match body.unlocated() {
        Expr::Loop { .. } | Expr::While { .. } => body,
        Expr::Block(statements, Some(tail)) if statements.is_empty() => tail,
        Expr::Block(statements, None) => {
            let [Stmt::Expr(expression)] = statements.as_slice() else {
                return None;
            };
            expression
        }
        _ => return None,
    };
    let (loop_body, loop_condition) = match loop_expression.unlocated() {
        Expr::Loop { body } => (body.as_ref(), None),
        Expr::While {
            condition,
            body,
            post_test,
        } => (
            body.as_ref(),
            Some(AsyncLoopConditionSource {
                expression: (**condition).clone(),
                post_test: *post_test,
            }),
        ),
        _ => return None,
    };
    let Expr::Block(statements, tail) = loop_body.unlocated() else {
        return None;
    };
    let (iteration_statements, decision) = match (statements.as_slice(), tail.as_deref()) {
        (statements, Some(decision)) => (statements, Some(decision)),
        ([prefix @ .., Stmt::Expr(decision)], None) => (prefix, Some(decision)),
        (statements, None) if loop_condition.is_some() => (statements, None),
        _ => return None,
    };
    let probe_awaits = iteration_statements
        .iter()
        .filter_map(|statement| {
            let Stmt::Let(binding) = statement else {
                return None;
            };
            let Expr::Await(child) = binding.value.unlocated() else {
                return None;
            };
            Some((binding.name.clone(), (**child).clone()))
        })
        .collect::<Vec<_>>();
    if probe_awaits.len() < 2 {
        return None;
    }

    let continue_constructor = format!("$async$loop$continue${id}");
    let break_constructor = format!("$async$loop$break${id}");
    let construct = |name: &str, value: Expr| {
        Expr::Call(
            Box::new(Expr::Name(name.to_owned())),
            vec![crate::ast::CallArg { label: None, value }],
        )
    };
    let continue_step = construct(&continue_constructor, Expr::Unit);
    let (rewritten_decision, break_value) = if let Some(decision) = decision {
        let (condition, then_control, else_control) = simple_loop_decision(decision)?;
        let break_value = match (&then_control, &else_control) {
            (SimpleLoopControl::Break(value), control) if control.continues() => value.clone(),
            (control, SimpleLoopControl::Break(value)) if control.continues() => value.clone(),
            _ => return None,
        };
        let break_step = construct(&break_constructor, break_value.clone());
        let lower_control = |control: SimpleLoopControl| match control {
            SimpleLoopControl::Break(_) => break_step.clone(),
            SimpleLoopControl::Continue => continue_step.clone(),
            SimpleLoopControl::Fallthrough(expression) => Expr::Block(
                vec![Stmt::Expr(expression)],
                Some(Box::new(continue_step.clone())),
            ),
        };
        (
            Expr::If {
                condition: Box::new(condition),
                then_branch: Box::new(lower_control(then_control)),
                else_branch: Some(Box::new(lower_control(else_control))),
            },
            break_value,
        )
    } else {
        (continue_step, Expr::Unit)
    };
    Some(AsyncSourcePlan {
        factory_body: Expr::Async {
            body: Box::new(Expr::Block(
                iteration_statements.to_vec(),
                Some(Box::new(rewritten_decision)),
            )),
        },
        has_await: true,
        continuation: None,
        retained: Vec::new(),
        loop_step: Some(AsyncLoopStepSource {
            binding: String::new(),
            break_value,
            output_hint: None,
            probe_awaits,
            carry_names: Vec::new(),
            continue_constructor,
            break_constructor,
        }),
        loop_condition,
    })
}

fn simple_recurring_async_loop_source(body: &Expr, id: usize) -> Option<AsyncSourcePlan> {
    let loop_expression = match body.unlocated() {
        Expr::Loop { .. } | Expr::While { .. } => body,
        Expr::Block(statements, Some(tail)) if statements.is_empty() => tail,
        Expr::Block(statements, None) => {
            let [Stmt::Expr(expression)] = statements.as_slice() else {
                return None;
            };
            expression
        }
        _ => return None,
    };
    let (loop_body, loop_condition) = match loop_expression.unlocated() {
        Expr::Loop { body } => (body.as_ref(), None),
        Expr::While {
            condition,
            body,
            post_test,
        } => (
            body.as_ref(),
            Some(AsyncLoopConditionSource {
                expression: (**condition).clone(),
                post_test: *post_test,
            }),
        ),
        _ => return None,
    };
    let Expr::Block(statements, tail) = loop_body.unlocated() else {
        return None;
    };
    let (iteration_statements, decision) = match (statements.as_slice(), tail.as_deref()) {
        (statements, Some(decision)) => (statements, Some(decision)),
        ([prefix @ .., Stmt::Expr(decision)], None) => (prefix, Some(decision)),
        (statements, None) if loop_condition.is_some() => (statements, None),
        _ => return None,
    };
    let (await_statement, prefix) = iteration_statements.split_last()?;
    let Stmt::Let(binding) = await_statement else {
        return None;
    };
    let child = match binding.value.unlocated() {
        Expr::Await(child) => (**child).clone(),
        expression => hoist_control_await(expression)?,
    };
    let factory_body = if prefix.is_empty() {
        child
    } else {
        Expr::Block(prefix.to_vec(), Some(Box::new(child)))
    };
    let (condition, then_control, else_control) = decision.map_or_else(
        || {
            Some((
                Expr::Bool(false),
                SimpleLoopControl::Break(Expr::Unit),
                SimpleLoopControl::Continue,
            ))
        },
        simple_loop_decision,
    )?;
    let break_value = match (&then_control, &else_control) {
        (SimpleLoopControl::Break(value), control) if control.continues() => Some(value.clone()),
        (control, SimpleLoopControl::Break(value)) if control.continues() => Some(value.clone()),
        (then_control, else_control) if then_control.continues() && else_control.continues() => {
            None
        }
        _ => return None,
    };
    let continue_constructor = format!("$async$loop$continue${id}");
    let break_constructor = format!("$async$loop$break${id}");
    let construct = |name: &str, value: Expr| {
        Expr::Call(
            Box::new(Expr::Name(name.to_owned())),
            vec![crate::ast::CallArg { label: None, value }],
        )
    };
    let continue_step = construct(&continue_constructor, Expr::Unit);
    let break_step = break_value
        .as_ref()
        .map(|value| construct(&break_constructor, value.clone()));
    let lower_control = |control: SimpleLoopControl| match control {
        SimpleLoopControl::Break(_) => break_step
            .clone()
            .expect("a source break has an internal break constructor"),
        SimpleLoopControl::Continue => continue_step.clone(),
        SimpleLoopControl::Fallthrough(expression) => Expr::Block(
            vec![Stmt::Expr(expression)],
            Some(Box::new(continue_step.clone())),
        ),
    };
    let then_branch = lower_control(then_control);
    let else_branch = lower_control(else_control);
    Some(AsyncSourcePlan {
        factory_body,
        has_await: true,
        continuation: Some(AsyncContinuationSource {
            name: binding.name.clone(),
            mutable: binding.mutable,
            body: Expr::If {
                condition: Box::new(condition),
                then_branch: Box::new(then_branch),
                else_branch: Some(Box::new(else_branch)),
            },
        }),
        retained: Vec::new(),
        loop_step: Some(AsyncLoopStepSource {
            binding: binding.name.clone(),
            break_value: break_value.clone().unwrap_or(Expr::Unit),
            output_hint: break_value.is_none().then_some(Ty::Never),
            probe_awaits: Vec::new(),
            carry_names: {
                let mut names = decision.map(referenced_names).unwrap_or_default();
                names.remove(&binding.name);
                let mut names = names.into_iter().collect::<Vec<_>>();
                names.sort();
                names
            },
            continue_constructor,
            break_constructor,
        }),
        loop_condition,
    })
}

enum SimpleLoopControl {
    Break(Expr),
    Continue,
    Fallthrough(Expr),
}

impl SimpleLoopControl {
    fn continues(&self) -> bool {
        matches!(self, Self::Continue | Self::Fallthrough(_))
    }
}

fn simple_loop_decision(expression: &Expr) -> Option<(Expr, SimpleLoopControl, SimpleLoopControl)> {
    match expression.unlocated() {
        Expr::If {
            condition,
            then_branch,
            else_branch,
        } => Some((
            (**condition).clone(),
            simple_loop_control(then_branch)?,
            else_branch
                .as_deref()
                .map_or(Some(SimpleLoopControl::Continue), simple_loop_control)?,
        )),
        Expr::Match { scrutinee, arms } if arms.len() == 2 => {
            let true_arm = arms
                .iter()
                .find(|arm| matches!(arm.pattern, crate::ast::Pattern::Bool(true)))?;
            let false_arm = arms
                .iter()
                .find(|arm| matches!(arm.pattern, crate::ast::Pattern::Bool(false)))?;
            if true_arm.guard.is_some() || false_arm.guard.is_some() {
                return None;
            }
            Some((
                (**scrutinee).clone(),
                simple_loop_control(&true_arm.body)?,
                simple_loop_control(&false_arm.body)?,
            ))
        }
        _ => None,
    }
}

fn simple_loop_control(expression: &Expr) -> Option<SimpleLoopControl> {
    match expression.unlocated() {
        Expr::Break(value) => Some(SimpleLoopControl::Break(
            value.as_deref().cloned().unwrap_or(Expr::Unit),
        )),
        Expr::Continue => Some(SimpleLoopControl::Continue),
        Expr::Unit => Some(SimpleLoopControl::Continue),
        Expr::Block(statements, Some(tail)) if statements.is_empty() => simple_loop_control(tail),
        Expr::Block(statements, None) => {
            if let [Stmt::Expr(statement)] = statements.as_slice() {
                if let Some(control) = simple_loop_control(statement) {
                    return Some(control);
                }
            }
            is_simple_loop_fallthrough(expression)
                .then(|| SimpleLoopControl::Fallthrough(expression.clone()))
        }
        _ if is_simple_loop_fallthrough(expression) => {
            Some(SimpleLoopControl::Fallthrough(expression.clone()))
        }
        _ => None,
    }
}

fn is_simple_loop_fallthrough(expression: &Expr) -> bool {
    let mut expression = expression.clone();
    let mut supported = true;
    super::source_rewrite::visit_expr_mut(&mut expression, &mut |expression| {
        if matches!(
            expression.unlocated(),
            Expr::Await(_)
                | Expr::Break(_)
                | Expr::Continue
                | Expr::Return(_)
                | Expr::Loop { .. }
                | Expr::While { .. }
        ) {
            supported = false;
        }
    });
    supported
}

fn rewrite_async_loop_continue_carry(expression: &mut Expr, constructor: &str, carry: &Expr) {
    super::source_rewrite::visit_expr_mut(expression, &mut |expression| {
        let Expr::Call(callee, arguments) = expression.unlocated_mut() else {
            return;
        };
        if !matches!(callee.unlocated(), Expr::Name(name) if name == constructor) {
            return;
        }
        let [argument] = arguments.as_mut_slice() else {
            return;
        };
        argument.value = carry.clone();
    });
}

fn split_async_source(body: &Expr) -> AsyncSourcePlan {
    let mut body = body.clone();
    match body.unlocated_mut() {
        Expr::Await(operand) => {
            return AsyncSourcePlan {
                factory_body: (**operand).clone(),
                has_await: true,
                continuation: None,
                retained: Vec::new(),
                loop_step: None,
                loop_condition: None,
            };
        }
        expression if hoist_control_await(expression).is_some() => {
            return AsyncSourcePlan {
                factory_body: hoist_control_await(expression)
                    .expect("checked control-flow await hoisting"),
                has_await: true,
                continuation: None,
                retained: Vec::new(),
                loop_step: None,
                loop_condition: None,
            };
        }
        Expr::Block(statements, Some(tail)) => {
            if let Expr::Await(operand) = tail.unlocated() {
                if !statements.is_empty() {
                    let result = "$async$tail$result".to_owned();
                    let mut rewritten = statements.clone();
                    rewritten.push(Stmt::Let(crate::ast::Binding {
                        mutable: false,
                        name: result.clone(),
                        annotation: None,
                        value: Expr::Await(Box::new((**operand).clone())),
                        value_source: None,
                    }));
                    rewritten.extend(
                        non_borrow_binding_names(statements)
                            .into_iter()
                            .map(|name| Stmt::Expr(Expr::Name(name))),
                    );
                    return split_async_source(&Expr::Block(
                        rewritten,
                        Some(Box::new(Expr::Name(result))),
                    ));
                }
                **tail = (**operand).clone();
                return AsyncSourcePlan {
                    factory_body: body,
                    has_await: true,
                    continuation: None,
                    retained: Vec::new(),
                    loop_step: None,
                    loop_condition: None,
                };
            }
            if let Some(hoisted) = hoist_control_await(tail) {
                **tail = hoisted;
                return AsyncSourcePlan {
                    factory_body: body,
                    has_await: true,
                    continuation: None,
                    retained: Vec::new(),
                    loop_step: None,
                    loop_condition: None,
                };
            }
        }
        _ => {}
    }
    let Expr::Block(statements, tail) = body.unlocated() else {
        return AsyncSourcePlan {
            factory_body: body,
            has_await: false,
            continuation: None,
            retained: Vec::new(),
            loop_step: None,
            loop_condition: None,
        };
    };
    let Some((position, binding, operand)) =
        statements
            .iter()
            .enumerate()
            .find_map(|(position, statement)| {
                let Stmt::Let(binding) = statement else {
                    return None;
                };
                let operand = match binding.value.unlocated() {
                    Expr::Await(operand) => (**operand).clone(),
                    expression => hoist_control_await(expression)?,
                };
                Some((position, binding, operand))
            })
    else {
        return AsyncSourcePlan {
            factory_body: body,
            has_await: false,
            continuation: None,
            retained: Vec::new(),
            loop_step: None,
            loop_condition: None,
        };
    };
    let continuation_body = Expr::Block(statements[position + 1..].to_vec(), tail.clone());
    let mut referenced = referenced_names(&continuation_body);
    let dependencies = statements[..position]
        .iter()
        .filter_map(|statement| {
            let Stmt::Let(binding) = statement else {
                return None;
            };
            Some((
                binding.name.clone(),
                async_initializer_root(&binding.value)?,
            ))
        })
        .collect::<Vec<_>>();
    let borrowed_names = borrowed_binding_names(&statements[..position]);
    loop {
        let mut changed = false;
        for (binding, referent) in &dependencies {
            if referenced.contains(binding) {
                changed |= referenced.insert(referent.clone());
            }
        }
        if !changed {
            break;
        }
    }
    let mut retained = Vec::<AsyncRetainedSource>::new();
    for statement in &statements[..position] {
        let Stmt::Let(binding) = statement else {
            continue;
        };
        if !referenced.contains(&binding.name) {
            continue;
        }
        let referent = async_initializer_root(&binding.value)
            .map(|referent| resolve_async_dependency(&referent, &dependencies));
        if let Some(existing) = retained
            .iter_mut()
            .find(|retained| retained.name == binding.name)
        {
            *existing = AsyncRetainedSource {
                name: binding.name.clone(),
                referent,
                borrowed: borrowed_names.contains(&binding.name),
            };
        } else {
            retained.push(AsyncRetainedSource {
                name: binding.name.clone(),
                referent,
                borrowed: borrowed_names.contains(&binding.name),
            });
        }
    }
    let factory_tail = if retained.is_empty() {
        operand.clone()
    } else {
        Expr::Tuple(
            std::iter::once(operand.clone())
                .chain(
                    retained
                        .iter()
                        .map(|retained| Expr::Name(retained.name.clone())),
                )
                .collect(),
        )
    };
    AsyncSourcePlan {
        factory_body: Expr::Block(
            statements[..position].to_vec(),
            Some(Box::new(factory_tail)),
        ),
        has_await: true,
        continuation: Some(AsyncContinuationSource {
            name: binding.name.clone(),
            mutable: binding.mutable,
            body: continuation_body,
        }),
        retained,
        loop_step: None,
        loop_condition: None,
    }
}

fn hoist_control_await(expression: &Expr) -> Option<Expr> {
    match expression.unlocated() {
        Expr::If {
            condition,
            then_branch,
            else_branch: Some(else_branch),
        } => {
            let then_future = branch_await_future(then_branch);
            let else_future = branch_await_future(else_branch);
            if then_future.is_none() && else_future.is_none() {
                return None;
            }
            Some(Expr::If {
                condition: condition.clone(),
                then_branch: Box::new(then_future.unwrap_or_else(|| Expr::Async {
                    body: then_branch.clone(),
                })),
                else_branch: Some(Box::new(else_future.unwrap_or_else(|| Expr::Async {
                    body: else_branch.clone(),
                }))),
            })
        }
        Expr::Match { scrutinee, arms } if !arms.is_empty() => {
            let futures = arms
                .iter()
                .map(|arm| branch_await_future(&arm.body))
                .collect::<Vec<_>>();
            if futures.iter().all(Option::is_none) {
                return None;
            }
            let mut hoisted_arms = Vec::with_capacity(arms.len());
            for (arm, future) in arms.iter().zip(futures) {
                let mut arm = arm.clone();
                arm.body = future.unwrap_or_else(|| Expr::Async {
                    body: Box::new(arm.body.clone()),
                });
                hoisted_arms.push(arm);
            }
            Some(Expr::Match {
                scrutinee: scrutinee.clone(),
                arms: hoisted_arms,
            })
        }
        Expr::Loop { body } => {
            let (iteration, _) = terminating_loop_iteration(body)?;
            branch_await_future(&iteration)
        }
        Expr::While {
            condition,
            body,
            post_test,
        } => {
            let (iteration, break_value) = terminating_loop_iteration(body)?;
            if break_value.is_some() {
                return None;
            }
            if *post_test {
                return branch_await_future(&iteration);
            }
            let condition_future = branch_await_future(condition);
            let iteration_future = branch_await_future(&iteration);
            match (condition_future, iteration_future) {
                (None, Some(iteration)) => Some(Expr::If {
                    condition: condition.clone(),
                    then_branch: Box::new(iteration),
                    else_branch: Some(Box::new(Expr::Async {
                        body: Box::new(Expr::Unit),
                    })),
                }),
                (Some(condition), None) => Some(Expr::Async {
                    body: Box::new(Expr::Block(
                        vec![Stmt::Let(crate::ast::Binding {
                            mutable: false,
                            name: "$async$while$condition".to_owned(),
                            annotation: None,
                            value: Expr::Await(Box::new(condition)),
                            value_source: None,
                        })],
                        Some(Box::new(Expr::Unit)),
                    )),
                }),
                _ => None,
            }
        }
        _ => None,
    }
}

fn terminating_loop_iteration(body: &Expr) -> Option<(Expr, Option<Expr>)> {
    match body.unlocated() {
        Expr::Break(value) => Some((
            value.as_deref().cloned().unwrap_or(Expr::Unit),
            value.as_deref().cloned(),
        )),
        Expr::Block(statements, tail) => {
            if let Some(Expr::Break(value)) = tail.as_deref().map(Expr::unlocated) {
                return Some((
                    Expr::Block(
                        statements.clone(),
                        Some(Box::new(value.as_deref().cloned().unwrap_or(Expr::Unit))),
                    ),
                    value.as_deref().cloned(),
                ));
            }
            let (last, prefix) = statements.split_last()?;
            let Stmt::Expr(last) = last else {
                return None;
            };
            let Expr::Break(value) = last.unlocated() else {
                return None;
            };
            Some((
                Expr::Block(
                    prefix.to_vec(),
                    Some(Box::new(value.as_deref().cloned().unwrap_or(Expr::Unit))),
                ),
                value.as_deref().cloned(),
            ))
        }
        _ => None,
    }
}

fn branch_await_future(expression: &Expr) -> Option<Expr> {
    if let Some(future) = tail_await_operand(expression) {
        return Some(future);
    }
    let Expr::Block(statements, _) = expression.unlocated() else {
        return None;
    };
    statements
        .iter()
        .any(|statement| {
            let Stmt::Let(binding) = statement else {
                return false;
            };
            matches!(binding.value.unlocated(), Expr::Await(_))
                || hoist_control_await(&binding.value).is_some()
        })
        .then(|| Expr::Async {
            body: Box::new(expression.clone()),
        })
}

fn heterogeneous_branch_factory(
    expression: &Expr,
    variants: usize,
) -> Option<(Vec<Stmt>, Expr, Vec<Expr>)> {
    match expression.unlocated() {
        Expr::Match { arms, .. } if !arms.is_empty() && arms.len() == variants => {
            Some((Vec::new(), expression.clone(), Vec::new()))
        }
        Expr::Block(statements, Some(tail)) => match tail.unlocated() {
            Expr::Match { arms, .. } if !arms.is_empty() && arms.len() == variants => {
                Some((statements.clone(), (**tail).clone(), Vec::new()))
            }
            Expr::Tuple(fields) if !fields.is_empty() => {
                let selection = fields.first()?;
                let Expr::Match { arms, .. } = selection.unlocated() else {
                    return None;
                };
                if arms.is_empty() || arms.len() != variants {
                    return None;
                }
                Some((statements.clone(), selection.clone(), fields[1..].to_vec()))
            }
            _ => None,
        },
        _ => None,
    }
}

fn wrap_heterogeneous_branch_factory(
    prefix: Vec<Stmt>,
    mut selection: Expr,
    retained: &[Expr],
    starts: &[String],
) -> Expr {
    match selection.unlocated_mut() {
        Expr::Match { arms, .. } => {
            debug_assert_eq!(arms.len(), starts.len());
            for (variant, (arm, start)) in arms.iter_mut().zip(starts).enumerate() {
                let binding = format!("$async$branch$value${variant}");
                let arguments = std::iter::once(CallArg {
                    label: None,
                    value: Expr::Name(binding.clone()),
                })
                .chain(
                    retained
                        .iter()
                        .cloned()
                        .map(|value| CallArg { label: None, value }),
                )
                .chain(std::iter::once(CallArg {
                    label: None,
                    value: Expr::Name("self".to_owned()),
                }))
                .collect();
                arm.body = Expr::Block(
                    vec![Stmt::Let(crate::ast::Binding {
                        mutable: false,
                        name: binding.clone(),
                        annotation: None,
                        value: arm.body.clone(),
                        value_source: None,
                    })],
                    Some(Box::new(Expr::Call(
                        Box::new(Expr::Name(start.clone())),
                        arguments,
                    ))),
                );
            }
        }
        _ => unreachable!("validated heterogeneous async factory is a match"),
    }
    if prefix.is_empty() {
        selection
    } else {
        Expr::Block(prefix, Some(Box::new(selection)))
    }
}

fn tail_await_operand(expression: &Expr) -> Option<Expr> {
    match expression.unlocated() {
        Expr::Await(future) => Some((**future).clone()),
        Expr::Block(statements, Some(tail)) => {
            let Expr::Await(future) = tail.unlocated() else {
                return None;
            };
            if statements.is_empty() {
                Some((**future).clone())
            } else {
                Some(Expr::Async {
                    body: Box::new(expression.clone()),
                })
            }
        }
        _ => None,
    }
}

fn non_borrow_binding_names(statements: &[Stmt]) -> Vec<String> {
    let borrowed = borrowed_binding_names(statements);
    statements
        .iter()
        .filter_map(|statement| {
            let Stmt::Let(binding) = statement else {
                return None;
            };
            (!borrowed.contains(&binding.name)).then(|| binding.name.clone())
        })
        .collect()
}

fn borrowed_binding_names(statements: &[Stmt]) -> std::collections::HashSet<String> {
    let dependencies = statements
        .iter()
        .filter_map(|statement| {
            let Stmt::Let(binding) = statement else {
                return None;
            };
            Some((
                binding.name.clone(),
                async_initializer_root(&binding.value)?,
            ))
        })
        .collect::<Vec<_>>();
    let mut borrowed = statements
        .iter()
        .filter_map(|statement| {
            let Stmt::Let(binding) = statement else {
                return None;
            };
            (matches!(binding.annotation, Some(crate::ast::Type::Borrow { .. }))
                || matches!(binding.value.unlocated(), Expr::Borrow { .. }))
            .then(|| binding.name.clone())
        })
        .collect::<std::collections::HashSet<_>>();
    loop {
        let mut changed = false;
        for (binding, referent) in &dependencies {
            if borrowed.contains(referent) {
                changed |= borrowed.insert(binding.clone());
            }
        }
        if !changed {
            return borrowed;
        }
    }
}

fn referenced_names(expression: &Expr) -> std::collections::HashSet<String> {
    let mut expression = expression.clone();
    let mut names = std::collections::HashSet::new();
    super::source_rewrite::visit_expr_mut(&mut expression, &mut |expression| {
        if let Expr::Name(name) = expression.unlocated() {
            names.insert(name.clone());
        }
    });
    names
}

fn async_place_root(expression: &Expr) -> Option<String> {
    match expression.unlocated() {
        Expr::Name(name) => Some(name.clone()),
        Expr::Member(base, _) | Expr::Index { base, .. } => async_place_root(base),
        _ => None,
    }
}

fn async_initializer_root(expression: &Expr) -> Option<String> {
    match expression.unlocated() {
        Expr::Borrow { value, .. } => async_place_root(value),
        expression => async_place_root(expression),
    }
}

fn resolve_async_dependency(name: &str, dependencies: &[(String, String)]) -> String {
    let mut resolved = name.to_owned();
    let mut visited = std::collections::HashSet::new();
    while visited.insert(resolved.clone()) {
        let Some((_, referent)) = dependencies
            .iter()
            .rev()
            .find(|(binding, _)| binding == &resolved)
        else {
            break;
        };
        resolved = referent.clone();
    }
    resolved
}

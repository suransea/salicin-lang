use std::collections::HashSet;

use crate::ast::{CallArg, Expr, Param, PassMode, Stmt, Type};
use crate::core::LangItemKind;

use super::compile_time::source_effect_identity;
use super::effects::{handled_operation_call, standard_throws_error_source};
use super::fallible::StandardFallibleKind;
use super::flow::{LocalInfo, LowerCtx};
use super::hir::{
    ClosureCaptureMode, ClosureEffectContext, FunctionSig, FunctionTy, HirArgument, HirExpr,
    HirExprKind, LocalCapability, Ty,
};
use super::lower::{error_expr, flatten_call, TypeProbe};
use super::source_rewrite::source_type_expression_name;
use super::Analyzer;

impl Analyzer {
    pub(super) fn infer_try_result_type(&mut self, body: &Expr, context: &LowerCtx) -> Option<Ty> {
        let payload = match self.probe_expr_ty(body, None, context) {
            TypeProbe::Known(ty) | TypeProbe::KnownSource(ty, _) | TypeProbe::Defaultable(ty) => ty,
            TypeProbe::Unsupported => match body {
                Expr::Block(statements, Some(tail)) if statements.is_empty() => {
                    match self.call_throws_info(tail, context) {
                        Some((payload, _)) => payload,
                        None => {
                            self.error(
                                "cannot infer the success type of `try { ... }`; add a contextual `Result(E)(T)` type",
                            );
                            return None;
                        }
                    }
                }
                Expr::Call(_, _) => match self.call_throws_info(body, context) {
                    Some((payload, _)) => payload,
                    None => {
                        self.error(
                            "cannot infer the success type of `try { ... }`; add a contextual `Result(E)(T)` type",
                        );
                        return None;
                    }
                },
                Expr::Throw(_) => Ty::Never,
                _ => {
                    self.error(
                        "cannot infer the success type of `try { ... }`; add a contextual `Result(E)(T)` type",
                    );
                    return None;
                }
            },
        };
        let mut errors = HashSet::new();
        self.collect_escaping_throws(body, context, &mut errors);
        let error = match errors.len() {
            0 => {
                self.error(
                    "cannot infer `try { ... }` because its body has no escaping throws source; add a contextual `Result(E)(T)` type",
                );
                return None;
            }
            1 => errors.into_iter().next().expect("one inferred error type"),
            _ => {
                let mut names = errors
                    .iter()
                    .map(|error| self.diagnostic_type_name(error))
                    .collect::<Vec<_>>();
                names.sort();
                self.error(format!(
                    "cannot infer `try {{ ... }}` from multiple escaping error types: {}; convert them to one type or add a contextual `Result(E)(T)` type",
                    names
                        .iter()
                        .map(|name| format!("`{name}`"))
                        .collect::<Vec<_>>()
                        .join(", ")
                ));
                return None;
            }
        };
        self.ensure_throws_result_type(payload, error)
    }

    fn throws_info_from_function(&self, function: &FunctionTy) -> Option<(Ty, Ty)> {
        let error = function.throws_error.as_deref()?.clone();
        let payload = self
            .standard_fallible_info_for_ty(&function.result)?
            .payload;
        Some((payload, error))
    }

    fn throws_info_from_signature(&self, signature: &FunctionSig) -> Option<(Ty, Ty)> {
        let error = signature.throws_error.clone()?;
        let result = signature.result.as_ref()?;
        let payload = self.standard_fallible_info_for_ty(result)?.payload;
        Some((payload, error))
    }

    pub(super) fn call_throws_info(
        &self,
        expression: &Expr,
        context: &LowerCtx,
    ) -> Option<(Ty, Ty)> {
        let mut groups = Vec::new();
        let root = flatten_call(expression, &mut groups);
        if let Expr::Name(name) = root {
            if let Some(local) = context.lookup(name) {
                let function = match &local.ty {
                    Ty::Function(function) => function,
                    Ty::Callable(callable) => &callable.signature,
                    _ => return None,
                };
                return (groups.len() == function.groups.len())
                    .then(|| self.throws_info_from_function(function))
                    .flatten();
            }
            if let Some(candidates) = self.function_overloads.get(name) {
                if !groups
                    .iter()
                    .flat_map(|group| group.iter())
                    .any(|argument| argument.label.is_some())
                {
                    return None;
                }
                let matches = self.matching_function_overloads(candidates, &groups, 0);
                let [selected] = matches.as_slice() else {
                    return None;
                };
                let signature = self.signatures.get(selected)?;
                return (groups.len() == signature.groups.len())
                    .then(|| self.throws_info_from_signature(signature))
                    .flatten();
            }
            let signature = self.signatures.get(name)?;
            return (groups.len() == signature.groups.len())
                .then(|| self.throws_info_from_signature(signature))
                .flatten();
        }
        let Expr::Member(base, member) = root else {
            return None;
        };
        if let Some((_, ty, _)) = self.probe_nominal_type_head(base, context) {
            let target = match &ty {
                Ty::Struct(target) | Ty::Enum(target) => target.clone(),
                _ => return None,
            };
            let overload_key = (target.clone(), member.clone(), false);
            let canonical = if let Some(candidates) = self.inherent_overloads.get(&overload_key) {
                if !groups
                    .iter()
                    .flat_map(|group| group.iter())
                    .any(|argument| argument.label.is_some())
                {
                    return None;
                }
                let matches = self.matching_function_overloads(candidates, &groups, 0);
                let [selected] = matches.as_slice() else {
                    return None;
                };
                selected.clone()
            } else if let Some(canonical) = self
                .inherent_members
                .get(&target)
                .and_then(|members| members.functions.get(member))
            {
                canonical.clone()
            } else {
                let candidates =
                    self.trait_associated_function_candidates(&ty, member, &context.origin);
                match candidates.as_slice() {
                    [canonical] => canonical.clone(),
                    [_, _, ..]
                        if groups
                            .iter()
                            .flat_map(|group| group.iter())
                            .any(|argument| argument.label.is_some()) =>
                    {
                        let matches = self.matching_function_overloads(&candidates, &groups, 0);
                        let [selected] = matches.as_slice() else {
                            return None;
                        };
                        selected.clone()
                    }
                    _ => return None,
                }
            };
            let signature = self.signatures.get(&canonical)?;
            return (groups.len() == signature.groups.len())
                .then(|| self.throws_info_from_signature(signature))
                .flatten();
        }
        let receiver = match self.probe_expr_ty(base, None, context) {
            TypeProbe::Known(ty) | TypeProbe::KnownSource(ty, _) => ty,
            TypeProbe::Defaultable(_) | TypeProbe::Unsupported => return None,
        };
        let target = match &receiver {
            Ty::Struct(target) | Ty::Enum(target) => target,
            _ => return None,
        };
        let overload_key = (target.clone(), member.clone(), true);
        let inherent = if let Some(candidates) = self.inherent_overloads.get(&overload_key) {
            if !groups
                .iter()
                .flat_map(|group| group.iter())
                .any(|argument| argument.label.is_some())
            {
                return None;
            }
            let matches = self.matching_function_overloads(candidates, &groups, 1);
            let [selected] = matches.as_slice() else {
                return None;
            };
            Some(selected.clone())
        } else {
            self.inherent_members
                .get(target)
                .and_then(|members| members.methods.get(member))
                .cloned()
        };
        let canonical = if let Some(canonical) = inherent {
            canonical
        } else {
            let candidates =
                self.trait_method_function_candidates(&receiver, member, &context.origin);
            match candidates.as_slice() {
                [(_, canonical)] => canonical.clone(),
                [_, _, ..]
                    if groups
                        .iter()
                        .flat_map(|group| group.iter())
                        .any(|argument| argument.label.is_some()) =>
                {
                    let canonicals = candidates
                        .iter()
                        .map(|(_, canonical)| canonical.clone())
                        .collect::<Vec<_>>();
                    let matches = self.matching_function_overloads(&canonicals, &groups, 1);
                    let [selected] = matches.as_slice() else {
                        return None;
                    };
                    selected.clone()
                }
                _ => return None,
            }
        };
        let signature = self.signatures.get(&canonical)?;
        (groups.len() + 1 == signature.groups.len())
            .then(|| self.throws_info_from_signature(signature))
            .flatten()
    }

    pub(super) fn lower_try(
        &mut self,
        body: &Expr,
        expected: Option<&Ty>,
        context: &mut LowerCtx,
    ) -> HirExpr {
        let expected = match expected.cloned() {
            Some(expected) => expected,
            None => match self.infer_try_result_type(body, context) {
                Some(inferred) => inferred,
                None => {
                    let _ = self.lower_expr(body, None, context);
                    return error_expr();
                }
            },
        };
        let Some(info) = self.standard_fallible_info_for_ty(&expected) else {
            let _ = self.lower_expr(body, None, context);
            self.error(format!(
                "`try {{ ... }}` produces `Result(E)(T)`, but this context expects `{expected}`"
            ));
            return error_expr();
        };
        if info.kind != StandardFallibleKind::Result {
            let _ = self.lower_expr(body, None, context);
            self.error("`try { ... }` requires `Result(E)(T)`, not `Option(T)`");
            return error_expr();
        }
        let error = info.error.expect("Result has an error type");
        if !self.try_body_uses_dedicated_throws_call(body, context)
            && self.try_body_uses_standard_throws(body, &error, context)
        {
            return self.lower_standard_throws_try(body, expected, context);
        }
        let closure = self.lower_local_closure(
            &[],
            body,
            Some(expected.clone()),
            ClosureEffectContext {
                unsafe_depth: context.unsafe_depth,
                throws_error: Some(error),
                custom_effects: context.active_custom_effects.clone(),
                custom_effect_sources: context.active_custom_effect_sources.clone(),
                lexical_handler_effects: context.lexical_handler_effects.clone(),
                lexical_handler_effect_sources: context.lexical_handler_effect_sources.clone(),
            },
            context,
        );
        let HirExprKind::LocalClosure(info) = closure.kind else {
            return error_expr();
        };
        let mut loans = Vec::new();
        let arguments = info
            .captures
            .into_iter()
            .map(|capture| match capture.mode {
                ClosureCaptureMode::Shared => {
                    if let Some(loan) = capture.place.loan {
                        loans.push(loan);
                    }
                    HirArgument::SharedBorrow(capture.place)
                }
                ClosureCaptureMode::Mutable => {
                    if let Some(loan) = capture.place.loan {
                        loans.push(loan);
                    }
                    HirArgument::MutBorrow(capture.place)
                }
                ClosureCaptureMode::Move => HirArgument::Move(
                    *capture
                        .value
                        .expect("move capture stores its evaluated value"),
                ),
            })
            .collect();
        self.release_loans(&loans, context);
        HirExpr {
            ty: expected,
            kind: HirExprKind::Call {
                function: info.function,
                arguments,
                consumed_callable: None,
                diverges: false,
            },
        }
    }

    fn lower_standard_throws_try(
        &mut self,
        body: &Expr,
        expected: Ty,
        context: &mut LowerCtx,
    ) -> HirExpr {
        let Some(info) = self.standard_fallible_info_for_ty(&expected) else {
            self.error("internal error: standard Throws try requires a Result expectation");
            return error_expr();
        };
        let Some(error_source) = info
            .error
            .as_ref()
            .and_then(|error| self.source_type_for_ty(error))
        else {
            self.error("standard Throws try requires a source-level error type");
            return error_expr();
        };
        let Some(payload_source) = info.payload_source.clone() else {
            self.error("standard Throws try requires a source-level success type");
            return error_expr();
        };
        let throws_name = self.lang_item_name(LangItemKind::ThrowsEffect).to_owned();
        let Some(definition) = self.effect_defs.get(&throws_name).cloned() else {
            self.error("compiler core did not register its validated `Throws` effect");
            return error_expr();
        };
        let instance = Type::Named(throws_name, vec![error_source]);
        let inferred = Type::Named("$context$infer".into(), Vec::new());
        let result_name = self.lang_item_name(LangItemKind::Result).to_owned();
        let done_value = "$try$value".to_owned();
        let raise_error = "$try$error".to_owned();
        let clauses = vec![
            CallArg {
                label: Some("done".to_owned()),
                value: Expr::Closure(
                    vec![Param {
                        mode: PassMode::Inferred,
                        access: None,
                        passing: None,
                        region: None,
                        name: done_value.clone(),
                        ty: payload_source,
                    }],
                    Box::new(Expr::Call(
                        Box::new(Expr::Member(
                            Box::new(Expr::Name(result_name.clone())),
                            "Ok".to_owned(),
                        )),
                        vec![CallArg {
                            label: None,
                            value: Expr::Name(done_value),
                        }],
                    )),
                ),
            },
            CallArg {
                label: Some("raise".to_owned()),
                value: Expr::Closure(
                    vec![Param {
                        mode: PassMode::Inferred,
                        access: None,
                        passing: None,
                        region: None,
                        name: raise_error.clone(),
                        ty: inferred,
                    }],
                    Box::new(Expr::Call(
                        Box::new(Expr::Member(
                            Box::new(Expr::Name(result_name)),
                            "Err".to_owned(),
                        )),
                        vec![CallArg {
                            label: None,
                            value: Expr::Name(raise_error),
                        }],
                    )),
                ),
            },
        ];
        let action = vec![CallArg {
            label: None,
            value: Expr::Closure(Vec::new(), Box::new(body.clone())),
        }];
        let groups = vec![clauses.as_slice(), action.as_slice()];
        self.lower_effect_handler(&definition, &instance, &groups, Some(&expected), context)
    }

    fn try_body_uses_standard_throws(
        &self,
        expression: &Expr,
        error: &Ty,
        context: &LowerCtx,
    ) -> bool {
        let Some(error_source) = self.source_type_for_ty(error) else {
            return false;
        };
        let identity = source_effect_identity(&Type::Named(
            self.lang_item_name(LangItemKind::ThrowsEffect).to_owned(),
            vec![error_source],
        ));
        self.expression_uses_standard_throws_identity(expression, &identity, context)
    }

    fn expression_uses_standard_throws_identity(
        &self,
        expression: &Expr,
        identity: &str,
        context: &LowerCtx,
    ) -> bool {
        match expression {
            Expr::Throw(_) => true,
            Expr::Try(_) | Expr::Closure(_, _) => false,
            Expr::Call(callee, arguments) => {
                handled_operation_call(expression, identity).is_some()
                    || self
                        .call_custom_effect_identities(expression, context)
                        .is_some_and(|effects| effects.iter().any(|effect| effect == identity))
                    || self.effect_handler_call_uses_standard_throws_identity(
                        expression, identity, context,
                    )
                    || self.expression_uses_standard_throws_identity(callee, identity, context)
                    || arguments.iter().any(|argument| {
                        self.expression_uses_standard_throws_identity(
                            &argument.value,
                            identity,
                            context,
                        )
                    })
            }
            Expr::Unary(_, value)
            | Expr::Borrow { value, .. }
            | Expr::DoBlock { body: value }
            | Expr::Unsafe(value)
            | Expr::Return(Some(value))
            | Expr::Break(Some(value)) => {
                self.expression_uses_standard_throws_identity(value, identity, context)
            }
            Expr::Return(None) | Expr::Break(None) | Expr::Continue => false,
            Expr::Binary(left, _, right)
            | Expr::Coalesce(left, right)
            | Expr::Assign(left, right)
            | Expr::CompoundAssign(left, _, right) => {
                self.expression_uses_standard_throws_identity(left, identity, context)
                    || self.expression_uses_standard_throws_identity(right, identity, context)
            }
            Expr::HandlerCoalesce {
                scrutinee,
                success,
                fallback,
                ..
            } => {
                self.expression_uses_standard_throws_identity(scrutinee, identity, context)
                    || self.expression_uses_standard_throws_identity(success, identity, context)
                    || self.expression_uses_standard_throws_identity(fallback, identity, context)
            }
            Expr::HandlerChainCall(chain) => {
                self.expression_uses_standard_throws_identity(&chain.scrutinee, identity, context)
                    || chain.groups.iter().flatten().any(|argument| {
                        self.expression_uses_standard_throws_identity(
                            &argument.value,
                            identity,
                            context,
                        )
                    })
                    || self.expression_uses_standard_throws_identity(
                        &chain.success,
                        identity,
                        context,
                    )
                    || self.expression_uses_standard_throws_identity(
                        &chain.residual,
                        identity,
                        context,
                    )
            }
            Expr::Member(base, _) | Expr::ChainMember(base, _) => {
                self.expression_uses_standard_throws_identity(base, identity, context)
            }
            Expr::Array(elements) => elements.iter().any(|element| {
                self.expression_uses_standard_throws_identity(element, identity, context)
            }),
            Expr::StructLiteral { fields, .. } => fields.iter().any(|field| {
                self.expression_uses_standard_throws_identity(&field.value, identity, context)
            }),
            Expr::Index { base, index } => {
                self.expression_uses_standard_throws_identity(base, identity, context)
                    || self.expression_uses_standard_throws_identity(index, identity, context)
            }
            Expr::Block(statements, tail) => {
                statements.iter().any(|statement| match statement {
                    Stmt::Let(binding) => self.expression_uses_standard_throws_identity(
                        &binding.value,
                        identity,
                        context,
                    ),
                    Stmt::Expr(expression) => {
                        self.expression_uses_standard_throws_identity(expression, identity, context)
                    }
                }) || tail.as_ref().is_some_and(|tail| {
                    self.expression_uses_standard_throws_identity(tail, identity, context)
                })
            }
            Expr::If {
                condition,
                then_branch,
                else_branch,
            } => {
                self.expression_uses_standard_throws_identity(condition, identity, context)
                    || self.expression_uses_standard_throws_identity(then_branch, identity, context)
                    || else_branch.as_ref().is_some_and(|else_branch| {
                        self.expression_uses_standard_throws_identity(
                            else_branch,
                            identity,
                            context,
                        )
                    })
            }
            Expr::While { condition, body } => {
                self.expression_uses_standard_throws_identity(condition, identity, context)
                    || self.expression_uses_standard_throws_identity(body, identity, context)
            }
            Expr::Loop { body } => {
                self.expression_uses_standard_throws_identity(body, identity, context)
            }
            Expr::Match { scrutinee, arms } => {
                self.expression_uses_standard_throws_identity(scrutinee, identity, context)
                    || arms.iter().any(|arm| {
                        arm.guard.as_ref().is_some_and(|guard| {
                            self.expression_uses_standard_throws_identity(guard, identity, context)
                        }) || self
                            .expression_uses_standard_throws_identity(&arm.body, identity, context)
                    })
            }
            Expr::Type(_) | Expr::Unit | Expr::Integer(_) | Expr::Bool(_) | Expr::Name(_) => false,
        }
    }

    fn effect_handler_call_uses_standard_throws_identity(
        &self,
        expression: &Expr,
        identity: &str,
        context: &LowerCtx,
    ) -> bool {
        let Expr::Call(inner_callee, action_arguments) = expression else {
            return false;
        };
        let [CallArg {
            label: None,
            value: Expr::Closure(action_parameters, action_body),
        }] = action_arguments.as_slice()
        else {
            return false;
        };
        if !action_parameters.is_empty() {
            return false;
        }
        let mut groups = Vec::new();
        let Expr::Member(effect, member) = flatten_call(inner_callee, &mut groups) else {
            return false;
        };
        if member != "handle" || groups.len() != 1 {
            return false;
        }
        let Some(effect_name) = source_type_expression_name(effect) else {
            return false;
        };
        let root_name = effect_name.split('(').next().unwrap_or(&effect_name);
        if !self.effect_defs.contains_key(root_name) {
            return false;
        }
        groups[0].iter().any(|argument| {
            matches!(
                &argument.value,
                Expr::Closure(_, body)
                    if self.expression_uses_standard_throws_identity(body, identity, context)
            )
        }) || self.expression_uses_standard_throws_identity(action_body, identity, context)
    }

    fn try_body_uses_dedicated_throws_call(&self, expression: &Expr, context: &LowerCtx) -> bool {
        match expression {
            Expr::Try(_) | Expr::Closure(_, _) => false,
            Expr::Call(callee, arguments) => {
                self.call_throws_info(expression, context).is_some()
                    || self.try_body_uses_dedicated_throws_call(callee, context)
                    || arguments.iter().any(|argument| {
                        self.try_body_uses_dedicated_throws_call(&argument.value, context)
                    })
            }
            Expr::Unary(_, value)
            | Expr::Borrow { value, .. }
            | Expr::DoBlock { body: value }
            | Expr::Throw(value)
            | Expr::Unsafe(value)
            | Expr::Return(Some(value))
            | Expr::Break(Some(value)) => self.try_body_uses_dedicated_throws_call(value, context),
            Expr::Return(None) | Expr::Break(None) | Expr::Continue => false,
            Expr::Binary(left, _, right)
            | Expr::Coalesce(left, right)
            | Expr::Assign(left, right)
            | Expr::CompoundAssign(left, _, right) => {
                self.try_body_uses_dedicated_throws_call(left, context)
                    || self.try_body_uses_dedicated_throws_call(right, context)
            }
            Expr::HandlerCoalesce {
                scrutinee,
                success,
                fallback,
                ..
            } => {
                self.try_body_uses_dedicated_throws_call(scrutinee, context)
                    || self.try_body_uses_dedicated_throws_call(success, context)
                    || self.try_body_uses_dedicated_throws_call(fallback, context)
            }
            Expr::HandlerChainCall(chain) => {
                self.try_body_uses_dedicated_throws_call(&chain.scrutinee, context)
                    || chain.groups.iter().flatten().any(|argument| {
                        self.try_body_uses_dedicated_throws_call(&argument.value, context)
                    })
                    || self.try_body_uses_dedicated_throws_call(&chain.success, context)
                    || self.try_body_uses_dedicated_throws_call(&chain.residual, context)
            }
            Expr::Member(base, _) | Expr::ChainMember(base, _) => {
                self.try_body_uses_dedicated_throws_call(base, context)
            }
            Expr::Array(elements) => elements
                .iter()
                .any(|element| self.try_body_uses_dedicated_throws_call(element, context)),
            Expr::StructLiteral { fields, .. } => fields
                .iter()
                .any(|field| self.try_body_uses_dedicated_throws_call(&field.value, context)),
            Expr::Index { base, index } => {
                self.try_body_uses_dedicated_throws_call(base, context)
                    || self.try_body_uses_dedicated_throws_call(index, context)
            }
            Expr::Block(statements, tail) => {
                statements.iter().any(|statement| match statement {
                    Stmt::Let(binding) => {
                        self.try_body_uses_dedicated_throws_call(&binding.value, context)
                    }
                    Stmt::Expr(expression) => {
                        self.try_body_uses_dedicated_throws_call(expression, context)
                    }
                }) || tail
                    .as_ref()
                    .is_some_and(|tail| self.try_body_uses_dedicated_throws_call(tail, context))
            }
            Expr::If {
                condition,
                then_branch,
                else_branch,
            } => {
                self.try_body_uses_dedicated_throws_call(condition, context)
                    || self.try_body_uses_dedicated_throws_call(then_branch, context)
                    || else_branch.as_ref().is_some_and(|else_branch| {
                        self.try_body_uses_dedicated_throws_call(else_branch, context)
                    })
            }
            Expr::While { condition, body } => {
                self.try_body_uses_dedicated_throws_call(condition, context)
                    || self.try_body_uses_dedicated_throws_call(body, context)
            }
            Expr::Loop { body } => self.try_body_uses_dedicated_throws_call(body, context),
            Expr::Match { scrutinee, arms } => {
                self.try_body_uses_dedicated_throws_call(scrutinee, context)
                    || arms.iter().any(|arm| {
                        arm.guard.as_ref().is_some_and(|guard| {
                            self.try_body_uses_dedicated_throws_call(guard, context)
                        }) || self.try_body_uses_dedicated_throws_call(&arm.body, context)
                    })
            }
            Expr::Type(_) | Expr::Unit | Expr::Integer(_) | Expr::Bool(_) | Expr::Name(_) => false,
        }
    }

    fn collect_escaping_throws(
        &self,
        expression: &Expr,
        context: &LowerCtx,
        errors: &mut HashSet<Ty>,
    ) {
        match expression {
            Expr::Type(_)
            | Expr::Unit
            | Expr::Integer(_)
            | Expr::Bool(_)
            | Expr::Name(_)
            | Expr::Closure(_, _) => {}
            Expr::Try(_) => {}
            Expr::Throw(value) => {
                match self.probe_expr_ty(value, None, context) {
                    TypeProbe::Known(ty)
                    | TypeProbe::KnownSource(ty, _)
                    | TypeProbe::Defaultable(ty) => {
                        errors.insert(ty);
                    }
                    TypeProbe::Unsupported => {}
                }
                self.collect_escaping_throws(value, context, errors);
            }
            Expr::Unary(_, value)
            | Expr::Borrow { value, .. }
            | Expr::DoBlock { body: value }
            | Expr::Unsafe(value)
            | Expr::Return(Some(value))
            | Expr::Break(Some(value)) => self.collect_escaping_throws(value, context, errors),
            Expr::Return(None) | Expr::Break(None) | Expr::Continue => {}
            Expr::Binary(left, _, right)
            | Expr::Coalesce(left, right)
            | Expr::Assign(left, right)
            | Expr::CompoundAssign(left, _, right) => {
                self.collect_escaping_throws(left, context, errors);
                self.collect_escaping_throws(right, context, errors);
            }
            Expr::HandlerCoalesce {
                scrutinee,
                success,
                fallback,
                ..
            } => {
                self.collect_escaping_throws(scrutinee, context, errors);
                self.collect_escaping_throws(success, context, errors);
                self.collect_escaping_throws(fallback, context, errors);
            }
            Expr::HandlerChainCall(chain) => {
                self.collect_escaping_throws(&chain.scrutinee, context, errors);
                for argument in chain.groups.iter().flatten() {
                    self.collect_escaping_throws(&argument.value, context, errors);
                }
                self.collect_escaping_throws(&chain.success, context, errors);
                self.collect_escaping_throws(&chain.residual, context, errors);
            }
            Expr::Call(_, _) => {
                if let Some((_, error)) = self.call_throws_info(expression, context) {
                    errors.insert(error);
                }
                self.collect_standard_throws_errors_from_call(expression, context, errors);
                self.collect_standard_throws_errors_from_effect_handler_call(
                    expression, context, errors,
                );
                let mut groups = Vec::new();
                let root = flatten_call(expression, &mut groups);
                match root {
                    Expr::Member(base, _) | Expr::ChainMember(base, _) => {
                        self.collect_escaping_throws(base, context, errors)
                    }
                    Expr::Name(_) => {}
                    root => self.collect_escaping_throws(root, context, errors),
                }
                for argument in groups.iter().flat_map(|group| group.iter()) {
                    self.collect_escaping_throws(&argument.value, context, errors);
                }
            }
            Expr::Member(base, _) | Expr::ChainMember(base, _) => {
                self.collect_escaping_throws(base, context, errors)
            }
            Expr::Array(elements) => {
                for element in elements {
                    self.collect_escaping_throws(element, context, errors);
                }
            }
            Expr::StructLiteral { fields, .. } => {
                for field in fields {
                    self.collect_escaping_throws(&field.value, context, errors);
                }
            }
            Expr::Index { base, index } => {
                self.collect_escaping_throws(base, context, errors);
                self.collect_escaping_throws(index, context, errors);
            }
            Expr::Block(statements, tail) => {
                let mut block_context = context.clone();
                block_context.push_scope();
                for statement in statements {
                    let value = match statement {
                        Stmt::Let(binding) => &binding.value,
                        Stmt::Expr(value) => value,
                    };
                    self.collect_escaping_throws(value, &block_context, errors);
                    let Stmt::Let(binding) = statement else {
                        continue;
                    };
                    let annotation = binding
                        .annotation
                        .as_ref()
                        .and_then(|source| self.probe_source_ty(source));
                    let inferred = match self.probe_expr_ty(
                        &binding.value,
                        annotation.as_ref(),
                        &block_context,
                    ) {
                        TypeProbe::Known(ty)
                        | TypeProbe::KnownSource(ty, _)
                        | TypeProbe::Defaultable(ty) => Some(ty),
                        TypeProbe::Unsupported => None,
                    };
                    if let Some(ty) = annotation.or(inferred) {
                        let id = block_context.fresh_local();
                        block_context.insert_local(
                            binding.name.clone(),
                            LocalInfo {
                                id,
                                ty,
                                mutable: binding.mutable,
                                capability: LocalCapability::Owned,
                                alias: None,
                                partial: None,
                                closure: None,
                            },
                        );
                    }
                }
                if let Some(tail) = tail {
                    self.collect_escaping_throws(tail, &block_context, errors);
                }
            }
            Expr::If {
                condition,
                then_branch,
                else_branch,
            } => {
                self.collect_escaping_throws(condition, context, errors);
                self.collect_escaping_throws(then_branch, context, errors);
                if let Some(else_branch) = else_branch {
                    self.collect_escaping_throws(else_branch, context, errors);
                }
            }
            Expr::While { condition, body } => {
                self.collect_escaping_throws(condition, context, errors);
                self.collect_escaping_throws(body, context, errors);
            }
            Expr::Loop { body } => self.collect_escaping_throws(body, context, errors),
            Expr::Match { scrutinee, arms } => {
                self.collect_escaping_throws(scrutinee, context, errors);
                for arm in arms {
                    if let Some(guard) = &arm.guard {
                        self.collect_escaping_throws(guard, context, errors);
                    }
                    self.collect_escaping_throws(&arm.body, context, errors);
                }
            }
        }
    }

    fn collect_standard_throws_errors_from_call(
        &self,
        expression: &Expr,
        context: &LowerCtx,
        errors: &mut HashSet<Ty>,
    ) {
        let throws_name = self.lang_item_name(LangItemKind::ThrowsEffect);
        if let Some(sources) = self.call_custom_effect_sources(expression, context) {
            for source in sources
                .iter()
                .filter_map(|effect| standard_throws_error_source(effect, throws_name))
            {
                if let Some(error) = self.probe_source_ty(&source) {
                    errors.insert(error);
                }
            }
        }
    }

    fn collect_standard_throws_errors_from_effect_handler_call(
        &self,
        expression: &Expr,
        context: &LowerCtx,
        errors: &mut HashSet<Ty>,
    ) {
        let Expr::Call(inner_callee, action_arguments) = expression else {
            return;
        };
        let [CallArg {
            label: None,
            value: Expr::Closure(action_parameters, action_body),
        }] = action_arguments.as_slice()
        else {
            return;
        };
        if !action_parameters.is_empty() {
            return;
        }
        let mut groups = Vec::new();
        let Expr::Member(effect, member) = flatten_call(inner_callee, &mut groups) else {
            return;
        };
        if member != "handle" || groups.len() != 1 {
            return;
        }
        let Some(effect_name) = source_type_expression_name(effect) else {
            return;
        };
        let root_name = effect_name.split('(').next().unwrap_or(&effect_name);
        if !self.effect_defs.contains_key(root_name) {
            return;
        }
        for argument in groups[0] {
            if let Expr::Closure(_, body) = &argument.value {
                self.collect_escaping_throws(body, context, errors);
            }
        }
        self.collect_escaping_throws(action_body, context, errors);
    }
}

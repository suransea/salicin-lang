use std::collections::HashSet;

use crate::ast::{CallArg, Expr, Stmt};
use crate::core::LangItemKind;

use super::effects::standard_throws_error_source;
use super::flow::{LocalInfo, LowerCtx};
use super::hir::{FunctionSig, FunctionTy, LocalCapability, Ty};
use super::lower::{flatten_call, TypeProbe};
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

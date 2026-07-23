use std::cell::{Cell, RefCell};
use std::collections::HashMap;
use std::rc::Rc;

use crate::ast::{
    Binding, CallArg, CompileParamKind, EffectDef, Expr, Function, FunctionEffects, Stmt, Type,
};
use crate::core::LangItemKind;

use super::compile_time::{
    source_effect_identities, source_effect_identity, source_effect_source_map,
};
use super::flow::LowerCtx;
use super::handlers::{
    contextual_annotation, AlgebraicHandler, AlgebraicHandlerClause, AlgebraicHandlerOperation,
    SourceContinuation, SourceErasedCallable,
};
use super::hir::{ClosureCaptureMode, ClosureEffectContext, HirArgument, HirExpr, HirExprKind, Ty};
use super::lower::{error_expr, flatten_call, TypeProbe};
use super::source_rewrite::{source_effect_expression_identity, substitute_function_types};
use super::Analyzer;

impl Analyzer {
    pub(super) fn is_standard_unsafe_effect_source(&self, effect: &Type) -> bool {
        matches!(
            effect,
            Type::Named(name, arguments)
                if name == self.lang_item_name(LangItemKind::UnsafeEffect)
                    && arguments.is_empty()
        )
    }

    pub(super) fn source_effects_include_standard_unsafe(&self, effects: &[Type]) -> bool {
        effects
            .iter()
            .any(|effect| self.is_standard_unsafe_effect_source(effect))
    }

    pub(super) fn custom_effect_sources_without_standard_unsafe(
        &self,
        effects: &[Type],
    ) -> Vec<Type> {
        effects
            .iter()
            .filter(|effect| !self.is_standard_unsafe_effect_source(effect))
            .cloned()
            .collect()
    }

    pub(super) fn custom_effect_identities_without_standard_unsafe(
        &self,
        effects: &[Type],
    ) -> Vec<String> {
        source_effect_identities(&self.custom_effect_sources_without_standard_unsafe(effects))
    }

    pub(super) fn custom_effect_source_map_without_standard_unsafe(
        &self,
        effects: &[Type],
    ) -> HashMap<String, Type> {
        source_effect_source_map(&self.custom_effect_sources_without_standard_unsafe(effects))
    }

    pub(super) fn function_effects_unsafe(&self, effects: &FunctionEffects) -> bool {
        effects.unsafe_effect || self.source_effects_include_standard_unsafe(&effects.custom)
    }

    pub(super) fn strip_authorized_unsafe_effects(&self, effects: &mut FunctionEffects) {
        effects.unsafe_effect = false;
        effects
            .custom
            .retain(|effect| !self.is_standard_unsafe_effect_source(effect));
    }

    pub(super) fn effect_abi_result_source(
        &self,
        logical: Type,
        effects: &FunctionEffects,
    ) -> Type {
        match effects.throws.as_deref() {
            Some(error) => Type::Named(
                self.lang_item_name(LangItemKind::Result).to_owned(),
                vec![error.clone(), logical],
            ),
            None => logical,
        }
    }

    pub(super) fn lower_do_block(
        &mut self,
        body: &Expr,
        expected: Option<&Ty>,
        context: &mut LowerCtx,
    ) -> HirExpr {
        if !do_block_requires_function_boundary(body) {
            return self.lower_expr(body, expected, context);
        }
        let active_throws_error = context.active_throws_error.clone();
        let logical_result =
            expected
                .filter(|ty| **ty != Ty::Error)
                .cloned()
                .or_else(|| match self.probe_expr_ty(body, None, context) {
                    TypeProbe::Known(ty)
                    | TypeProbe::KnownSource(ty, _)
                    | TypeProbe::Defaultable(ty) => Some(ty),
                    TypeProbe::Unsupported => None,
                });
        let declared_result = match (&logical_result, &active_throws_error) {
            (Some(logical), Some(error)) => {
                self.ensure_throws_result_type(logical.clone(), error.clone())
            }
            (Some(logical), None) => Some(logical.clone()),
            (None, Some(_)) => {
                self.error(
                    "cannot infer the result of an effect-forwarding `do` block; add a contextual type",
                );
                Some(Ty::Error)
            }
            (None, None) => None,
        };
        let closure = self.lower_local_closure(
            &[],
            body,
            declared_result,
            ClosureEffectContext {
                unsafe_depth: context.unsafe_depth,
                throws_error: active_throws_error.clone(),
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
        let result = info.result.clone();
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
        let call = HirExpr {
            ty: result,
            kind: HirExprKind::Call {
                function: info.function,
                arguments,
                consumed_callable: None,
                diverges: false,
            },
        };
        if let Some(error) = active_throws_error.as_ref() {
            self.lower_automatic_throws(call, error, expected.or(logical_result.as_ref()), context)
        } else {
            call
        }
    }

    pub(super) fn resolve_effect_application(
        &mut self,
        expression: &Expr,
        context: &LowerCtx,
    ) -> Result<Option<(EffectDef, Type)>, ()> {
        let (name, arguments) = match expression {
            Expr::Name(name)
                if !context.shadows_top_level_name(name) && self.effect_defs.contains_key(name) =>
            {
                (name.clone(), Vec::new())
            }
            Expr::Call(callee, arguments) => {
                let Expr::Name(name) = callee.as_ref() else {
                    return Ok(None);
                };
                if context.shadows_top_level_name(name) || !self.effect_defs.contains_key(name) {
                    return Ok(None);
                }
                if arguments.iter().any(|argument| argument.label.is_some()) {
                    self.error(format!(
                        "effect application `{name}` currently requires positional type arguments"
                    ));
                    return Err(());
                }
                let mut sources = Vec::new();
                for argument in arguments {
                    let Some(source) =
                        self.type_argument_from_expr(&argument.value, &context.type_substitutions)
                    else {
                        return Err(());
                    };
                    sources.push(source);
                }
                (name.clone(), sources)
            }
            _ => return Ok(None),
        };
        let definition = self.effect_defs[&name].clone();
        let parameters = definition
            .compile_groups
            .iter()
            .flatten()
            .collect::<Vec<_>>();
        if arguments.len() != parameters.len() {
            self.error(format!(
                "effect argument count mismatch for `{name}`: expected {}, found {}",
                parameters.len(),
                arguments.len()
            ));
            return Err(());
        }
        Ok(Some((definition, Type::Named(name, arguments))))
    }

    fn probe_handler_action_logical_ty(
        &mut self,
        expression: &Expr,
        context: &LowerCtx,
    ) -> Option<Ty> {
        if let Some((payload, _)) = self.call_throws_info(expression, context) {
            return Some(payload);
        }
        match expression {
            Expr::Block(_, Some(tail)) | Expr::Unsafe(tail) | Expr::DoBlock { body: tail } => {
                self.probe_handler_action_logical_ty(tail, context)
            }
            _ => match self.probe_expr_ty(expression, None, context) {
                TypeProbe::Known(ty)
                | TypeProbe::KnownSource(ty, _)
                | TypeProbe::Defaultable(ty) => Some(ty),
                TypeProbe::Unsupported => None,
            },
        }
    }

    pub(super) fn lower_effect_handler(
        &mut self,
        definition: &EffectDef,
        instance: &Type,
        groups: &[&[CallArg]],
        expected: Option<&Ty>,
        context: &mut LowerCtx,
    ) -> HirExpr {
        let diagnostic_count = self.diagnostics.len();
        let handle_protocol = self.lang_item_name(LangItemKind::Handle).to_owned();
        if !self.traits.get(&handle_protocol).is_some_and(|schema| {
            schema.valid && schema.self_parameter.kind == CompileParamKind::Effect
        }) {
            self.error(
                "effect handler lowering requires the validated `core.control.Handle` protocol",
            );
            return error_expr();
        }
        if groups.len() != 2 || groups[1].len() != 1 || groups[1][0].label.is_some() {
            self.error(format!(
                "`{}.handle` expects one labeled clause group followed by one trailing action closure",
                source_effect_identity(instance)
            ));
            return error_expr();
        }
        let Expr::Closure(action_parameters, action_body) = &groups[1][0].value else {
            self.error("an effect handler requires a trailing closure");
            return error_expr();
        };
        if !action_parameters.is_empty() {
            self.error("an effect handler action closure cannot take parameters");
            return error_expr();
        }

        let Type::Named(_, arguments) = instance else {
            unreachable!("resolved effect instances are nominal applications")
        };
        let substitutions = definition
            .compile_groups
            .iter()
            .flatten()
            .zip(arguments)
            .map(|(parameter, argument)| (parameter.name.clone(), argument.clone()))
            .collect::<HashMap<_, _>>();
        let mut operations = definition.operations.clone();
        for operation in &mut operations {
            substitute_function_types(operation, &substitutions);
        }

        let mut clauses = HashMap::new();
        let mut handler_operations: HashMap<String, Vec<AlgebraicHandlerOperation>> =
            HashMap::new();
        for operation in &operations {
            let mut residual_effects = operation.effects.clone();
            residual_effects.custom.retain(|effect| {
                source_effect_identity(effect) != source_effect_identity(instance)
            });
            handler_operations
                .entry(operation.name.clone())
                .or_default()
                .push(AlgebraicHandlerOperation {
                    key: effect_operation_key(operation),
                    labels: effect_operation_labels(operation),
                    residual_effects,
                });
        }
        let mut done = None;
        for argument in groups[0] {
            let Some(label) = &argument.label else {
                self.error("effect handler clauses must use operation names as argument labels");
                continue;
            };
            let Expr::Closure(parameters, body) = &argument.value else {
                self.error(format!("handler clause `{label}` must be a closure"));
                continue;
            };
            if label == "done" {
                if done.is_some() {
                    self.error("duplicate handler clause `done`");
                    continue;
                }
                if parameters.len() != 1 {
                    self.error("handler clause `done` expects one result parameter");
                    continue;
                }
                done = Some(AlgebraicHandlerClause {
                    parameters: parameters.clone(),
                    resume: None,
                    body: (**body).clone(),
                    resume_input: None,
                });
                continue;
            }
            let candidates = operations
                .iter()
                .filter(|operation| operation.name == *label)
                .collect::<Vec<_>>();
            if candidates.is_empty() {
                self.error(format!(
                    "unknown handler clause `{label}` for effect `{}`",
                    source_effect_identity(instance)
                ));
                continue;
            }
            let operation = if candidates.len() == 1 {
                candidates[0]
            } else {
                let matching = candidates
                    .iter()
                    .copied()
                    .filter(|operation| {
                        let label_count = parameters.len().saturating_sub(usize::from(
                            operation_resume_input_source(operation).is_some(),
                        ));
                        let clause_labels = parameters
                            .iter()
                            .take(label_count)
                            .map(|parameter| parameter.name.as_str())
                            .collect::<Vec<_>>();
                        effect_operation_labels(operation)
                            .iter()
                            .map(String::as_str)
                            .eq(clause_labels.iter().copied())
                    })
                    .collect::<Vec<_>>();
                if matching.len() != 1 {
                    self.error(format!(
                        "overloaded handler clause `{label}` must name the operation parameters in declaration order before `resume`"
                    ));
                    continue;
                }
                matching[0]
            };
            let operation_key = effect_operation_key(operation);
            if clauses.contains_key(&operation_key) {
                self.error(format!("duplicate handler clause `{operation_key}`"));
                continue;
            }
            let operation_parameters = operation.groups.iter().flatten().collect::<Vec<_>>();
            let resume_input = operation_resume_input_source(operation);
            let expected_parameter_count =
                operation_parameters.len() + usize::from(resume_input.is_some());
            if parameters.len() != expected_parameter_count {
                if resume_input.is_some() {
                    self.error(format!(
                        "handler clause `{label}` expects {} operation parameter(s) followed by `resume`, found {} parameter(s)",
                        operation_parameters.len(),
                        parameters.len()
                    ));
                } else {
                    self.error(format!(
                        "handler clause `{label}` handles a `Never`-returning operation and expects {} operation parameter(s) without `resume`, found {} parameter(s)",
                        operation_parameters.len(),
                        parameters.len()
                    ));
                }
                continue;
            }
            let mut parameters = parameters.clone();
            for (parameter, declared) in parameters.iter_mut().zip(operation_parameters.iter()) {
                if parameter.ty == Type::Named("$context$infer".into(), Vec::new()) {
                    parameter.ty = declared.ty.clone();
                }
            }
            let resume = resume_input
                .is_some()
                .then(|| parameters.pop().expect("validated resume parameter").name);
            clauses.insert(
                operation_key,
                AlgebraicHandlerClause {
                    parameters,
                    resume,
                    body: (**body).clone(),
                    resume_input,
                },
            );
        }
        for operation in &operations {
            let key = effect_operation_key(operation);
            if !clauses.contains_key(&key) {
                let display = if handler_operations
                    .get(&operation.name)
                    .is_some_and(|candidates| candidates.len() > 1)
                {
                    key
                } else {
                    operation.name.clone()
                };
                self.error(format!(
                    "missing handler clause `{display}` for effect `{}`",
                    source_effect_identity(instance)
                ));
            }
        }
        if self.diagnostics.len() != diagnostic_count {
            return error_expr();
        }

        let inferred_from_action = if done.is_none() {
            self.probe_handler_action_logical_ty(action_body, context)
                .filter(|ty| !self.is_uninhabited_type(ty))
        } else {
            None
        };
        let inferred_handler_result = inferred_from_action
            .or_else(|| {
                done.is_none()
                    .then(|| {
                        handled_action_result_source(
                            action_body,
                            &source_effect_identity(instance),
                            &operations,
                        )
                        .map(|source| self.lower_source_type(&source))
                    })
                    .flatten()
            })
            .or_else(|| expected.cloned())
            .or_else(|| {
                let bodies = clauses
                    .values()
                    .map(|clause| clause.body.clone())
                    .collect::<Vec<_>>();
                bodies
                    .into_iter()
                    .find_map(|body| match self.probe_expr_ty(&body, None, context) {
                        TypeProbe::Known(ty)
                        | TypeProbe::KnownSource(ty, _)
                        | TypeProbe::Defaultable(ty) => Some(ty),
                        TypeProbe::Unsupported => None,
                    })
            });
        let erased_callables = context
            .function_name
            .as_ref()
            .into_iter()
            .flat_map(|function_name| {
                self.runtime_handler_actions
                    .iter()
                    .filter(move |((candidate, _, _), action)| {
                        candidate == function_name
                            && action.effect == source_effect_identity(instance)
                    })
                    .filter_map(|((_, group_index, parameter_index), action)| {
                        let function = self.functions.get(function_name)?;
                        let parameter = function.groups.get(*group_index)?.get(*parameter_index)?;
                        Some((
                            parameter.name.clone(),
                            SourceErasedCallable {
                                output: self.source_type_for_ty(&action.output)?,
                                answer: self.source_type_for_ty(&action.answer)?,
                                accepts_input: action.accepts_input,
                            },
                        ))
                    })
            })
            .collect::<HashMap<_, _>>();
        let handler = Rc::new(AlgebraicHandler {
            identity: source_effect_identity(instance),
            source: instance.clone(),
            clauses,
            operations: handler_operations,
            lexical_unsafe_depth: Rc::new(Cell::new(context.unsafe_depth)),
            function_aliases: Rc::new(RefCell::new(HashMap::new())),
            resumable_closures: Rc::new(RefCell::new(HashMap::new())),
            dynamic_callables: Rc::new(RefCell::new(HashMap::new())),
            erased_callables,
            done,
            inlining: Rc::new(RefCell::new(HashMap::new())),
            loop_breaks: Rc::new(RefCell::new(HashMap::new())),
            result_source: inferred_handler_result
                .as_ref()
                .and_then(|ty| self.source_type_for_ty(ty)),
            return_continuations: Rc::new(RefCell::new(HashMap::new())),
        });
        let final_continuation: SourceContinuation = if let Some(done) = handler.done.clone() {
            let handler = handler.clone();
            Rc::new(move |analyzer, value| {
                let binding = Binding {
                    mutable: false,
                    name: done.parameters[0].name.clone(),
                    annotation: contextual_annotation(&done.parameters[0]),
                    value,
                };
                let identity: SourceContinuation = Rc::new(|_, value| Ok(value));
                let body = analyzer.transform_handler_expr(
                    done.body.clone(),
                    handler.clone(),
                    None,
                    identity,
                )?;
                Ok(Expr::Block(vec![Stmt::Let(binding)], Some(Box::new(body))))
            })
        } else {
            Rc::new(|_, value| Ok(value))
        };
        let handled_identity = handler.identity.clone();
        let transformed = match self.transform_handler_expr(
            (**action_body).clone(),
            handler,
            None,
            final_continuation,
        ) {
            Ok(expression) => expression,
            Err(()) => return error_expr(),
        };
        let newly_active = context
            .active_custom_effects
            .insert(handled_identity.clone());
        let previous_active_source = context
            .active_custom_effect_sources
            .insert(handled_identity.clone(), instance.clone());
        let newly_lexical = context
            .lexical_handler_effects
            .insert(handled_identity.clone());
        let previous_lexical_source = context
            .lexical_handler_effect_sources
            .insert(handled_identity.clone(), instance.clone());
        let lowered = self.lower_expr(&transformed, expected, context);
        if newly_active {
            context.active_custom_effects.remove(&handled_identity);
        }
        match previous_active_source {
            Some(source) => {
                context
                    .active_custom_effect_sources
                    .insert(handled_identity.clone(), source);
            }
            None => {
                context
                    .active_custom_effect_sources
                    .remove(&handled_identity);
            }
        }
        if newly_lexical {
            context.lexical_handler_effects.remove(&handled_identity);
        }
        match previous_lexical_source {
            Some(source) => {
                context
                    .lexical_handler_effect_sources
                    .insert(handled_identity, source);
            }
            None => {
                context
                    .lexical_handler_effect_sources
                    .remove(&handled_identity);
            }
        }
        lowered
    }

    pub(super) fn function_effects_custom_identities(
        &self,
        effects: &FunctionEffects,
    ) -> Vec<String> {
        self.custom_effect_identities_without_standard_unsafe(&effects.custom)
    }

    pub(super) fn function_effects_custom_source_map(
        &self,
        effects: &FunctionEffects,
    ) -> HashMap<String, Type> {
        self.custom_effect_source_map_without_standard_unsafe(&effects.custom)
    }
}

pub(super) fn do_block_requires_function_boundary(expression: &Expr) -> bool {
    match expression {
        Expr::Return(_) | Expr::Try(_) | Expr::Throw(_) => true,
        Expr::Closure(_, _) | Expr::DoBlock { .. } => false,
        Expr::Unary(_, value)
        | Expr::Unsafe(value)
        | Expr::Borrow { value, .. }
        | Expr::Member(value, _)
        | Expr::ChainMember(value, _)
        | Expr::Loop { body: value } => do_block_requires_function_boundary(value),
        Expr::Binary(left, _, right)
        | Expr::Coalesce(left, right)
        | Expr::Assign(left, right)
        | Expr::CompoundAssign(left, _, right) => {
            do_block_requires_function_boundary(left) || do_block_requires_function_boundary(right)
        }
        Expr::HandlerCoalesce {
            scrutinee,
            success,
            fallback,
            ..
        } => {
            do_block_requires_function_boundary(scrutinee)
                || do_block_requires_function_boundary(success)
                || do_block_requires_function_boundary(fallback)
        }
        Expr::HandlerChainCall(chain) => {
            do_block_requires_function_boundary(&chain.scrutinee)
                || chain
                    .groups
                    .iter()
                    .flatten()
                    .any(|argument| do_block_requires_function_boundary(&argument.value))
                || do_block_requires_function_boundary(&chain.success)
                || do_block_requires_function_boundary(&chain.residual)
        }
        Expr::Call(callee, arguments) => {
            do_block_requires_function_boundary(callee)
                || arguments
                    .iter()
                    .any(|argument| do_block_requires_function_boundary(&argument.value))
        }
        Expr::StructLiteral { fields, .. } => fields
            .iter()
            .any(|field| do_block_requires_function_boundary(&field.value)),
        Expr::Array(elements) => elements.iter().any(do_block_requires_function_boundary),
        Expr::Index { base, index } => {
            do_block_requires_function_boundary(base) || do_block_requires_function_boundary(index)
        }
        Expr::Block(statements, tail) => {
            statements.iter().any(|statement| match statement {
                Stmt::Let(binding) => do_block_requires_function_boundary(&binding.value),
                Stmt::Expr(expression) => do_block_requires_function_boundary(expression),
            }) || tail
                .as_deref()
                .is_some_and(do_block_requires_function_boundary)
        }
        Expr::If {
            condition,
            then_branch,
            else_branch,
        } => {
            do_block_requires_function_boundary(condition)
                || do_block_requires_function_boundary(then_branch)
                || else_branch
                    .as_deref()
                    .is_some_and(do_block_requires_function_boundary)
        }
        Expr::While { condition, body } => {
            do_block_requires_function_boundary(condition)
                || do_block_requires_function_boundary(body)
        }
        Expr::Break(_) | Expr::Continue => true,
        Expr::Match { scrutinee, arms } => {
            do_block_requires_function_boundary(scrutinee)
                || arms.iter().any(|arm| {
                    arm.guard
                        .as_ref()
                        .is_some_and(do_block_requires_function_boundary)
                        || do_block_requires_function_boundary(&arm.body)
                })
        }
        Expr::Type(_) | Expr::Unit | Expr::Integer(_) | Expr::Bool(_) | Expr::Name(_) => false,
    }
}

pub(super) fn handled_operation_call(
    expression: &Expr,
    identity: &str,
) -> Option<(String, Vec<CallArg>)> {
    let mut groups = Vec::new();
    let root = flatten_call(expression, &mut groups);
    let Expr::Member(effect, operation) = root else {
        return None;
    };
    if source_effect_expression_identity(effect)? != identity || operation == "handle" {
        return None;
    }
    Some((
        operation.clone(),
        groups
            .into_iter()
            .flat_map(|group| group.iter().cloned())
            .collect(),
    ))
}

pub(super) fn effect_operation_labels(operation: &Function) -> Vec<String> {
    operation
        .groups
        .iter()
        .flatten()
        .map(|parameter| parameter.name.clone())
        .collect()
}

pub(super) fn effect_operation_key(operation: &Function) -> String {
    format!(
        "{}({})",
        operation.name,
        effect_operation_labels(operation).join(",")
    )
}

pub(super) fn call_argument_labels(arguments: &[CallArg]) -> Option<Vec<String>> {
    arguments
        .iter()
        .map(|argument| argument.label.clone())
        .collect()
}

pub(super) fn logical_effect_result_source(result: &Type, effects: &FunctionEffects) -> Type {
    let Some(error) = effects.throws.as_deref() else {
        return result.clone();
    };
    match result {
        Type::Named(_, arguments) if arguments.len() == 2 && arguments[1] == *error => {
            arguments[0].clone()
        }
        _ => result.clone(),
    }
}

pub(super) fn logical_function_result_source(function: &Function) -> Option<Type> {
    function
        .return_type
        .as_ref()
        .map(|result| logical_effect_result_source(result, &function.effects))
}

pub(super) fn source_type_is_never(source: &Type) -> bool {
    matches!(
        source,
        Type::Named(name, arguments)
            if arguments.is_empty() && name.rsplit("::").next() == Some("Never")
    )
}

pub(super) fn operation_resume_input_source(function: &Function) -> Option<Type> {
    logical_function_result_source(function).filter(|source| !source_type_is_never(source))
}

pub(super) fn standard_throws_error_source(effect: &Type, throws_name: &str) -> Option<Type> {
    match effect {
        Type::Named(name, arguments) if name == throws_name && arguments.len() == 1 => {
            Some(arguments[0].clone())
        }
        _ => None,
    }
}

pub(super) fn handled_action_result_source(
    expression: &Expr,
    identity: &str,
    operations: &[Function],
) -> Option<Type> {
    if let Some((operation, arguments)) = handled_operation_call(expression, identity) {
        let candidates = operations
            .iter()
            .filter(|candidate| candidate.name == operation)
            .collect::<Vec<_>>();
        let selected = if candidates.len() == 1 {
            candidates.first().copied()
        } else {
            let labels = call_argument_labels(&arguments)?;
            candidates
                .into_iter()
                .find(|candidate| effect_operation_labels(candidate) == labels)
        }?;
        return operation_resume_input_source(selected);
    }
    match expression {
        Expr::Block(_, Some(tail)) => handled_action_result_source(tail, identity, operations),
        Expr::If {
            then_branch,
            else_branch: Some(else_branch),
            ..
        } => {
            let then_type = handled_action_result_source(then_branch, identity, operations)?;
            let else_type = handled_action_result_source(else_branch, identity, operations)?;
            (then_type == else_type).then_some(then_type)
        }
        _ => None,
    }
}

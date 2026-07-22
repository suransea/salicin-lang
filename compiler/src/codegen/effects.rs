use std::cell::{Cell, RefCell};
use std::collections::{HashMap, HashSet};
use std::rc::Rc;

use crate::ast::{BinaryOp, Binding, CallArg, Expr, Function, FunctionEffects, Param, Stmt, Type};
use crate::core::LangItemKind;

use super::compile_time::{source_effect_identities, source_effect_source_map};
use super::source_rewrite::{source_effect_expression_identity, source_type_expression_name};
use super::{flatten_call, Analyzer};

pub(super) type SourceContinuation = Rc<dyn Fn(&mut Analyzer, Expr) -> Result<Expr, ()>>;
pub(super) type SourceArgumentsContinuation =
    Rc<dyn Fn(&mut Analyzer, Vec<CallArg>) -> Result<Expr, ()>>;

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

#[derive(Clone)]
pub(super) struct AlgebraicHandlerClause {
    pub(super) parameters: Vec<Param>,
    pub(super) resume: Option<String>,
    pub(super) body: Expr,
    pub(super) resume_input: Option<Type>,
}

#[derive(Clone)]
pub(super) struct AlgebraicHandler {
    pub(super) identity: String,
    pub(super) source: Type,
    pub(super) clauses: HashMap<String, AlgebraicHandlerClause>,
    pub(super) operations: HashMap<String, Vec<AlgebraicHandlerOperation>>,
    pub(super) lexical_unsafe_depth: Rc<Cell<usize>>,
    pub(super) function_aliases: Rc<RefCell<HashMap<String, String>>>,
    pub(super) resumable_closures: Rc<RefCell<HashMap<String, SourceResumableClosure>>>,
    pub(super) dynamic_callables: Rc<RefCell<HashMap<String, SourceDynamicCallable>>>,
    pub(super) erased_callables: HashMap<String, SourceErasedCallable>,
    pub(super) done: Option<AlgebraicHandlerClause>,
    pub(super) inlining: Rc<RefCell<HashMap<String, SourceInlineFrame>>>,
    pub(super) loop_breaks: Rc<RefCell<HashMap<String, SourceContinuation>>>,
    pub(super) result_source: Option<Type>,
    pub(super) return_continuations: Rc<RefCell<HashMap<String, SourceContinuation>>>,
}

#[derive(Clone)]
pub(super) struct SourceErasedCallable {
    pub(super) output: Type,
    pub(super) answer: Type,
    pub(super) accepts_input: bool,
}

#[derive(Clone)]
pub(super) struct SourceResumableClosure {
    pub(super) input: Type,
    pub(super) answer: Type,
    pub(super) group_lengths: Vec<usize>,
}

#[derive(Clone, PartialEq, Eq)]
pub(super) struct SourceDynamicCallable {
    pub(super) targets: Vec<String>,
    pub(super) group_lengths: Vec<usize>,
}

#[derive(Clone)]
pub(super) struct AlgebraicHandlerOperation {
    pub(super) key: String,
    pub(super) labels: Vec<String>,
    pub(super) residual_effects: FunctionEffects,
}

#[derive(Clone)]
pub(super) struct SourceInlineFrame {
    pub(super) recursive_name: String,
    pub(super) input: Type,
    pub(super) answer: Type,
}

#[derive(Clone)]
pub(super) struct SourceResume {
    pub(super) name: String,
    pub(super) runtime_name: String,
    pub(super) uses: Rc<Cell<usize>>,
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

pub(super) fn internal_stored_callable_capture(expression: &Expr) -> Option<(&str, usize)> {
    let Expr::Call(callee, arguments) = expression else {
        return None;
    };
    let Expr::Name(name) = callee.as_ref() else {
        return None;
    };
    let index = name
        .strip_prefix("$handler$stored$capture$")?
        .parse::<usize>()
        .ok()?;
    let [CallArg {
        label: None,
        value: Expr::Name(local),
    }] = arguments.as_slice()
    else {
        return None;
    };
    Some((local, index))
}

pub(super) fn contextual_annotation(parameter: &Param) -> Option<Type> {
    (parameter.ty != Type::Named("$context$infer".into(), Vec::new())).then(|| parameter.ty.clone())
}

pub(super) fn resume_call_argument(expression: &Expr, resume: &str) -> Option<Expr> {
    let mut groups = Vec::new();
    let root = flatten_call(expression, &mut groups);
    if !matches!(root, Expr::Name(name) if name == resume)
        || groups.len() != 1
        || groups[0].len() != 1
        || groups[0][0].label.is_some()
    {
        return None;
    }
    Some(groups[0][0].value.clone())
}

pub(super) fn internal_handler_return_argument(expression: &Expr) -> Option<(String, Expr)> {
    let mut groups = Vec::new();
    let root = flatten_call(expression, &mut groups);
    let Expr::Name(name) = root else {
        return None;
    };
    if !name.starts_with("$handler$return$")
        || groups.len() != 1
        || groups[0].len() != 1
        || groups[0][0].label.is_some()
    {
        return None;
    }
    Some((name.clone(), groups[0][0].value.clone()))
}

pub(super) fn internal_handler_loop_break_argument(expression: &Expr) -> Option<(String, Expr)> {
    let mut groups = Vec::new();
    let root = flatten_call(expression, &mut groups);
    let Expr::Name(name) = root else {
        return None;
    };
    if !name.starts_with("$handler$loop$break$")
        || groups.len() != 1
        || groups[0].len() != 1
        || groups[0][0].label.is_some()
    {
        return None;
    }
    Some((name.clone(), groups[0][0].value.clone()))
}

pub(super) fn rewrite_handler_loop_control(
    expression: &mut Expr,
    recursive_name: &str,
    break_name: &str,
    nested_loop_depth: usize,
) {
    if nested_loop_depth == 0 {
        match expression {
            Expr::Break(value) => {
                let value = value.take().map_or(Expr::Unit, |value| *value);
                *expression = Expr::Call(
                    Box::new(Expr::Name(format!("$handler$return${recursive_name}"))),
                    vec![CallArg {
                        label: None,
                        value: Expr::Call(
                            Box::new(Expr::Name(break_name.to_owned())),
                            vec![CallArg { label: None, value }],
                        ),
                    }],
                );
                return;
            }
            Expr::Continue => {
                *expression = Expr::Call(
                    Box::new(Expr::Name(format!("$handler$return${recursive_name}"))),
                    vec![CallArg {
                        label: None,
                        value: Expr::Call(
                            Box::new(Expr::Name(recursive_name.to_owned())),
                            Vec::new(),
                        ),
                    }],
                );
                return;
            }
            _ => {}
        }
    }
    match expression {
        Expr::While { .. } | Expr::Loop { .. } | Expr::Closure(_, _) => {}
        Expr::Unary(_, value)
        | Expr::Try(value)
        | Expr::DoBlock { body: value }
        | Expr::Throw(value)
        | Expr::Unsafe(value)
        | Expr::Borrow { value, .. } => {
            rewrite_handler_loop_control(value, recursive_name, break_name, nested_loop_depth)
        }
        Expr::Binary(left, _, right)
        | Expr::Coalesce(left, right)
        | Expr::Assign(left, right)
        | Expr::CompoundAssign(left, _, right) => {
            rewrite_handler_loop_control(left, recursive_name, break_name, nested_loop_depth);
            rewrite_handler_loop_control(right, recursive_name, break_name, nested_loop_depth);
        }
        Expr::HandlerCoalesce {
            scrutinee,
            success,
            fallback,
            ..
        } => {
            rewrite_handler_loop_control(scrutinee, recursive_name, break_name, nested_loop_depth);
            rewrite_handler_loop_control(success, recursive_name, break_name, nested_loop_depth);
            rewrite_handler_loop_control(fallback, recursive_name, break_name, nested_loop_depth);
        }
        Expr::HandlerChainCall(chain) => {
            rewrite_handler_loop_control(
                &mut chain.scrutinee,
                recursive_name,
                break_name,
                nested_loop_depth,
            );
            for argument in chain.groups.iter_mut().flatten() {
                rewrite_handler_loop_control(
                    &mut argument.value,
                    recursive_name,
                    break_name,
                    nested_loop_depth,
                );
            }
            rewrite_handler_loop_control(
                &mut chain.success,
                recursive_name,
                break_name,
                nested_loop_depth,
            );
            rewrite_handler_loop_control(
                &mut chain.residual,
                recursive_name,
                break_name,
                nested_loop_depth,
            );
        }
        Expr::Call(callee, arguments) => {
            rewrite_handler_loop_control(callee, recursive_name, break_name, nested_loop_depth);
            for argument in arguments {
                rewrite_handler_loop_control(
                    &mut argument.value,
                    recursive_name,
                    break_name,
                    nested_loop_depth,
                );
            }
        }
        Expr::StructLiteral { fields, .. } => {
            for field in fields {
                rewrite_handler_loop_control(
                    &mut field.value,
                    recursive_name,
                    break_name,
                    nested_loop_depth,
                );
            }
        }
        Expr::Member(base, _) | Expr::ChainMember(base, _) => {
            rewrite_handler_loop_control(base, recursive_name, break_name, nested_loop_depth)
        }
        Expr::Array(elements) => {
            for element in elements {
                rewrite_handler_loop_control(
                    element,
                    recursive_name,
                    break_name,
                    nested_loop_depth,
                );
            }
        }
        Expr::Index { base, index } => {
            rewrite_handler_loop_control(base, recursive_name, break_name, nested_loop_depth);
            rewrite_handler_loop_control(index, recursive_name, break_name, nested_loop_depth);
        }
        Expr::Block(statements, tail) => {
            for statement in statements {
                let expression = match statement {
                    Stmt::Let(binding) => &mut binding.value,
                    Stmt::Expr(expression) => expression,
                };
                rewrite_handler_loop_control(
                    expression,
                    recursive_name,
                    break_name,
                    nested_loop_depth,
                );
            }
            if let Some(tail) = tail {
                rewrite_handler_loop_control(tail, recursive_name, break_name, nested_loop_depth);
            }
        }
        Expr::If {
            condition,
            then_branch,
            else_branch,
        } => {
            rewrite_handler_loop_control(condition, recursive_name, break_name, nested_loop_depth);
            rewrite_handler_loop_control(
                then_branch,
                recursive_name,
                break_name,
                nested_loop_depth,
            );
            if let Some(branch) = else_branch {
                rewrite_handler_loop_control(branch, recursive_name, break_name, nested_loop_depth);
            }
        }
        Expr::Return(value) | Expr::Break(value) => {
            if let Some(value) = value {
                rewrite_handler_loop_control(value, recursive_name, break_name, nested_loop_depth);
            }
        }
        Expr::Match { scrutinee, arms } => {
            rewrite_handler_loop_control(scrutinee, recursive_name, break_name, nested_loop_depth);
            for arm in arms {
                if let Some(guard) = &mut arm.guard {
                    rewrite_handler_loop_control(
                        guard,
                        recursive_name,
                        break_name,
                        nested_loop_depth,
                    );
                }
                rewrite_handler_loop_control(
                    &mut arm.body,
                    recursive_name,
                    break_name,
                    nested_loop_depth,
                );
            }
        }
        Expr::Type(_)
        | Expr::Unit
        | Expr::Integer(_)
        | Expr::Bool(_)
        | Expr::Name(_)
        | Expr::Continue => {}
    }
}

pub(super) fn collect_internal_recursion_tokens(expression: &Expr, tokens: &mut HashSet<String>) {
    match expression {
        Expr::Name(name) if name.starts_with("$handler$recursive$") => {
            tokens.insert(name.clone());
        }
        Expr::Unary(_, value)
        | Expr::Try(value)
        | Expr::DoBlock { body: value }
        | Expr::Throw(value)
        | Expr::Unsafe(value) => collect_internal_recursion_tokens(value, tokens),
        Expr::Borrow { value, .. } => collect_internal_recursion_tokens(value, tokens),
        Expr::Binary(left, _, right)
        | Expr::Coalesce(left, right)
        | Expr::Assign(left, right)
        | Expr::CompoundAssign(left, _, right) => {
            collect_internal_recursion_tokens(left, tokens);
            collect_internal_recursion_tokens(right, tokens);
        }
        Expr::HandlerCoalesce {
            scrutinee,
            success,
            fallback,
            ..
        } => {
            collect_internal_recursion_tokens(scrutinee, tokens);
            collect_internal_recursion_tokens(success, tokens);
            collect_internal_recursion_tokens(fallback, tokens);
        }
        Expr::HandlerChainCall(chain) => {
            collect_internal_recursion_tokens(&chain.scrutinee, tokens);
            for argument in chain.groups.iter().flatten() {
                collect_internal_recursion_tokens(&argument.value, tokens);
            }
            collect_internal_recursion_tokens(&chain.success, tokens);
            collect_internal_recursion_tokens(&chain.residual, tokens);
        }
        Expr::Call(callee, arguments) => {
            collect_internal_recursion_tokens(callee, tokens);
            for argument in arguments {
                collect_internal_recursion_tokens(&argument.value, tokens);
            }
        }
        Expr::StructLiteral { fields, .. } => {
            for field in fields {
                collect_internal_recursion_tokens(&field.value, tokens);
            }
        }
        Expr::Member(base, _) | Expr::ChainMember(base, _) => {
            collect_internal_recursion_tokens(base, tokens)
        }
        Expr::Array(elements) => {
            for element in elements {
                collect_internal_recursion_tokens(element, tokens);
            }
        }
        Expr::Index { base, index } => {
            collect_internal_recursion_tokens(base, tokens);
            collect_internal_recursion_tokens(index, tokens);
        }
        Expr::Block(statements, tail) => {
            for statement in statements {
                match statement {
                    Stmt::Let(binding) => collect_internal_recursion_tokens(&binding.value, tokens),
                    Stmt::Expr(expression) => collect_internal_recursion_tokens(expression, tokens),
                }
            }
            if let Some(tail) = tail {
                collect_internal_recursion_tokens(tail, tokens);
            }
        }
        Expr::Closure(_, body) => collect_internal_recursion_tokens(body, tokens),
        Expr::If {
            condition,
            then_branch,
            else_branch,
        } => {
            collect_internal_recursion_tokens(condition, tokens);
            collect_internal_recursion_tokens(then_branch, tokens);
            if let Some(branch) = else_branch {
                collect_internal_recursion_tokens(branch, tokens);
            }
        }
        Expr::Return(value) | Expr::Break(value) => {
            if let Some(value) = value {
                collect_internal_recursion_tokens(value, tokens);
            }
        }
        Expr::While { condition, body } => {
            collect_internal_recursion_tokens(condition, tokens);
            collect_internal_recursion_tokens(body, tokens);
        }
        Expr::Loop { body } => collect_internal_recursion_tokens(body, tokens),
        Expr::Match { scrutinee, arms } => {
            collect_internal_recursion_tokens(scrutinee, tokens);
            for arm in arms {
                if let Some(guard) = &arm.guard {
                    collect_internal_recursion_tokens(guard, tokens);
                }
                collect_internal_recursion_tokens(&arm.body, tokens);
            }
        }
        Expr::Type(_)
        | Expr::Unit
        | Expr::Integer(_)
        | Expr::Bool(_)
        | Expr::Name(_)
        | Expr::Continue => {}
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

pub(super) fn handler_alias_reference(
    expression: &Expr,
    aliases: &HashMap<String, String>,
) -> Option<String> {
    if let Expr::Name(name) = expression {
        return aliases.contains_key(name).then(|| name.clone());
    }
    if let Expr::Closure(parameters, body) = expression {
        let shadowed = parameters
            .iter()
            .map(|parameter| parameter.name.as_str())
            .collect::<HashSet<_>>();
        if aliases.keys().any(|name| shadowed.contains(name.as_str())) {
            let visible = aliases
                .iter()
                .filter(|(name, _)| !shadowed.contains(name.as_str()))
                .map(|(name, target)| (name.clone(), target.clone()))
                .collect::<HashMap<_, _>>();
            return handler_alias_reference(body, &visible);
        }
        return handler_alias_reference(body, aliases);
    }
    handler_expression_children(expression)
        .into_iter()
        .find_map(|child| handler_alias_reference(child, aliases))
}

pub(super) fn remap_dynamic_callable_tag(source: &str, from: &[String], to: &[String]) -> Expr {
    let destination_tag = |target: &str| {
        to.iter()
            .position(|candidate| candidate == target)
            .expect("compatible dynamic target sets") as i128
    };
    let mut remapped = Expr::Integer(destination_tag(
        from.last()
            .expect("dynamic callable has at least two targets"),
    ));
    for (source_tag, target) in from.iter().enumerate().rev().skip(1) {
        remapped = Expr::If {
            condition: Box::new(Expr::Binary(
                Box::new(Expr::Name(source.to_owned())),
                BinaryOp::Eq,
                Box::new(Expr::Integer(source_tag as i128)),
            )),
            then_branch: Box::new(Expr::Integer(destination_tag(target))),
            else_branch: Some(Box::new(remapped)),
        };
    }
    remapped
}

pub(super) fn expand_dynamic_callable_selection(
    selection: Expr,
    sources: &[(String, Vec<String>)],
    targets: &[String],
) -> Expr {
    match selection {
        Expr::Integer(index) => {
            let source = sources
                .get(index as usize)
                .expect("selection leaf has a matching source");
            if source.1.len() == 1 {
                return Expr::Integer(
                    targets
                        .iter()
                        .position(|target| target == &source.1[0])
                        .expect("selection target belongs to its union")
                        as i128,
                );
            }
            remap_dynamic_callable_tag(&source.0, &source.1, targets)
        }
        Expr::If {
            condition,
            then_branch,
            else_branch: Some(else_branch),
        } => Expr::If {
            condition,
            then_branch: Box::new(expand_dynamic_callable_selection(
                *then_branch,
                sources,
                targets,
            )),
            else_branch: Some(Box::new(expand_dynamic_callable_selection(
                *else_branch,
                sources,
                targets,
            ))),
        },
        _ => unreachable!("static callable selection contains only conditions and integer leaves"),
    }
}

pub(super) fn static_callable_selection(
    expression: &Expr,
    targets: &mut Vec<String>,
) -> Option<Expr> {
    match expression {
        Expr::Name(name) => {
            let index = targets.len();
            targets.push(name.clone());
            Some(Expr::Integer(index as i128))
        }
        Expr::Block(statements, Some(tail)) if statements.is_empty() => {
            static_callable_selection(tail, targets)
        }
        Expr::If {
            condition,
            then_branch,
            else_branch: Some(else_branch),
        } => Some(Expr::If {
            condition: condition.clone(),
            then_branch: Box::new(static_callable_selection(then_branch, targets)?),
            else_branch: Some(Box::new(static_callable_selection(else_branch, targets)?)),
        }),
        _ => None,
    }
}

pub(super) fn replace_static_selection_leaves(selection: Expr, calls: &[Expr]) -> Expr {
    match selection {
        Expr::Integer(index) => calls
            .get(index as usize)
            .cloned()
            .expect("selection leaf has a matching specialized call"),
        Expr::If {
            condition,
            then_branch,
            else_branch: Some(else_branch),
        } => Expr::If {
            condition,
            then_branch: Box::new(replace_static_selection_leaves(*then_branch, calls)),
            else_branch: Some(Box::new(replace_static_selection_leaves(
                *else_branch,
                calls,
            ))),
        },
        _ => unreachable!("static callable selection contains only conditions and integer leaves"),
    }
}

pub(super) fn handler_expression_children(expression: &Expr) -> Vec<&Expr> {
    match expression {
        Expr::Unary(_, value)
        | Expr::Try(value)
        | Expr::DoBlock { body: value }
        | Expr::Throw(value)
        | Expr::Unsafe(value)
        | Expr::Borrow { value, .. }
        | Expr::Member(value, _)
        | Expr::ChainMember(value, _)
        | Expr::Loop { body: value } => vec![value],
        Expr::Binary(left, _, right)
        | Expr::Coalesce(left, right)
        | Expr::Assign(left, right)
        | Expr::CompoundAssign(left, _, right) => vec![left, right],
        Expr::HandlerCoalesce {
            scrutinee,
            success,
            fallback,
            ..
        } => vec![scrutinee, success, fallback],
        Expr::HandlerChainCall(chain) => {
            let mut children = vec![chain.scrutinee.as_ref()];
            children.extend(
                chain
                    .groups
                    .iter()
                    .flatten()
                    .map(|argument| &argument.value),
            );
            children.push(&chain.success);
            children.push(&chain.residual);
            children
        }
        Expr::Call(callee, arguments) => {
            let mut children = Vec::with_capacity(arguments.len() + 1);
            children.push(callee.as_ref());
            children.extend(arguments.iter().map(|argument| &argument.value));
            children
        }
        Expr::StructLiteral { fields, .. } => fields.iter().map(|field| &field.value).collect(),
        Expr::Array(elements) => elements.iter().collect(),
        Expr::Index { base, index } => vec![base, index],
        Expr::Block(statements, tail) => {
            let mut children = statements
                .iter()
                .map(|statement| match statement {
                    Stmt::Let(binding) => &binding.value,
                    Stmt::Expr(expression) => expression,
                })
                .collect::<Vec<_>>();
            children.extend(tail.iter().map(|tail| tail.as_ref()));
            children
        }
        Expr::If {
            condition,
            then_branch,
            else_branch,
        } => {
            let mut children = vec![condition.as_ref(), then_branch.as_ref()];
            children.extend(else_branch.iter().map(|branch| branch.as_ref()));
            children
        }
        Expr::Return(value) | Expr::Break(value) => {
            value.iter().map(|value| value.as_ref()).collect()
        }
        Expr::While { condition, body } => vec![condition, body],
        Expr::Match { scrutinee, arms } => {
            let mut children = vec![scrutinee.as_ref()];
            for arm in arms {
                children.extend(arm.guard.iter());
                children.push(&arm.body);
            }
            children
        }
        Expr::Type(_)
        | Expr::Unit
        | Expr::Integer(_)
        | Expr::Bool(_)
        | Expr::Name(_)
        | Expr::Closure(_, _)
        | Expr::Continue => Vec::new(),
    }
}

pub(super) fn expression_handles_effect(expression: &Expr, identity: &str) -> bool {
    if let Expr::Call(inner, action) = expression {
        if matches!(action.as_slice(), [CallArg { label: None, value: Expr::Closure(parameters, _) }] if parameters.is_empty())
        {
            let mut groups = Vec::new();
            if let Expr::Member(effect, member) = flatten_call(inner, &mut groups) {
                if member == "handle"
                    && source_type_expression_name(effect).is_some_and(|effect| effect == identity)
                {
                    return true;
                }
            }
        }
    }
    handler_expression_children(expression)
        .into_iter()
        .any(|child| expression_handles_effect(child, identity))
}

pub(super) fn inject_handler_action_binding(
    expression: &mut Expr,
    identity: &str,
    action_binding: Binding,
) -> bool {
    if let Expr::Call(inner, action) = expression {
        if let [CallArg {
            label: None,
            value: Expr::Closure(parameters, action_body),
        }] = action.as_mut_slice()
        {
            let mut groups = Vec::new();
            if parameters.is_empty()
                && matches!(flatten_call(inner, &mut groups), Expr::Member(effect, member)
                    if member == "handle"
                        && source_type_expression_name(effect).is_some_and(|effect| effect == identity))
            {
                let old_body = (**action_body).clone();
                **action_body =
                    Expr::Block(vec![Stmt::Let(action_binding)], Some(Box::new(old_body)));
                return true;
            }
        }
    }
    match expression {
        Expr::Block(statements, tail) => {
            for statement in statements {
                let child = match statement {
                    Stmt::Let(local) => &mut local.value,
                    Stmt::Expr(expression) => expression,
                };
                if inject_handler_action_binding(child, identity, action_binding.clone()) {
                    return true;
                }
            }
            tail.as_mut()
                .is_some_and(|tail| inject_handler_action_binding(tail, identity, action_binding))
        }
        Expr::Unsafe(body) | Expr::DoBlock { body } => {
            inject_handler_action_binding(body, identity, action_binding)
        }
        _ => false,
    }
}

pub(super) fn rewrite_handler_chain_wrappers(
    expression: &mut Expr,
    canonical: &str,
    success_variant: &str,
    residual_variant: &str,
) {
    if let Expr::Call(callee, arguments) = expression {
        if let Expr::Name(wrapper) = callee.as_ref() {
            if matches!(
                wrapper.as_str(),
                "$handler$chain$wrap$success" | "$handler$chain$wrap$residual"
            ) && arguments.len() == 1
                && arguments[0].label.is_none()
            {
                let mut value = arguments.remove(0).value;
                rewrite_handler_chain_wrappers(
                    &mut value,
                    canonical,
                    success_variant,
                    residual_variant,
                );
                let variant = if wrapper == "$handler$chain$wrap$success" {
                    success_variant
                } else {
                    residual_variant
                };
                let member = Expr::Member(
                    Box::new(Expr::Name(canonical.to_owned())),
                    variant.to_owned(),
                );
                *expression = if variant == "None" {
                    member
                } else {
                    Expr::Call(Box::new(member), vec![CallArg { label: None, value }])
                };
                return;
            }
        }
    }
    match expression {
        Expr::Unary(_, value)
        | Expr::Try(value)
        | Expr::DoBlock { body: value }
        | Expr::Throw(value)
        | Expr::Unsafe(value)
        | Expr::Borrow { value, .. }
        | Expr::Member(value, _)
        | Expr::ChainMember(value, _)
        | Expr::Loop { body: value } => {
            rewrite_handler_chain_wrappers(value, canonical, success_variant, residual_variant)
        }
        Expr::Binary(left, _, right)
        | Expr::Coalesce(left, right)
        | Expr::Assign(left, right)
        | Expr::CompoundAssign(left, _, right) => {
            rewrite_handler_chain_wrappers(left, canonical, success_variant, residual_variant);
            rewrite_handler_chain_wrappers(right, canonical, success_variant, residual_variant);
        }
        Expr::HandlerCoalesce {
            scrutinee,
            success,
            fallback,
            ..
        } => {
            rewrite_handler_chain_wrappers(scrutinee, canonical, success_variant, residual_variant);
            rewrite_handler_chain_wrappers(success, canonical, success_variant, residual_variant);
            rewrite_handler_chain_wrappers(fallback, canonical, success_variant, residual_variant);
        }
        Expr::HandlerChainCall(chain) => {
            rewrite_handler_chain_wrappers(
                &mut chain.scrutinee,
                canonical,
                success_variant,
                residual_variant,
            );
            for argument in chain.groups.iter_mut().flatten() {
                rewrite_handler_chain_wrappers(
                    &mut argument.value,
                    canonical,
                    success_variant,
                    residual_variant,
                );
            }
            rewrite_handler_chain_wrappers(
                &mut chain.success,
                canonical,
                success_variant,
                residual_variant,
            );
            rewrite_handler_chain_wrappers(
                &mut chain.residual,
                canonical,
                success_variant,
                residual_variant,
            );
        }
        Expr::Call(callee, arguments) => {
            rewrite_handler_chain_wrappers(callee, canonical, success_variant, residual_variant);
            for argument in arguments {
                rewrite_handler_chain_wrappers(
                    &mut argument.value,
                    canonical,
                    success_variant,
                    residual_variant,
                );
            }
        }
        Expr::StructLiteral { fields, .. } => {
            for field in fields {
                rewrite_handler_chain_wrappers(
                    &mut field.value,
                    canonical,
                    success_variant,
                    residual_variant,
                );
            }
        }
        Expr::Array(elements) => {
            for element in elements {
                rewrite_handler_chain_wrappers(
                    element,
                    canonical,
                    success_variant,
                    residual_variant,
                );
            }
        }
        Expr::Index { base, index } => {
            rewrite_handler_chain_wrappers(base, canonical, success_variant, residual_variant);
            rewrite_handler_chain_wrappers(index, canonical, success_variant, residual_variant);
        }
        Expr::Block(statements, tail) => {
            for statement in statements {
                let value = match statement {
                    Stmt::Let(binding) => &mut binding.value,
                    Stmt::Expr(value) => value,
                };
                rewrite_handler_chain_wrappers(value, canonical, success_variant, residual_variant);
            }
            if let Some(tail) = tail {
                rewrite_handler_chain_wrappers(tail, canonical, success_variant, residual_variant);
            }
        }
        Expr::Closure(_, body) => {
            rewrite_handler_chain_wrappers(body, canonical, success_variant, residual_variant)
        }
        Expr::If {
            condition,
            then_branch,
            else_branch,
        } => {
            rewrite_handler_chain_wrappers(condition, canonical, success_variant, residual_variant);
            rewrite_handler_chain_wrappers(
                then_branch,
                canonical,
                success_variant,
                residual_variant,
            );
            if let Some(else_branch) = else_branch {
                rewrite_handler_chain_wrappers(
                    else_branch,
                    canonical,
                    success_variant,
                    residual_variant,
                );
            }
        }
        Expr::Return(value) | Expr::Break(value) => {
            if let Some(value) = value {
                rewrite_handler_chain_wrappers(value, canonical, success_variant, residual_variant);
            }
        }
        Expr::While { condition, body } => {
            rewrite_handler_chain_wrappers(condition, canonical, success_variant, residual_variant);
            rewrite_handler_chain_wrappers(body, canonical, success_variant, residual_variant);
        }
        Expr::Match { scrutinee, arms } => {
            rewrite_handler_chain_wrappers(scrutinee, canonical, success_variant, residual_variant);
            for arm in arms {
                if let Some(guard) = &mut arm.guard {
                    rewrite_handler_chain_wrappers(
                        guard,
                        canonical,
                        success_variant,
                        residual_variant,
                    );
                }
                rewrite_handler_chain_wrappers(
                    &mut arm.body,
                    canonical,
                    success_variant,
                    residual_variant,
                );
            }
        }
        Expr::Type(_)
        | Expr::Unit
        | Expr::Integer(_)
        | Expr::Bool(_)
        | Expr::Name(_)
        | Expr::Continue => {}
    }
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

pub(super) fn is_internal_handler_closure_binding(name: &str) -> bool {
    const PREFIXES: &[&str] = &[
        "$handler$continuation$",
        "$handler$closure$continuation$",
        "$handler$recursive$continuation$",
        "$handler$call$continuation$",
        "$handler$frame$",
        "$handler$loop$frame$",
    ];
    PREFIXES.iter().any(|prefix| {
        name.strip_prefix(prefix).is_some_and(|suffix| {
            !suffix.is_empty() && suffix.bytes().all(|byte| byte.is_ascii_digit())
        })
    })
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

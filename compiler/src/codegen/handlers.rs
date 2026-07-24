use std::cell::{Cell, RefCell};
use std::collections::{HashMap, HashSet};
use std::rc::Rc;

use crate::ast::{
    BinaryOp, Binding, CallArg, Expr, Function, FunctionEffects, HandlerChainCall, ItemOrigin,
    MatchArm, Param, PassMode, Pattern, Stmt, Type, Visibility,
};
use crate::core::LangItemKind;

use super::compile_time::source_effect_identity;
use super::effects::{
    call_argument_labels, handled_operation_call, logical_effect_result_source,
    logical_function_result_source, standard_throws_error_source,
};
use super::flow::LowerCtx;
use super::hir::{
    AccessBoundary, ClosureCaptureMode, FunctionSig, LocalCapability, ParamSig,
    RuntimeHandlerAction, Ty,
};
use super::lower::flatten_call;
use super::names::hex_name;
use super::source_rewrite::{
    append_innermost_closure_parameter, handler_match_commit, hygienic_inline_function,
    pattern_contains_binding, pattern_for_suspended_guard, rewrite_handler_returns,
    rewrite_static_function_values, source_type_expression, source_type_expression_name,
    substitute_type_parameters, visit_expr_mut,
};
use super::Analyzer;

pub(super) type SourceContinuation = Rc<dyn Fn(&mut Analyzer, Expr) -> Result<Expr, ()>>;
pub(super) type SourceArgumentsContinuation =
    Rc<dyn Fn(&mut Analyzer, Vec<CallArg>) -> Result<Expr, ()>>;

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
        Expr::While {
            condition, body, ..
        } => {
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
        Expr::While {
            condition, body, ..
        } => vec![condition, body],
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
        Expr::While {
            condition, body, ..
        } => {
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

fn source_borrow_channel_mode(mode: PassMode, ty: &Type) -> Option<PassMode> {
    match (mode, ty) {
        (PassMode::Borrow | PassMode::MutBorrow, _) => Some(mode),
        (_, Type::Borrow { mutable: true, .. }) => Some(PassMode::MutBorrow),
        (_, Type::Borrow { mutable: false, .. }) => Some(PassMode::Borrow),
        _ => None,
    }
}

impl Analyzer {
    pub(super) fn materialize_direct_handler_action(
        &mut self,
        name: &str,
        groups: &[&[CallArg]],
    ) -> Option<Expr> {
        let function = self.functions.get(name)?.clone();
        if groups.len() != function.groups.len() {
            return None;
        }
        let action_positions = self
            .runtime_handler_actions
            .keys()
            .cloned()
            .collect::<Vec<_>>();
        for (candidate, group_index, parameter_index) in action_positions {
            if candidate != name {
                continue;
            }
            let arguments = groups.get(group_index).copied()?;
            let parameter = function.groups.get(group_index)?.get(parameter_index)?;
            let argument_index = if arguments.iter().all(|argument| argument.label.is_none()) {
                parameter_index
            } else {
                arguments.iter().position(|argument| {
                    argument.label.as_deref() == Some(parameter.name.as_str())
                })?
            };
            let Some(CallArg {
                value: Expr::Closure(_, _),
                ..
            }) = arguments.get(argument_index)
            else {
                continue;
            };
            let mut rewritten_groups = groups
                .iter()
                .map(|group| group.to_vec())
                .collect::<Vec<_>>();
            let mut bindings = Vec::new();
            for (earlier_group, rewritten_group) in rewritten_groups
                .iter_mut()
                .enumerate()
                .take(group_index + 1)
            {
                let end = if earlier_group == group_index {
                    argument_index
                } else {
                    rewritten_group.len()
                };
                for (earlier_argument, rewritten_argument) in
                    rewritten_group.iter_mut().enumerate().take(end)
                {
                    let earlier_parameter =
                        function.groups.get(earlier_group)?.get(earlier_argument)?;
                    let parameter_ty = self.lower_source_type(&earlier_parameter.ty);
                    if self
                        .borrow_channel_mode(earlier_parameter.mode, &parameter_ty)
                        .is_some()
                    {
                        return None;
                    }
                    let id = self.next_closure;
                    self.next_closure += 1;
                    let local = format!("$handler$direct$argument${id}");
                    bindings.push(Stmt::Let(Binding {
                        mutable: false,
                        name: local.clone(),
                        annotation: Some(earlier_parameter.ty.clone()),
                        value: rewritten_argument.value.clone(),
                    }));
                    rewritten_argument.value = Expr::Name(local);
                }
            }
            let id = self.next_closure;
            self.next_closure += 1;
            let local = format!("$handler$direct$action${id}");
            bindings.push(Stmt::Let(Binding {
                mutable: true,
                name: local.clone(),
                annotation: Some(parameter.ty.clone()),
                value: arguments[argument_index].value.clone(),
            }));
            rewritten_groups[group_index][argument_index].value = Expr::Name(local);
            let mut call = Expr::Name(name.to_owned());
            for group in rewritten_groups {
                call = Expr::Call(Box::new(call), group);
            }
            return Some(Expr::Block(bindings, Some(Box::new(call))));
        }
        None
    }

    pub(super) fn specialize_capturing_handler_action_binding(
        &mut self,
        binding: &Binding,
        call: &mut Expr,
        context: &LowerCtx,
    ) -> bool {
        let Some(Type::Function { effects, .. }) = binding.annotation.as_ref() else {
            return false;
        };
        let Expr::Closure(parameters, body) = &binding.value else {
            return false;
        };
        let mut group_refs = Vec::new();
        let Expr::Name(target) = flatten_call(call, &mut group_refs) else {
            return false;
        };
        let Some(function) = self.functions.get(target).cloned() else {
            return false;
        };
        if group_refs.len() != function.groups.len() {
            return false;
        }

        let mut action_position = None;
        for ((candidate, group_index, parameter_index), action) in &self.runtime_handler_actions {
            if candidate != target
                || !effects
                    .custom
                    .iter()
                    .any(|effect| source_effect_identity(effect) == action.effect)
            {
                continue;
            }
            let arguments = group_refs.get(*group_index).copied().unwrap_or_default();
            let parameter = &function.groups[*group_index][*parameter_index];
            let argument_index = if arguments.iter().all(|argument| argument.label.is_none()) {
                *parameter_index
            } else {
                let Some(index) = arguments.iter().position(|argument| {
                    argument.label.as_deref() == Some(parameter.name.as_str())
                }) else {
                    continue;
                };
                index
            };
            if matches!(arguments.get(argument_index), Some(CallArg { value: Expr::Name(name), .. }) if name == &binding.name)
            {
                action_position = Some((
                    *group_index,
                    *parameter_index,
                    argument_index,
                    action.clone(),
                    parameter.name.clone(),
                ));
                break;
            }
        }
        let Some((group_index, parameter_index, argument_index, action, parameter_name)) =
            action_position
        else {
            return false;
        };

        let mut bound = parameters
            .iter()
            .map(|parameter| parameter.name.clone())
            .collect::<HashSet<_>>();
        let mut captures = Vec::new();
        if !self.scan_simple_closure_captures(body, &mut bound, context, &mut captures) {
            return false;
        }
        if captures.iter().any(|capture| {
            context
                .lookup(&capture.name)
                .is_none_or(|local| match capture.mode {
                    ClosureCaptureMode::Shared => !self.is_copy_type(&local.ty),
                    ClosureCaptureMode::Mutable => {
                        !local.mutable
                            || local.capability != LocalCapability::Owned
                            || !self.is_copy_type(&local.ty)
                    }
                    ClosureCaptureMode::Move => {
                        local.capability != LocalCapability::Owned
                            || !matches!(
                                local.ty,
                                Ty::Struct(_)
                                    | Ty::Enum(_)
                                    | Ty::Callable(_)
                                    | Ty::Continuation { .. }
                                    | Ty::EffectCallable { .. }
                            )
                    }
                })
        }) {
            return false;
        }

        let specialization = self.next_closure;
        self.next_closure += 1;
        let canonical = format!("$capturing$handler${}${specialization}", hex_name(target));
        let mut specialized = function;
        specialized.name = canonical.clone();
        specialized.groups[group_index].remove(parameter_index);
        let mut replacements = HashMap::new();
        let mut lifted_arguments = Vec::new();
        for (index, capture) in captures.iter().enumerate() {
            let local = context
                .lookup(&capture.name)
                .expect("capture scanner records visible locals");
            let Some(source_ty) = self.source_type_for_ty(&local.ty) else {
                return false;
            };
            let lifted = format!("$handler$action$capture${specialization}${index}");
            replacements.insert(capture.name.clone(), lifted.clone());
            let mode = match capture.mode {
                ClosureCaptureMode::Shared => PassMode::Borrow,
                ClosureCaptureMode::Mutable => PassMode::MutBorrow,
                ClosureCaptureMode::Move => PassMode::Move,
            };
            specialized.groups[group_index].insert(
                parameter_index + index,
                Param {
                    mode,
                    access: None,
                    passing: None,
                    region: None,
                    name: lifted.clone(),
                    ty: source_ty,
                },
            );
            lifted_arguments.push((lifted, capture.name.clone()));
        }
        let mut injected = binding.clone();
        injected.name = parameter_name;
        rewrite_static_function_values(&mut injected.value, &replacements);
        let Some(specialized_body) = specialized.body.as_mut() else {
            return false;
        };
        if !inject_handler_action_binding(specialized_body, &action.effect, injected) {
            return false;
        }

        let signature = FunctionSig {
            groups: specialized
                .groups
                .iter()
                .map(|group| {
                    group
                        .iter()
                        .map(|parameter| ParamSig {
                            name: parameter.name.clone(),
                            ty: self.lower_source_type(&parameter.ty),
                            mode: parameter.mode,
                        })
                        .collect()
                })
                .collect(),
            unsafe_effect: self.function_effects_unsafe(&specialized.effects),
            throws_error: specialized
                .effects
                .throws
                .as_deref()
                .map(|error| self.lower_source_type(error)),
            custom_effects: self.function_effects_custom_identities(&specialized.effects),
            result: specialized
                .return_type
                .as_ref()
                .map(|result| self.lower_source_type(result)),
        };
        self.functions.insert(canonical.clone(), specialized);
        self.signatures.insert(canonical.clone(), signature);
        self.function_origins
            .insert(canonical.clone(), self.function_origins[target].clone());
        let origin = self
            .function_origins
            .get(target)
            .cloned()
            .unwrap_or_default();
        let access = self
            .function_accesses
            .get(target)
            .cloned()
            .unwrap_or(AccessBoundary {
                visibility: Visibility::Private,
                origin,
            });
        self.function_accesses.insert(canonical.clone(), access);
        self.function_order.push(canonical.clone());

        let mut groups = group_refs
            .iter()
            .map(|group| group.to_vec())
            .collect::<Vec<_>>();
        let labeled = groups[group_index]
            .iter()
            .all(|argument| argument.label.is_some());
        groups[group_index].remove(argument_index);
        for (offset, (label, name)) in lifted_arguments.into_iter().enumerate() {
            groups[group_index].insert(
                argument_index + offset,
                CallArg {
                    label: labeled.then_some(label),
                    value: Expr::Name(name),
                },
            );
        }
        let mut rewritten = Expr::Name(canonical);
        for group in groups {
            rewritten = Expr::Call(Box::new(rewritten), group);
        }
        *call = rewritten;
        true
    }

    pub(super) fn distribute_static_handler_selection(
        &mut self,
        name: &str,
        groups: &[&[CallArg]],
        context: &LowerCtx,
    ) -> Option<Expr> {
        let function = self.functions.get(name)?.clone();
        if groups.len() != function.groups.len() || function.groups.first()?.is_empty() {
            return None;
        }
        let Type::Function { effects, .. } = &function.groups[0][0].ty else {
            return None;
        };
        if !effects.custom.iter().any(|effect| {
            let identity = source_effect_identity(effect);
            let root = identity.split('(').next().unwrap_or(&identity);
            self.effect_defs
                .get(root)
                .is_some_and(|definition| !definition.operations.is_empty())
        }) {
            return None;
        }

        let mut ordered_groups = Vec::with_capacity(groups.len());
        for (parameters, arguments) in function.groups.iter().zip(groups) {
            if parameters.len() != arguments.len() {
                return None;
            }
            let ordered = if arguments.iter().all(|argument| argument.label.is_none()) {
                Some(arguments.to_vec())
            } else if arguments.iter().all(|argument| argument.label.is_some()) {
                parameters
                    .iter()
                    .map(|parameter| {
                        let mut matches = arguments.iter().filter(|argument| {
                            argument.label.as_deref() == Some(parameter.name.as_str())
                        });
                        let argument = matches.next()?.clone();
                        matches.next().is_none().then_some(argument)
                    })
                    .collect::<Option<Vec<_>>>()
            } else {
                None
            }?;
            ordered_groups.push(ordered);
        }

        let mut targets = Vec::new();
        let selection = static_callable_selection(&ordered_groups[0][0].value, &mut targets)?;
        if targets.len() < 2
            || targets.iter().any(|target| {
                !self.functions.contains_key(target)
                    && context.lookup(target).is_none_or(|local| {
                        local.partial.as_ref().is_none_or(|partial| {
                            partial.consumed_groups != 0 || partial.capture_count != 0
                        })
                    })
            })
        {
            return None;
        }

        let calls = targets
            .into_iter()
            .map(|target| {
                let mut target_groups = ordered_groups.clone();
                target_groups[0][0] = CallArg {
                    label: None,
                    value: Expr::Name(target),
                };
                let mut call = Expr::Name(name.to_owned());
                for group in target_groups {
                    call = Expr::Call(Box::new(call), group);
                }
                call
            })
            .collect::<Vec<_>>();
        Some(replace_static_selection_leaves(selection, &calls))
    }

    pub(super) fn register_runtime_handler_actions(&mut self) {
        for function_name in self.function_order.clone() {
            let function = self.functions[&function_name].clone();
            let Some(body) = function.body.as_ref() else {
                continue;
            };
            let Some(answer_source) = function.return_type.as_ref() else {
                continue;
            };
            let answer = self.lower_source_type(answer_source);
            for (group_index, group) in function.groups.iter().enumerate() {
                for (parameter_index, parameter) in group.iter().enumerate() {
                    if source_borrow_channel_mode(parameter.mode, &parameter.ty).is_some() {
                        continue;
                    }
                    let Type::Function {
                        groups,
                        effects,
                        result,
                    } = &parameter.ty
                    else {
                        continue;
                    };
                    if self.function_effects_unsafe(effects)
                        || effects.throws.is_some()
                        || !effects.parameters.is_empty()
                        || self.function_effects_custom_identities(effects).len() != 1
                        || groups.len() != 1
                        || groups[0].len() > 1
                    {
                        continue;
                    }
                    let effect = self
                        .function_effects_custom_identities(effects)
                        .into_iter()
                        .next()
                        .expect("exactly one normalized custom effect");
                    let root = effect.split('(').next().unwrap_or(&effect);
                    if self
                        .effect_defs
                        .get(root)
                        .is_none_or(|definition| definition.operations.is_empty())
                        || function
                            .effects
                            .custom
                            .iter()
                            .filter(|candidate| !self.is_standard_unsafe_effect_source(candidate))
                            .any(|candidate| source_effect_identity(candidate) == effect)
                        || !expression_handles_effect(body, &effect)
                    {
                        continue;
                    }
                    let input = groups[0]
                        .first()
                        .map(|input| self.lower_source_type(input))
                        .unwrap_or(Ty::Unit);
                    let output = self.lower_source_type(result);
                    self.runtime_handler_actions.insert(
                        (function_name.clone(), group_index, parameter_index),
                        RuntimeHandlerAction {
                            effect,
                            input,
                            output,
                            answer: answer.clone(),
                            accepts_input: !groups[0].is_empty(),
                        },
                    );
                }
            }
        }
    }

    pub(super) fn transform_handler_expr(
        &mut self,
        expression: Expr,
        handler: Rc<AlgebraicHandler>,
        resume: Option<SourceResume>,
        continuation: SourceContinuation,
    ) -> Result<Expr, ()> {
        if let Expr::Name(name) = &expression {
            if handler.function_aliases.borrow().contains_key(name) {
                self.error(format!(
                    "effectful function alias `{name}` cannot escape its handler or be used as a runtime value"
                ));
                return Err(());
            }
            if handler.dynamic_callables.borrow().contains_key(name) {
                self.error(format!(
                    "dynamic effectful callable `{name}` cannot escape its handler as a runtime value"
                ));
                return Err(());
            }
        }
        if let Some((name, argument)) = internal_handler_return_argument(&expression) {
            let returned = handler
                .return_continuations
                .borrow()
                .get(&name)
                .cloned()
                .unwrap_or_else(|| Rc::new(|_, value| Ok(Expr::Return(Some(Box::new(value))))));
            return self.transform_handler_expr(argument, handler, resume, returned);
        }
        if let Some((name, argument)) = internal_handler_loop_break_argument(&expression) {
            let Some(loop_continuation) = handler.loop_breaks.borrow().get(&name).cloned() else {
                self.error("internal handler loop break escaped its continuation frame");
                return Err(());
            };
            return self.transform_handler_expr(argument, handler, resume, loop_continuation);
        }
        if let Some((operation, mut arguments)) =
            handled_operation_call(&expression, &handler.identity)
        {
            let candidates = handler
                .operations
                .get(&operation)
                .cloned()
                .unwrap_or_default();
            let labels = call_argument_labels(&arguments);
            if labels.is_none() && arguments.iter().any(|argument| argument.label.is_some()) {
                self.error(format!(
                    "cannot mix named and positional arguments in effect operation `{operation}`"
                ));
                return Err(());
            }
            let selected = match labels {
                Some(labels) => candidates
                    .iter()
                    .find(|candidate| candidate.labels == labels),
                None if candidates.len() == 1 => candidates.first(),
                None => {
                    self.error(format!(
                        "overloaded effect operation `{operation}` requires named arguments"
                    ));
                    return Err(());
                }
            };
            let Some(selected) = selected else {
                self.error(format!(
                    "no effect operation `{operation}` matches the supplied argument names"
                ));
                return Err(());
            };
            let Some(clause) = handler.clauses.get(&selected.key).cloned() else {
                self.error(format!("missing handler clause `{operation}`"));
                return Err(());
            };
            let residual_effects = selected.residual_effects.clone();
            if arguments.len() != clause.parameters.len() {
                self.error(format!(
                    "effect operation `{operation}` expects {} argument(s), found {}",
                    clause.parameters.len(),
                    arguments.len()
                ));
                return Err(());
            }
            for argument in &mut arguments {
                argument.label = None;
            }
            let handler_for_clause = handler.clone();
            let resume_for_arguments = resume.clone();
            let completed: SourceArgumentsContinuation = Rc::new(move |analyzer, arguments| {
                let mut bindings = Vec::new();
                let mut residual_effects = residual_effects.clone();
                if handler_for_clause.lexical_unsafe_depth.get() > 0 {
                    analyzer.strip_authorized_unsafe_effects(&mut residual_effects);
                }
                if residual_effects != FunctionEffects::default() {
                    let gate_id = analyzer.next_closure;
                    analyzer.next_closure += 1;
                    let gate_name = format!("$handler$operation$effects${gate_id}");
                    bindings.push(Stmt::Let(Binding {
                        mutable: false,
                        name: gate_name.clone(),
                        annotation: Some(Type::Function {
                            groups: vec![Vec::new()],
                            effects: residual_effects.clone(),
                            result: Box::new(
                                analyzer.effect_abi_result_source(Type::Unit, &residual_effects),
                            ),
                        }),
                        value: Expr::Closure(Vec::new(), Box::new(Expr::Unit)),
                    }));
                    bindings.push(Stmt::Expr(Expr::Call(
                        Box::new(Expr::Name(gate_name)),
                        Vec::new(),
                    )));
                }
                bindings.extend(clause.parameters.iter().zip(arguments).map(
                    |(parameter, argument)| {
                        Stmt::Let(Binding {
                            mutable: false,
                            name: parameter.name.clone(),
                            annotation: contextual_annotation(parameter),
                            value: argument.value,
                        })
                    },
                ));
                if clause.resume_input.is_none() {
                    if clause.resume.is_some() {
                        analyzer.error(
                            "internal handler clause has a resume name but no continuation input",
                        );
                        return Err(());
                    }
                    let identity: SourceContinuation = Rc::new(|_, value| Ok(value));
                    let body = analyzer.transform_handler_expr(
                        clause.body.clone(),
                        handler_for_clause.clone(),
                        None,
                        identity,
                    )?;
                    return Ok(Expr::Block(bindings, Some(Box::new(body))));
                }
                let Some(input) = clause.resume_input.clone() else {
                    analyzer.error("effect operation is missing its continuation input type");
                    return Err(());
                };
                let Some(answer) = handler_for_clause.result_source.clone() else {
                    analyzer.error(
                        "an algebraic continuation requires a contextual handler answer type",
                    );
                    return Err(());
                };
                let continuation_id = analyzer.next_closure;
                analyzer.next_closure += 1;
                let runtime_name = format!("$handler$continuation${continuation_id}");
                let input_name = format!("$handler$resume$value${continuation_id}");
                let continuation_body = continuation(analyzer, Expr::Name(input_name.clone()))?;
                bindings.push(Stmt::Let(Binding {
                    mutable: true,
                    name: runtime_name.clone(),
                    annotation: Some(Type::Function {
                        groups: vec![vec![input.clone()]],
                        effects: FunctionEffects::default(),
                        result: Box::new(answer),
                    }),
                    value: Expr::Closure(
                        vec![Param {
                            mode: PassMode::Inferred,
                            access: None,
                            passing: None,
                            region: None,
                            name: input_name,
                            ty: input,
                        }],
                        Box::new(continuation_body),
                    ),
                }));
                let source_resume = SourceResume {
                    name: clause
                        .resume
                        .clone()
                        .expect("operation clauses have resume"),
                    runtime_name,
                    uses: Rc::new(Cell::new(0)),
                };
                let identity: SourceContinuation = Rc::new(|_, value| Ok(value));
                let body = analyzer.transform_handler_expr(
                    clause.body.clone(),
                    handler_for_clause.clone(),
                    Some(source_resume),
                    identity,
                )?;
                Ok(Expr::Block(bindings, Some(Box::new(body))))
            });
            return self.transform_handler_arguments(
                arguments,
                Vec::new(),
                handler,
                resume_for_arguments,
                completed,
            );
        }

        if let Some(source_resume) = &resume {
            if let Some(argument) = resume_call_argument(&expression, &source_resume.name) {
                let uses = source_resume.uses.get() + 1;
                source_resume.uses.set(uses);
                if uses > 1 {
                    self.error(format!(
                        "continuation `{}` is one-shot and cannot be resumed more than once",
                        source_resume.name
                    ));
                    return Err(());
                }
                let runtime_name = source_resume.runtime_name.clone();
                let current = continuation.clone();
                let invoked: SourceContinuation = Rc::new(move |analyzer, value| {
                    current(
                        analyzer,
                        Expr::Call(
                            Box::new(Expr::Name(runtime_name.clone())),
                            vec![CallArg { label: None, value }],
                        ),
                    )
                });
                return self.transform_handler_expr(argument, handler, resume, invoked);
            }
            if matches!(&expression, Expr::Name(name) if name == &source_resume.name) {
                self.error(format!(
                    "continuation `{}` cannot escape its handler clause",
                    source_resume.name
                ));
                return Err(());
            }
        }

        if matches!(&expression, Expr::Call(_, _)) {
            if let Some(result) = self.transform_erased_effect_callable_call(
                &expression,
                handler.clone(),
                resume.clone(),
                continuation.clone(),
            ) {
                return result;
            }
            if let Some(result) = self.transform_effectful_chain_call(
                &expression,
                handler.clone(),
                resume.clone(),
                continuation.clone(),
            ) {
                return result;
            }
            if let Some(result) = self.transform_nested_effect_handler(
                &expression,
                handler.clone(),
                resume.clone(),
                continuation.clone(),
            ) {
                return result;
            }
            if let Some(result) = self.transform_resumable_closure_call(
                &expression,
                handler.clone(),
                resume.clone(),
                continuation.clone(),
            ) {
                return result;
            }
            if let Some(result) = self.transform_dynamic_callable_call(
                &expression,
                handler.clone(),
                resume.clone(),
                continuation.clone(),
            ) {
                return result;
            }
            if let Some(result) = self.transform_effectful_named_call(
                &expression,
                handler.clone(),
                resume.clone(),
                continuation.clone(),
            ) {
                return result;
            }
        }

        match expression {
            Expr::Block(statements, tail) => self.transform_handler_block(
                statements,
                tail.map(|tail| *tail),
                handler,
                resume,
                continuation,
            ),
            Expr::Unary(operator, operand) => {
                let next = continuation.clone();
                self.transform_handler_expr(
                    *operand,
                    handler,
                    resume,
                    Rc::new(move |analyzer, value| {
                        next(analyzer, Expr::Unary(operator, Box::new(value)))
                    }),
                )
            }
            Expr::Binary(left, operator, right) => {
                if matches!(operator, BinaryOp::And | BinaryOp::Or) {
                    let right = *right;
                    let handler_for_right = handler.clone();
                    let resume_for_right = resume.clone();
                    let next = continuation.clone();
                    return self.transform_handler_expr(
                        *left,
                        handler,
                        resume,
                        Rc::new(move |analyzer, left| {
                            let short_circuit = operator == BinaryOp::Or;
                            let next_for_right = next.clone();
                            let right_branch = analyzer.transform_handler_expr(
                                right.clone(),
                                handler_for_right.clone(),
                                resume_for_right.clone(),
                                next_for_right,
                            )?;
                            let short_value = next(analyzer, Expr::Bool(short_circuit))?;
                            Ok(Expr::If {
                                condition: Box::new(left),
                                then_branch: Box::new(if short_circuit {
                                    short_value.clone()
                                } else {
                                    right_branch.clone()
                                }),
                                else_branch: Some(Box::new(if short_circuit {
                                    right_branch
                                } else {
                                    short_value
                                })),
                            })
                        }),
                    );
                }
                let right = *right;
                let handler_for_right = handler.clone();
                let resume_for_right = resume.clone();
                let next = continuation.clone();
                self.transform_handler_expr(
                    *left,
                    handler,
                    resume,
                    Rc::new(move |analyzer, left| {
                        let left = left.clone();
                        let next = next.clone();
                        analyzer.transform_handler_expr(
                            right.clone(),
                            handler_for_right.clone(),
                            resume_for_right.clone(),
                            Rc::new(move |analyzer, right| {
                                next(
                                    analyzer,
                                    Expr::Binary(Box::new(left.clone()), operator, Box::new(right)),
                                )
                            }),
                        )
                    }),
                )
            }
            Expr::Coalesce(left, right) => {
                let right = *right;
                let handler_for_fallback = handler.clone();
                let resume_for_fallback = resume.clone();
                let next = continuation.clone();
                self.transform_handler_expr(
                    *left,
                    handler,
                    resume,
                    Rc::new(move |analyzer, scrutinee| {
                        let payload_id = analyzer.next_closure;
                        analyzer.next_closure += 1;
                        let payload = format!("$handler$coalesce$payload${payload_id}");
                        let success = next(analyzer, Expr::Name(payload.clone()))?;
                        let fallback = analyzer.transform_handler_expr(
                            right.clone(),
                            handler_for_fallback.clone(),
                            resume_for_fallback.clone(),
                            next.clone(),
                        )?;
                        Ok(Expr::HandlerCoalesce {
                            scrutinee: Box::new(scrutinee),
                            payload,
                            success: Box::new(success),
                            fallback: Box::new(fallback),
                        })
                    }),
                )
            }
            Expr::HandlerCoalesce {
                scrutinee,
                payload,
                success,
                fallback,
            } => {
                let handler_for_branches = handler.clone();
                let resume_for_branches = resume.clone();
                let next = continuation.clone();
                self.transform_handler_expr(
                    *scrutinee,
                    handler,
                    resume,
                    Rc::new(move |analyzer, scrutinee| {
                        let success = analyzer.transform_handler_expr(
                            (*success).clone(),
                            handler_for_branches.clone(),
                            resume_for_branches.clone(),
                            next.clone(),
                        )?;
                        let fallback = analyzer.transform_handler_expr(
                            (*fallback).clone(),
                            handler_for_branches.clone(),
                            resume_for_branches.clone(),
                            next.clone(),
                        )?;
                        Ok(Expr::HandlerCoalesce {
                            scrutinee: Box::new(scrutinee),
                            payload: payload.clone(),
                            success: Box::new(success),
                            fallback: Box::new(fallback),
                        })
                    }),
                )
            }
            Expr::HandlerChainCall(chain) => {
                let HandlerChainCall {
                    scrutinee,
                    payload,
                    error,
                    member,
                    groups,
                    success,
                    residual,
                } = *chain;
                let handler_for_branches = handler.clone();
                let resume_for_branches = resume.clone();
                let next = continuation.clone();
                self.transform_handler_expr(
                    *scrutinee,
                    handler,
                    resume,
                    Rc::new(move |analyzer, scrutinee| {
                        let success = analyzer.transform_handler_expr(
                            (*success).clone(),
                            handler_for_branches.clone(),
                            resume_for_branches.clone(),
                            next.clone(),
                        )?;
                        let residual = analyzer.transform_handler_expr(
                            (*residual).clone(),
                            handler_for_branches.clone(),
                            resume_for_branches.clone(),
                            next.clone(),
                        )?;
                        Ok(Expr::HandlerChainCall(Box::new(HandlerChainCall {
                            scrutinee: Box::new(scrutinee),
                            payload: payload.clone(),
                            error: error.clone(),
                            member: member.clone(),
                            groups: groups.clone(),
                            success: Box::new(success),
                            residual: Box::new(residual),
                        })))
                    }),
                )
            }
            Expr::Assign(place, value) => {
                let place = *place;
                if let Expr::Name(destination) = &place {
                    let destination_callable =
                        handler.dynamic_callables.borrow().get(destination).cloned();
                    if let Some(destination_callable) = destination_callable {
                        let Expr::Name(source) = value.as_ref() else {
                            self.error(format!(
                                "dynamic effectful callable `{destination}` must be assigned from another compatible dynamic callable"
                            ));
                            return Err(());
                        };
                        let Some(source_callable) =
                            handler.dynamic_callables.borrow().get(source).cloned()
                        else {
                            self.error(format!(
                                "dynamic effectful callable `{destination}` cannot be assigned from `{source}`"
                            ));
                            return Err(());
                        };
                        if destination_callable.group_lengths != source_callable.group_lengths
                            || destination_callable.targets.len() != source_callable.targets.len()
                            || destination_callable.targets.iter().any(|target| {
                                !source_callable
                                    .targets
                                    .iter()
                                    .any(|source| source == target)
                            })
                        {
                            self.error(format!(
                                "dynamic effectful callable assignment from `{source}` to `{destination}` has an incompatible target set"
                            ));
                            return Err(());
                        }
                        let value = remap_dynamic_callable_tag(
                            source,
                            &source_callable.targets,
                            &destination_callable.targets,
                        );
                        return continuation(self, Expr::Assign(Box::new(place), Box::new(value)));
                    }
                }
                let next = continuation.clone();
                self.transform_handler_expr(
                    *value,
                    handler,
                    resume,
                    Rc::new(move |analyzer, value| {
                        next(
                            analyzer,
                            Expr::Assign(Box::new(place.clone()), Box::new(value)),
                        )
                    }),
                )
            }
            Expr::CompoundAssign(place, operator, value) => {
                let place = *place;
                let next = continuation.clone();
                self.transform_handler_expr(
                    *value,
                    handler,
                    resume,
                    Rc::new(move |analyzer, value| {
                        next(
                            analyzer,
                            Expr::CompoundAssign(
                                Box::new(place.clone()),
                                operator,
                                Box::new(value),
                            ),
                        )
                    }),
                )
            }
            Expr::If {
                condition,
                then_branch,
                else_branch,
            } => {
                let handler_for_branches = handler.clone();
                let resume_for_branches = resume.clone();
                let next = continuation.clone();
                self.transform_handler_expr(
                    *condition,
                    handler,
                    resume,
                    Rc::new(move |analyzer, condition| {
                        let then_branch = analyzer.transform_handler_expr(
                            (*then_branch).clone(),
                            handler_for_branches.clone(),
                            resume_for_branches.clone(),
                            next.clone(),
                        )?;
                        let else_branch = match &else_branch {
                            Some(branch) => Some(Box::new(analyzer.transform_handler_expr(
                                (**branch).clone(),
                                handler_for_branches.clone(),
                                resume_for_branches.clone(),
                                next.clone(),
                            )?)),
                            None => Some(Box::new(next(analyzer, Expr::Unit)?)),
                        };
                        Ok(Expr::If {
                            condition: Box::new(condition),
                            then_branch: Box::new(then_branch),
                            else_branch,
                        })
                    }),
                )
            }
            Expr::Array(elements) => {
                let arguments = elements
                    .into_iter()
                    .map(|value| CallArg { label: None, value })
                    .collect();
                let completed: SourceArgumentsContinuation = Rc::new(move |analyzer, values| {
                    continuation(
                        analyzer,
                        Expr::Array(values.into_iter().map(|value| value.value).collect()),
                    )
                });
                self.transform_handler_arguments(arguments, Vec::new(), handler, resume, completed)
            }
            Expr::StructLiteral {
                constructor,
                fields,
            } => {
                let completed: SourceArgumentsContinuation = Rc::new(move |analyzer, fields| {
                    continuation(
                        analyzer,
                        Expr::StructLiteral {
                            constructor: constructor.clone(),
                            fields,
                        },
                    )
                });
                self.transform_handler_arguments(fields, Vec::new(), handler, resume, completed)
            }
            Expr::Index { base, index } => {
                let index = *index;
                let handler_for_index = handler.clone();
                let resume_for_index = resume.clone();
                let next = continuation.clone();
                self.transform_handler_expr(
                    *base,
                    handler,
                    resume,
                    Rc::new(move |analyzer, base| {
                        let base = base.clone();
                        let next = next.clone();
                        analyzer.transform_handler_expr(
                            index.clone(),
                            handler_for_index.clone(),
                            resume_for_index.clone(),
                            Rc::new(move |analyzer, index| {
                                next(
                                    analyzer,
                                    Expr::Index {
                                        base: Box::new(base.clone()),
                                        index: Box::new(index),
                                    },
                                )
                            }),
                        )
                    }),
                )
            }
            Expr::Member(base, member) => {
                let next = continuation.clone();
                self.transform_handler_expr(
                    *base,
                    handler,
                    resume,
                    Rc::new(move |analyzer, base| {
                        next(analyzer, Expr::Member(Box::new(base), member.clone()))
                    }),
                )
            }
            Expr::ChainMember(base, member) => {
                let next = continuation.clone();
                self.transform_handler_expr(
                    *base,
                    handler,
                    resume,
                    Rc::new(move |analyzer, base| {
                        next(analyzer, Expr::ChainMember(Box::new(base), member.clone()))
                    }),
                )
            }
            Expr::Try(value) => {
                let next = continuation.clone();
                self.transform_handler_expr(
                    *value,
                    handler,
                    resume,
                    Rc::new(move |analyzer, value| next(analyzer, Expr::Try(Box::new(value)))),
                )
            }
            Expr::Throw(value) => {
                if standard_throws_error_source(
                    &handler.source,
                    self.lang_item_name(LangItemKind::ThrowsEffect),
                )
                .is_some()
                {
                    let operation = Expr::Call(
                        Box::new(Expr::Member(
                            Box::new(source_type_expression(&handler.source)),
                            "raise".to_owned(),
                        )),
                        vec![CallArg {
                            label: None,
                            value: *value,
                        }],
                    );
                    return self.transform_handler_expr(operation, handler, resume, continuation);
                }
                let identity: SourceContinuation =
                    Rc::new(|_, value| Ok(Expr::Throw(Box::new(value))));
                self.transform_handler_expr(*value, handler, resume, identity)
            }
            Expr::Unsafe(value) => {
                let next = continuation.clone();
                let depth = handler.lexical_unsafe_depth.get();
                handler.lexical_unsafe_depth.set(depth + 1);
                let transformed = self.transform_handler_expr(
                    *value,
                    handler.clone(),
                    resume,
                    Rc::new(move |analyzer, value| next(analyzer, Expr::Unsafe(Box::new(value)))),
                );
                handler.lexical_unsafe_depth.set(depth);
                transformed
            }
            Expr::DoBlock { body } => {
                let next = continuation.clone();
                self.transform_handler_expr(
                    *body,
                    handler,
                    resume,
                    Rc::new(move |analyzer, body| {
                        next(
                            analyzer,
                            Expr::DoBlock {
                                body: Box::new(body),
                            },
                        )
                    }),
                )
            }
            Expr::Match { scrutinee, arms } => {
                let has_effectful_guard = arms.iter().any(|arm| {
                    arm.guard
                        .as_ref()
                        .is_some_and(|guard| self.handler_expression_may_suspend(guard, &handler))
                });
                if has_effectful_guard {
                    let can_delay_pattern_transfers =
                        arms.iter().enumerate().all(|(index, arm)| {
                            let guard_is_effectful = arm.guard.as_ref().is_some_and(|guard| {
                                self.handler_expression_may_suspend(guard, &handler)
                            });
                            guard_is_effectful
                                || !pattern_contains_binding(&arm.pattern)
                                || !arms[index + 1..].iter().any(|later| {
                                    later.guard.as_ref().is_some_and(|guard| {
                                        self.handler_expression_may_suspend(guard, &handler)
                                    })
                                })
                        });
                    let handler_for_arms = handler.clone();
                    let resume_for_arms = resume.clone();
                    let next = continuation.clone();
                    return self.transform_handler_expr(
                        *scrutinee,
                        handler,
                        resume,
                        Rc::new(move |analyzer, scrutinee| {
                            let match_id = analyzer.next_closure;
                            analyzer.next_closure += 1;
                            let input = if can_delay_pattern_transfers {
                                format!("$handler$match$inspect$input${match_id}")
                            } else {
                                format!("$handler$match$input${match_id}")
                            };
                            let candidates = analyzer.transform_handler_match_candidates(
                                &input,
                                &arms,
                                handler_for_arms.clone(),
                                resume_for_arms.clone(),
                                next.clone(),
                            )?;
                            Ok(Expr::Block(
                                vec![Stmt::Let(Binding {
                                    mutable: false,
                                    name: input,
                                    annotation: None,
                                    value: scrutinee,
                                })],
                                Some(Box::new(candidates)),
                            ))
                        }),
                    );
                }
                let handler_for_arms = handler.clone();
                let resume_for_arms = resume.clone();
                let next = continuation.clone();
                self.transform_handler_expr(
                    *scrutinee,
                    handler,
                    resume,
                    Rc::new(move |analyzer, scrutinee| {
                        let mut transformed = Vec::with_capacity(arms.len());
                        for arm in &arms {
                            transformed.push(MatchArm {
                                pattern: arm.pattern.clone(),
                                guard: arm.guard.clone(),
                                body: analyzer.transform_handler_expr(
                                    arm.body.clone(),
                                    handler_for_arms.clone(),
                                    resume_for_arms.clone(),
                                    next.clone(),
                                )?,
                            });
                        }
                        Ok(Expr::Match {
                            scrutinee: Box::new(scrutinee),
                            arms: transformed,
                        })
                    }),
                )
            }
            Expr::While {
                condition, body, ..
            } => {
                self.transform_handler_loop(Some(*condition), *body, handler, resume, continuation)
            }
            Expr::Loop { body } => {
                self.transform_handler_loop(None, *body, handler, resume, continuation)
            }
            Expr::Call(callee, arguments) => {
                let completed: SourceArgumentsContinuation = Rc::new(move |analyzer, arguments| {
                    continuation(analyzer, Expr::Call(callee.clone(), arguments))
                });
                self.transform_handler_arguments(arguments, Vec::new(), handler, resume, completed)
            }
            other => continuation(self, other),
        }
    }

    fn transform_erased_effect_callable_call(
        &mut self,
        expression: &Expr,
        handler: Rc<AlgebraicHandler>,
        resume: Option<SourceResume>,
        continuation: SourceContinuation,
    ) -> Option<Result<Expr, ()>> {
        let mut groups = Vec::new();
        let Expr::Name(name) = flatten_call(expression, &mut groups) else {
            return None;
        };
        let action = handler.erased_callables.get(name)?.clone();
        if groups.len() != 1
            || groups[0].len() != usize::from(action.accepts_input)
            || groups[0].iter().any(|argument| argument.label.is_some())
        {
            self.error(format!(
                "erased effect callable `{name}` must be fully applied with {} positional input(s)",
                usize::from(action.accepts_input)
            ));
            return Some(Err(()));
        }
        let arguments = groups[0].to_vec();
        let action_name = name.clone();
        let completed: SourceArgumentsContinuation = Rc::new(move |analyzer, arguments| {
            let specialization = analyzer.next_closure;
            analyzer.next_closure += 1;
            let continuation_name = format!("$handler$erased$action$continuation${specialization}");
            let continuation_value_name = format!("$handler$erased$action$value${specialization}");
            let continuation_body =
                continuation(analyzer, Expr::Name(continuation_value_name.clone()))?;
            let continuation_binding = Binding {
                mutable: true,
                name: continuation_name.clone(),
                annotation: Some(Type::Function {
                    groups: vec![vec![action.output.clone()]],
                    effects: FunctionEffects::default(),
                    result: Box::new(action.answer.clone()),
                }),
                value: Expr::Closure(
                    vec![Param {
                        mode: PassMode::Inferred,
                        access: None,
                        passing: None,
                        region: None,
                        name: continuation_value_name,
                        ty: action.output.clone(),
                    }],
                    Box::new(continuation_body),
                ),
            };
            let erased_continuation_name =
                format!("$handler$erased$action$continuation$value${specialization}");
            let erased_continuation_binding = Binding {
                mutable: true,
                name: erased_continuation_name.clone(),
                annotation: Some(Type::Named(
                    analyzer
                        .lang_item_name(LangItemKind::Continuation)
                        .to_owned(),
                    vec![action.output.clone(), action.answer.clone()],
                )),
                value: Expr::Call(
                    Box::new(Expr::Name("$handler$erase$continuation".to_owned())),
                    vec![CallArg {
                        label: None,
                        value: Expr::Name(continuation_name),
                    }],
                ),
            };
            let input = arguments
                .into_iter()
                .next()
                .map(|argument| argument.value)
                .unwrap_or(Expr::Unit);
            let invoke = Expr::Call(
                Box::new(Expr::Name("$handler$invoke$effect$callable".to_owned())),
                vec![
                    CallArg {
                        label: None,
                        value: Expr::Name(action_name.clone()),
                    },
                    CallArg {
                        label: None,
                        value: input,
                    },
                    CallArg {
                        label: None,
                        value: Expr::Name(erased_continuation_name),
                    },
                ],
            );
            Ok(Expr::Block(
                vec![
                    Stmt::Let(continuation_binding),
                    Stmt::Let(erased_continuation_binding),
                ],
                Some(Box::new(invoke)),
            ))
        });
        Some(self.transform_handler_arguments(arguments, Vec::new(), handler, resume, completed))
    }

    fn transform_effectful_chain_call(
        &mut self,
        expression: &Expr,
        handler: Rc<AlgebraicHandler>,
        resume: Option<SourceResume>,
        continuation: SourceContinuation,
    ) -> Option<Result<Expr, ()>> {
        let mut source_groups = Vec::new();
        let Expr::ChainMember(base, member) = flatten_call(expression, &mut source_groups) else {
            return None;
        };
        let base_may_suspend = self.handler_expression_may_suspend(base, &handler);
        let arguments_may_suspend = source_groups
            .iter()
            .flat_map(|group| group.iter())
            .any(|argument| self.handler_expression_may_suspend(&argument.value, &handler));
        if !base_may_suspend && !arguments_may_suspend {
            return None;
        }
        let groups = source_groups
            .iter()
            .map(|group| group.to_vec())
            .collect::<Vec<_>>();
        let member = member.clone();
        let handler_for_call = handler.clone();
        let resume_for_call = resume.clone();
        Some(self.transform_handler_expr(
            (**base).clone(),
            handler,
            resume,
            Rc::new(move |analyzer, scrutinee| {
                let id = analyzer.next_closure;
                analyzer.next_closure += 1;
                let payload = format!("$handler$chain$payload${id}");
                let error = format!("$handler$chain$error${id}");
                let success_wrap = |value| {
                    Expr::Call(
                        Box::new(Expr::Name("$handler$chain$wrap$success".to_owned())),
                        vec![CallArg { label: None, value }],
                    )
                };
                let residual_wrap = Expr::Call(
                    Box::new(Expr::Name("$handler$chain$wrap$residual".to_owned())),
                    vec![CallArg {
                        label: None,
                        value: Expr::Name(error.clone()),
                    }],
                );
                let residual = continuation(analyzer, residual_wrap)?;
                let completed = {
                    let continuation = continuation.clone();
                    Rc::new(move |analyzer: &mut Analyzer, value: Expr| {
                        continuation(analyzer, success_wrap(value))
                    }) as SourceContinuation
                };
                let callee = Expr::Member(Box::new(Expr::Name(payload.clone())), member.clone());
                let success = analyzer.transform_handler_call_groups(
                    callee,
                    groups.clone(),
                    handler_for_call.clone(),
                    resume_for_call.clone(),
                    completed,
                )?;
                Ok(Expr::HandlerChainCall(Box::new(HandlerChainCall {
                    scrutinee: Box::new(scrutinee),
                    payload,
                    error,
                    member: member.clone(),
                    groups: groups.clone(),
                    success: Box::new(success),
                    residual: Box::new(residual),
                })))
            }),
        ))
    }

    fn transform_handler_call_groups(
        &mut self,
        callee: Expr,
        mut groups: Vec<Vec<CallArg>>,
        handler: Rc<AlgebraicHandler>,
        resume: Option<SourceResume>,
        continuation: SourceContinuation,
    ) -> Result<Expr, ()> {
        if groups.is_empty() {
            return continuation(self, callee);
        }
        let arguments = groups.remove(0);
        let next_handler = handler.clone();
        let next_resume = resume.clone();
        self.transform_handler_arguments(
            arguments,
            Vec::new(),
            handler,
            resume,
            Rc::new(move |analyzer, arguments| {
                analyzer.transform_handler_call_groups(
                    Expr::Call(Box::new(callee.clone()), arguments),
                    groups.clone(),
                    next_handler.clone(),
                    next_resume.clone(),
                    continuation.clone(),
                )
            }),
        )
    }

    fn transform_handler_match_candidates(
        &mut self,
        scrutinee: &str,
        arms: &[MatchArm],
        handler: Rc<AlgebraicHandler>,
        resume: Option<SourceResume>,
        continuation: SourceContinuation,
    ) -> Result<Expr, ()> {
        if !arms.iter().any(|arm| {
            arm.guard
                .as_ref()
                .is_some_and(|guard| self.handler_expression_may_suspend(guard, &handler))
        }) {
            let mut transformed = Vec::with_capacity(arms.len());
            for arm in arms {
                transformed.push(MatchArm {
                    pattern: arm.pattern.clone(),
                    guard: arm.guard.clone(),
                    body: self.transform_handler_expr(
                        arm.body.clone(),
                        handler.clone(),
                        resume.clone(),
                        continuation.clone(),
                    )?,
                });
            }
            return Ok(handler_match_commit(scrutinee, transformed));
        }
        let Some((arm, remaining)) = arms.split_first() else {
            return Ok(Expr::Loop {
                body: Box::new(Expr::Unit),
            });
        };
        let body = self.transform_handler_expr(
            arm.body.clone(),
            handler.clone(),
            resume.clone(),
            continuation.clone(),
        )?;
        let covers_all = matches!(arm.pattern, Pattern::Wildcard | Pattern::Binding(_));
        let guard_is_effectful = arm
            .guard
            .as_ref()
            .is_some_and(|guard| self.handler_expression_may_suspend(guard, &handler));
        let delays_pattern_transfer = guard_is_effectful;

        let committed_body = if delays_pattern_transfer {
            handler_match_commit(
                scrutinee,
                vec![
                    MatchArm {
                        pattern: arm.pattern.clone(),
                        guard: None,
                        body: body.clone(),
                    },
                    MatchArm {
                        pattern: Pattern::Wildcard,
                        guard: None,
                        body: Expr::Loop {
                            body: Box::new(Expr::Unit),
                        },
                    },
                ],
            )
        } else {
            body.clone()
        };

        let candidate = if let Some(guard) = &arm.guard {
            if guard_is_effectful {
                let false_branch = self.transform_handler_match_candidates(
                    scrutinee,
                    remaining,
                    handler.clone(),
                    resume.clone(),
                    continuation.clone(),
                )?;
                let true_branch = committed_body;
                self.transform_handler_expr(
                    guard.clone(),
                    handler.clone(),
                    resume.clone(),
                    Rc::new(move |_, condition| {
                        Ok(Expr::If {
                            condition: Box::new(condition),
                            then_branch: Box::new(true_branch.clone()),
                            else_branch: Some(Box::new(false_branch.clone())),
                        })
                    }),
                )?
            } else {
                body
            }
        } else {
            body
        };

        let mut candidates = vec![MatchArm {
            pattern: if delays_pattern_transfer {
                pattern_for_suspended_guard(
                    &arm.pattern,
                    arm.guard.as_ref().expect("effectful guard exists"),
                )
            } else {
                arm.pattern.clone()
            },
            guard: if guard_is_effectful {
                None
            } else {
                arm.guard.clone()
            },
            body: candidate,
        }];
        if !covers_all || arm.guard.is_some() {
            let fallback = self.transform_handler_match_candidates(
                scrutinee,
                remaining,
                handler,
                resume,
                continuation,
            )?;
            candidates.push(MatchArm {
                pattern: Pattern::Wildcard,
                guard: None,
                body: fallback,
            });
        }
        Ok(Expr::Match {
            scrutinee: Box::new(Expr::Name(scrutinee.to_owned())),
            arms: candidates,
        })
    }

    fn handler_expression_may_suspend(
        &self,
        expression: &Expr,
        handler: &AlgebraicHandler,
    ) -> bool {
        if handled_operation_call(expression, &handler.identity).is_some() {
            return true;
        }
        if matches!(expression, Expr::Call(_, _)) {
            let mut groups = Vec::new();
            if let Expr::Name(name) = flatten_call(expression, &mut groups) {
                if handler.resumable_closures.borrow().contains_key(name)
                    || handler.dynamic_callables.borrow().contains_key(name)
                    || handler.erased_callables.contains_key(name)
                {
                    return true;
                }
                if self.functions.get(name).is_some_and(|function| {
                    function
                        .effects
                        .custom
                        .iter()
                        .any(|effect| source_effect_identity(effect) == handler.identity)
                }) {
                    return true;
                }
            }
        }
        handler_expression_children(expression)
            .into_iter()
            .any(|child| self.handler_expression_may_suspend(child, handler))
    }

    fn transform_nested_effect_handler(
        &mut self,
        expression: &Expr,
        handler: Rc<AlgebraicHandler>,
        resume: Option<SourceResume>,
        continuation: SourceContinuation,
    ) -> Option<Result<Expr, ()>> {
        let Expr::Call(inner_callee, action_arguments) = expression else {
            return None;
        };
        let [CallArg {
            label: None,
            value: Expr::Closure(action_parameters, action_body),
        }] = action_arguments.as_slice()
        else {
            return None;
        };
        if !action_parameters.is_empty() {
            return None;
        }
        let mut groups = Vec::new();
        let Expr::Member(effect, member) = flatten_call(inner_callee, &mut groups) else {
            return None;
        };
        if member != "handle" || groups.len() != 1 {
            return None;
        }
        let effect_name = source_type_expression_name(effect)?;
        let root_name = effect_name.split('(').next().unwrap_or(&effect_name);
        if !self.effect_defs.contains_key(root_name) || effect_name == handler.identity {
            return None;
        }

        let Expr::Call(handler_head, clause_arguments) = inner_callee.as_ref() else {
            return None;
        };
        let mut transformed_clauses = Vec::with_capacity(clause_arguments.len());
        for argument in clause_arguments {
            let value = if let Expr::Closure(parameters, body) = &argument.value {
                let identity: SourceContinuation = Rc::new(|_, value| Ok(value));
                let transformed = match self.transform_handler_expr(
                    (**body).clone(),
                    handler.clone(),
                    None,
                    identity,
                ) {
                    Ok(transformed) => transformed,
                    Err(()) => return Some(Err(())),
                };
                Expr::Closure(parameters.clone(), Box::new(transformed))
            } else {
                argument.value.clone()
            };
            transformed_clauses.push(CallArg {
                label: argument.label.clone(),
                value,
            });
        }
        let transformed_inner_callee =
            Expr::Call(Box::new((**handler_head).clone()), transformed_clauses);

        let wrap_in_unsafe = handler.lexical_unsafe_depth.get() > 0;
        let transformed_action = match self.transform_handler_expr(
            (**action_body).clone(),
            handler,
            resume,
            continuation,
        ) {
            Ok(transformed) => transformed,
            Err(()) => return Some(Err(())),
        };
        let call = Expr::Call(
            Box::new(transformed_inner_callee),
            vec![CallArg {
                label: None,
                value: Expr::Closure(Vec::new(), Box::new(transformed_action)),
            }],
        );
        Some(Ok(if wrap_in_unsafe {
            Expr::Unsafe(Box::new(call))
        } else {
            call
        }))
    }

    fn transform_handler_arguments(
        &mut self,
        mut remaining: Vec<CallArg>,
        completed_arguments: Vec<CallArg>,
        handler: Rc<AlgebraicHandler>,
        resume: Option<SourceResume>,
        completed: SourceArgumentsContinuation,
    ) -> Result<Expr, ()> {
        if remaining.is_empty() {
            return completed(self, completed_arguments);
        }
        let argument = remaining.remove(0);
        let label = argument.label.clone();
        let next_handler = handler.clone();
        let next_resume = resume.clone();
        self.transform_handler_expr(
            argument.value,
            handler,
            resume,
            Rc::new(move |analyzer, value| {
                let mut arguments = completed_arguments.clone();
                arguments.push(CallArg {
                    label: label.clone(),
                    value,
                });
                analyzer.transform_handler_arguments(
                    remaining.clone(),
                    arguments,
                    next_handler.clone(),
                    next_resume.clone(),
                    completed.clone(),
                )
            }),
        )
    }

    fn transform_handler_loop(
        &mut self,
        condition: Option<Expr>,
        mut body: Expr,
        handler: Rc<AlgebraicHandler>,
        resume: Option<SourceResume>,
        continuation: SourceContinuation,
    ) -> Result<Expr, ()> {
        let Some(result_source) = handler.result_source.clone() else {
            self.error("a handler containing a resumable loop requires a contextual result type");
            return Err(());
        };
        let specialization = self.next_closure;
        self.next_closure += 1;
        let recursive_name = format!("$handler$recursive$loop${specialization}");
        let break_name = format!("$handler$loop$break${specialization}");
        rewrite_handler_loop_control(&mut body, &recursive_name, &break_name, 0);
        let recursive_call =
            || Expr::Call(Box::new(Expr::Name(recursive_name.clone())), Vec::new());
        let break_call = |value| {
            Expr::Call(
                Box::new(Expr::Name(break_name.clone())),
                vec![CallArg { label: None, value }],
            )
        };
        let iteration = if let Some(condition) = condition {
            Expr::If {
                condition: Box::new(condition),
                then_branch: Box::new(Expr::Block(
                    vec![Stmt::Expr(body)],
                    Some(Box::new(recursive_call())),
                )),
                else_branch: Some(Box::new(break_call(Expr::Unit))),
            }
        } else {
            Expr::Block(vec![Stmt::Expr(body)], Some(Box::new(recursive_call())))
        };
        handler
            .loop_breaks
            .borrow_mut()
            .insert(break_name, continuation);
        let identity: SourceContinuation = Rc::new(|_, value| Ok(value));
        let transformed = self.transform_handler_expr(iteration, handler.clone(), resume, identity);
        handler
            .loop_breaks
            .borrow_mut()
            .remove(&format!("$handler$loop$break${specialization}"));
        let transformed = transformed?;
        let frame_name = format!("$handler$loop$frame${specialization}");
        let frame = Binding {
            mutable: true,
            name: frame_name.clone(),
            annotation: Some(Type::Function {
                groups: vec![Vec::new()],
                effects: FunctionEffects::default(),
                result: Box::new(result_source),
            }),
            value: Expr::Closure(Vec::new(), Box::new(transformed)),
        };
        Ok(Expr::Block(
            vec![Stmt::Let(frame)],
            Some(Box::new(Expr::Call(
                Box::new(Expr::Name(frame_name)),
                Vec::new(),
            ))),
        ))
    }

    fn transform_dynamic_callable_call(
        &mut self,
        expression: &Expr,
        handler: Rc<AlgebraicHandler>,
        resume: Option<SourceResume>,
        continuation: SourceContinuation,
    ) -> Option<Result<Expr, ()>> {
        let mut groups = Vec::new();
        let Expr::Name(name) = flatten_call(expression, &mut groups) else {
            return None;
        };
        let callable = handler.dynamic_callables.borrow().get(name).cloned()?;
        if groups.len() != callable.group_lengths.len()
            || groups
                .iter()
                .zip(&callable.group_lengths)
                .any(|(arguments, expected)| arguments.len() != *expected)
        {
            self.error(format!(
                "dynamic effectful callable `{name}` must be fully applied under its handler"
            ));
            return Some(Err(()));
        }
        if groups
            .iter()
            .flat_map(|group| group.iter())
            .any(|argument| self.handler_expression_may_suspend(&argument.value, &handler))
        {
            let group_lengths = callable.group_lengths.clone();
            let arguments = groups
                .iter()
                .flat_map(|group| group.iter().cloned())
                .collect::<Vec<_>>();
            let callee = name.clone();
            let next_handler = handler.clone();
            let next_resume = resume.clone();
            let next_continuation = continuation.clone();
            let completed: SourceArgumentsContinuation = Rc::new(move |analyzer, arguments| {
                let mut offset = 0;
                let mut call = Expr::Name(callee.clone());
                for length in &group_lengths {
                    let end = offset + length;
                    call = Expr::Call(Box::new(call), arguments[offset..end].to_vec());
                    offset = end;
                }
                analyzer
                    .transform_dynamic_callable_call(
                        &call,
                        next_handler.clone(),
                        next_resume.clone(),
                        next_continuation.clone(),
                    )
                    .unwrap_or_else(|| {
                        analyzer.error(
                            "internal handler lost its dynamic callable after argument lowering",
                        );
                        Err(())
                    })
            });
            return Some(self.transform_handler_arguments(
                arguments,
                Vec::new(),
                handler,
                resume,
                completed,
            ));
        }
        let rebuild = |target: &str| {
            let mut call = Expr::Name(target.to_owned());
            for group in &groups {
                call = Expr::Call(Box::new(call), group.to_vec());
            }
            call
        };
        let mut branches = Vec::with_capacity(callable.targets.len());
        for target in &callable.targets {
            match self.transform_handler_expr(
                rebuild(target),
                handler.clone(),
                resume.clone(),
                continuation.clone(),
            ) {
                Ok(branch) => branches.push(branch),
                Err(()) => return Some(Err(())),
            }
        }
        let Some(mut dispatch) = branches.pop() else {
            self.error("internal dynamic callable has no dispatch targets");
            return Some(Err(()));
        };
        for (index, branch) in branches.into_iter().enumerate().rev() {
            dispatch = Expr::If {
                condition: Box::new(Expr::Binary(
                    Box::new(Expr::Name(name.clone())),
                    BinaryOp::Eq,
                    Box::new(Expr::Integer(index as i128)),
                )),
                then_branch: Box::new(branch),
                else_branch: Some(Box::new(dispatch)),
            };
        }
        Some(Ok(dispatch))
    }

    fn transform_resumable_closure_call(
        &mut self,
        expression: &Expr,
        handler: Rc<AlgebraicHandler>,
        resume: Option<SourceResume>,
        continuation: SourceContinuation,
    ) -> Option<Result<Expr, ()>> {
        let mut groups = Vec::new();
        let Expr::Name(name) = flatten_call(expression, &mut groups) else {
            return None;
        };
        let closure = handler.resumable_closures.borrow().get(name).cloned()?;
        if groups.len() != closure.group_lengths.len()
            || groups
                .iter()
                .zip(&closure.group_lengths)
                .any(|(arguments, expected)| arguments.len() != *expected)
        {
            self.error(format!(
                "resumable closure `{name}` must be fully applied before it can run under a handler"
            ));
            return Some(Err(()));
        }
        if groups
            .iter()
            .flat_map(|group| group.iter())
            .any(|argument| self.handler_expression_may_suspend(&argument.value, &handler))
        {
            let group_lengths = closure.group_lengths.clone();
            let arguments = groups
                .iter()
                .flat_map(|group| group.iter().cloned())
                .collect::<Vec<_>>();
            let callee = name.clone();
            let next_handler = handler.clone();
            let next_resume = resume.clone();
            let next_continuation = continuation.clone();
            let completed: SourceArgumentsContinuation = Rc::new(move |analyzer, arguments| {
                let mut offset = 0;
                let mut call = Expr::Name(callee.clone());
                for length in &group_lengths {
                    let end = offset + length;
                    call = Expr::Call(Box::new(call), arguments[offset..end].to_vec());
                    offset = end;
                }
                analyzer
                    .transform_resumable_closure_call(
                        &call,
                        next_handler.clone(),
                        next_resume.clone(),
                        next_continuation.clone(),
                    )
                    .unwrap_or_else(|| {
                        analyzer.error(
                            "internal handler lost its resumable closure after argument lowering",
                        );
                        Err(())
                    })
            });
            return Some(self.transform_handler_arguments(
                arguments,
                Vec::new(),
                handler,
                resume,
                completed,
            ));
        }

        let specialization = self.next_closure;
        self.next_closure += 1;
        let value_name = format!("$handler$closure$continuation$value${specialization}");
        let continuation_name = format!("$handler$closure$continuation${specialization}");
        let continuation_body = match continuation(self, Expr::Name(value_name.clone())) {
            Ok(body) => body,
            Err(()) => return Some(Err(())),
        };
        let continuation_binding = Binding {
            mutable: true,
            name: continuation_name.clone(),
            annotation: Some(Type::Function {
                groups: vec![vec![closure.input.clone()]],
                effects: FunctionEffects::default(),
                result: Box::new(closure.answer.clone()),
            }),
            value: Expr::Closure(
                vec![Param {
                    mode: PassMode::Inferred,
                    access: None,
                    passing: None,
                    region: None,
                    name: value_name,
                    ty: closure.input.clone(),
                }],
                Box::new(continuation_body),
            ),
        };
        let erased_name = format!("$handler$erased$closure$continuation${specialization}");
        let erased_binding = Binding {
            mutable: true,
            name: erased_name.clone(),
            annotation: Some(Type::Named(
                self.lang_item_name(LangItemKind::Continuation).to_owned(),
                vec![closure.input, closure.answer],
            )),
            value: Expr::Call(
                Box::new(Expr::Name("$handler$erase$continuation".to_owned())),
                vec![CallArg {
                    label: None,
                    value: Expr::Name(continuation_name),
                }],
            ),
        };
        let mut call = Expr::Name(name.clone());
        for (index, group) in groups.iter().enumerate() {
            let mut arguments = group.to_vec();
            if index + 1 == groups.len() {
                arguments.push(CallArg {
                    label: None,
                    value: Expr::Name(erased_name.clone()),
                });
            }
            call = Expr::Call(Box::new(call), arguments);
        }
        Some(Ok(Expr::Block(
            vec![Stmt::Let(continuation_binding), Stmt::Let(erased_binding)],
            Some(Box::new(call)),
        )))
    }

    fn transform_resumable_closure_binding(
        &mut self,
        binding: &Binding,
        handler: Rc<AlgebraicHandler>,
    ) -> Option<Result<(Binding, SourceResumableClosure), ()>> {
        let Some(Type::Function {
            groups,
            effects,
            result,
        }) = binding.annotation.as_ref()
        else {
            return None;
        };
        if !effects
            .custom
            .iter()
            .any(|effect| source_effect_identity(effect) == handler.identity)
        {
            return None;
        }
        if !matches!(binding.value, Expr::Closure(_, _)) {
            return None;
        }
        let Some(answer) = handler.result_source.clone() else {
            self.error("a resumable closure requires a contextual handler answer type");
            return Some(Err(()));
        };
        let input = logical_effect_result_source(result, effects);
        let specialization = self.next_closure;
        self.next_closure += 1;
        let continuation_name = format!("$handler$closure$frame$continuation${specialization}");
        let continuation_ty = Type::Named(
            self.lang_item_name(LangItemKind::Continuation).to_owned(),
            vec![input.clone(), answer.clone()],
        );
        let mut value = binding.value.clone();
        let Some(body) = append_innermost_closure_parameter(
            &mut value,
            Param {
                mode: PassMode::Move,
                access: None,
                passing: None,
                region: None,
                name: continuation_name.clone(),
                ty: continuation_ty.clone(),
            },
        ) else {
            self.error("internal resumable closure binding lost its closure value");
            return Some(Err(()));
        };
        let tail_continuation_name = continuation_name.clone();
        let tail: SourceContinuation = Rc::new(move |_, value| {
            Ok(Expr::Call(
                Box::new(Expr::Name("$handler$invoke$continuation".to_owned())),
                vec![
                    CallArg {
                        label: None,
                        value: Expr::Name(tail_continuation_name.clone()),
                    },
                    CallArg { label: None, value },
                ],
            ))
        });
        let return_name = format!("$handler$closure$return${specialization}");
        rewrite_handler_returns(body, &return_name);
        handler
            .return_continuations
            .borrow_mut()
            .insert(return_name.clone(), tail.clone());
        let transformed = self.transform_handler_expr(body.clone(), handler.clone(), None, tail);
        handler
            .return_continuations
            .borrow_mut()
            .remove(&return_name);
        let transformed = match transformed {
            Ok(transformed) => transformed,
            Err(()) => return Some(Err(())),
        };
        *body = transformed;

        let mut rewritten_groups = groups.clone();
        let Some(last_group) = rewritten_groups.last_mut() else {
            self.error("a resumable closure type requires a runtime parameter group");
            return Some(Err(()));
        };
        last_group.push(continuation_ty);
        let mut rewritten_effects = effects.clone();
        rewritten_effects
            .custom
            .retain(|effect| source_effect_identity(effect) != handler.identity);
        if handler.lexical_unsafe_depth.get() > 0 {
            self.strip_authorized_unsafe_effects(&mut rewritten_effects);
        }
        let rewritten_result = self.effect_abi_result_source(answer.clone(), &rewritten_effects);
        let rewritten = Binding {
            mutable: binding.mutable,
            name: binding.name.clone(),
            annotation: Some(Type::Function {
                groups: rewritten_groups,
                effects: rewritten_effects,
                result: Box::new(rewritten_result),
            }),
            value,
        };
        Some(Ok((
            rewritten,
            SourceResumableClosure {
                input,
                answer,
                group_lengths: groups.iter().map(Vec::len).collect(),
            },
        )))
    }

    fn explicit_generic_handler_function(
        &mut self,
        name: &str,
        groups: &[&[CallArg]],
        handled_effect: &Type,
    ) -> Option<Result<(String, Function, usize), ()>> {
        let template = self.function_templates.get(name)?.clone();
        if groups.len() > template.compile_groups.len() + template.groups.len() {
            return None;
        }
        let inference_context = LowerCtx::for_global(ItemOrigin::default());
        let (compile_parameters, mut inferred, runtime_group_start) = match self
            .seed_type_argument_inference(
                name,
                &template.compile_groups,
                groups,
                &inference_context,
                false,
            ) {
            Some(inferred) => inferred,
            None => return Some(Err(())),
        };
        let mut matched_effect = false;
        for effect in &template.effects.custom {
            let mut effect = effect.clone();
            let substitutions = inferred
                .iter()
                .filter_map(|(name, inferred)| {
                    inferred
                        .source
                        .clone()
                        .or_else(|| self.source_type_for_ty(&inferred.ty))
                        .map(|source| (name.clone(), source))
                })
                .collect::<HashMap<_, _>>();
            substitute_type_parameters(&mut effect, &substitutions);
            let mut candidate = inferred.clone();
            if self
                .unify_source_template(
                    &effect,
                    handled_effect,
                    &compile_parameters,
                    &mut candidate,
                    "handled effect",
                )
                .is_ok()
            {
                inferred = candidate;
                matched_effect = true;
                break;
            }
        }
        if !matched_effect {
            return None;
        }
        let runtime_groups = &groups[runtime_group_start..];
        if runtime_groups.len() > template.groups.len() {
            return None;
        }
        let mut ordered_runtime_groups = Vec::new();
        for (group_index, (arguments, parameters)) in
            runtime_groups.iter().zip(&template.groups).enumerate()
        {
            let parameter_names = parameters
                .iter()
                .map(|parameter| parameter.name.clone())
                .collect::<Vec<_>>();
            let Some(ordered) =
                self.ordered_call_arguments(name, group_index + 1, arguments, &parameter_names)
            else {
                return Some(Err(()));
            };
            ordered_runtime_groups.push(ordered);
        }
        let constraints = ordered_runtime_groups
            .iter()
            .zip(&template.groups)
            .enumerate()
            .flat_map(|(group_index, (arguments, parameters))| {
                arguments
                    .iter()
                    .zip(parameters)
                    .map(move |(argument, parameter)| {
                        (
                            parameter.ty.clone(),
                            argument.value.clone(),
                            format!(
                                "argument for parameter `{}` in group {}",
                                parameter.name,
                                group_index + 1
                            ),
                        )
                    })
            })
            .collect::<Vec<_>>();
        let unsupported = match self.infer_from_expression_constraints(
            &constraints,
            &compile_parameters,
            &mut inferred,
            &inference_context,
        ) {
            Some(unsupported) => unsupported,
            None => return Some(Err(())),
        };
        let ordered_parameters = template
            .compile_groups
            .iter()
            .flatten()
            .cloned()
            .collect::<Vec<_>>();
        let (source_arguments, arguments) = match self.finish_type_argument_inference(
            name,
            &ordered_parameters,
            &inferred,
            unsupported,
        ) {
            Some(arguments) => arguments,
            None => return Some(Err(())),
        };
        let canonical = match self.ensure_function_instance(name, source_arguments, arguments) {
            Some(canonical) => canonical,
            None => return Some(Err(())),
        };
        let function = self
            .functions
            .get(&canonical)
            .cloned()
            .expect("created generic function instance is registered");
        Some(Ok((canonical, function, runtime_group_start)))
    }

    fn transform_effectful_named_call(
        &mut self,
        expression: &Expr,
        handler: Rc<AlgebraicHandler>,
        resume: Option<SourceResume>,
        continuation: SourceContinuation,
    ) -> Option<Result<Expr, ()>> {
        let mut groups = Vec::new();
        let Expr::Name(source_name) = flatten_call(expression, &mut groups) else {
            return None;
        };
        let original_groups = groups.clone();
        let aliased_name = handler.function_aliases.borrow().get(source_name).cloned();
        let source_name = source_name.clone();
        let selected_name = aliased_name.as_deref().unwrap_or(&source_name);
        let (name, function, runtime_group_start) =
            if let Some(function) = self.functions.get(selected_name).cloned() {
                (selected_name.to_owned(), function, 0)
            } else if let Some(resolved) =
                self.explicit_generic_handler_function(selected_name, &groups, &handler.source)
            {
                match resolved {
                    Ok(resolved) => resolved,
                    Err(()) => return Some(Err(())),
                }
            } else {
                return None;
            };
        groups = groups[runtime_group_start..].to_vec();
        if !function
            .effects
            .custom
            .iter()
            .any(|effect| source_effect_identity(effect) == handler.identity)
        {
            return None;
        }
        if groups
            .iter()
            .flat_map(|group| group.iter())
            .any(|argument| self.handler_expression_may_suspend(&argument.value, &handler))
        {
            let compile_prefix = original_groups[..runtime_group_start]
                .iter()
                .map(|group| group.to_vec())
                .collect::<Vec<_>>();
            let group_lengths = groups.iter().map(|group| group.len()).collect::<Vec<_>>();
            let arguments = groups
                .iter()
                .flat_map(|group| group.iter().cloned())
                .collect::<Vec<_>>();
            let callee = source_name;
            let next_handler = handler.clone();
            let next_resume = resume.clone();
            let next_continuation = continuation.clone();
            let completed: SourceArgumentsContinuation = Rc::new(move |analyzer, arguments| {
                let mut offset = 0;
                let mut call = Expr::Name(callee.clone());
                for group in &compile_prefix {
                    call = Expr::Call(Box::new(call), group.clone());
                }
                for length in &group_lengths {
                    let end = offset + length;
                    call = Expr::Call(Box::new(call), arguments[offset..end].to_vec());
                    offset = end;
                }
                analyzer
                    .transform_effectful_named_call(
                        &call,
                        next_handler.clone(),
                        next_resume.clone(),
                        next_continuation.clone(),
                    )
                    .unwrap_or_else(|| {
                        analyzer.error(
                            "internal handler call lost its effectful target after argument lowering",
                        );
                        Err(())
                    })
            });
            return Some(self.transform_handler_arguments(
                arguments,
                Vec::new(),
                handler,
                resume,
                completed,
            ));
        }
        if let Some(frame) = handler.inlining.borrow().get(&name).cloned() {
            let specialization = self.next_closure;
            self.next_closure += 1;
            let value_name = format!("$handler$recursive$continuation$value${specialization}");
            let continuation_name = format!("$handler$recursive$continuation${specialization}");
            let erased_name = format!("$handler$erased$recursive$continuation${specialization}");
            let continuation_body = match continuation(self, Expr::Name(value_name.clone())) {
                Ok(body) => body,
                Err(()) => return Some(Err(())),
            };
            let continuation_binding = Binding {
                mutable: true,
                name: continuation_name.clone(),
                annotation: Some(Type::Function {
                    groups: vec![vec![frame.input.clone()]],
                    effects: FunctionEffects::default(),
                    result: Box::new(frame.answer.clone()),
                }),
                value: Expr::Closure(
                    vec![Param {
                        mode: PassMode::Inferred,
                        access: None,
                        passing: None,
                        region: None,
                        name: value_name,
                        ty: frame.input.clone(),
                    }],
                    Box::new(continuation_body),
                ),
            };
            let erased_binding = Binding {
                mutable: true,
                name: erased_name.clone(),
                annotation: Some(Type::Named(
                    self.lang_item_name(LangItemKind::Continuation).to_owned(),
                    vec![frame.input, frame.answer],
                )),
                value: Expr::Call(
                    Box::new(Expr::Name("$handler$erase$continuation".to_owned())),
                    vec![CallArg {
                        label: None,
                        value: Expr::Name(continuation_name),
                    }],
                ),
            };
            let mut recursive_arguments = groups
                .iter()
                .flat_map(|group| group.iter())
                .map(|argument| CallArg {
                    label: None,
                    value: argument.value.clone(),
                })
                .collect::<Vec<_>>();
            recursive_arguments.push(CallArg {
                label: None,
                value: Expr::Name(erased_name),
            });
            let recursive_call = Expr::Call(
                Box::new(Expr::Name(frame.recursive_name)),
                recursive_arguments,
            );
            return Some(Ok(Expr::Block(
                vec![Stmt::Let(continuation_binding), Stmt::Let(erased_binding)],
                Some(Box::new(recursive_call)),
            )));
        }
        let Some(_) = function.body else {
            self.error(format!(
                "effectful function `{name}` has no source body available for handler lowering"
            ));
            return Some(Err(()));
        };
        if groups.len() != function.groups.len()
            || groups
                .iter()
                .zip(&function.groups)
                .any(|(arguments, parameters)| arguments.len() != parameters.len())
        {
            self.error(format!(
                "effectful function `{name}` must be fully applied before it can run under a handler"
            ));
            return Some(Err(()));
        }
        let specialization = self.next_closure;
        let recursive_name = format!("$handler$recursive${specialization}");
        let prefix = format!("$handler$frame${specialization}${name}$");
        self.next_closure += 1;
        let (parameters, mut body) = hygienic_inline_function(&function, &prefix);
        let mut parameter_types = parameters
            .iter()
            .flatten()
            .map(|parameter| (parameter.name.clone(), parameter.ty.clone()))
            .collect::<HashMap<_, _>>();
        visit_expr_mut(&mut body, &mut |expression| match expression {
            Expr::Block(statements, _) => {
                for statement in statements {
                    let Stmt::Let(binding) = statement else {
                        continue;
                    };
                    if let Some(annotation) = &binding.annotation {
                        parameter_types.insert(binding.name.clone(), annotation.clone());
                    }
                }
            }
            Expr::Closure(parameters, _) => {
                for parameter in parameters {
                    parameter_types.insert(parameter.name.clone(), parameter.ty.clone());
                }
            }
            _ => {}
        });
        let origin = self
            .function_origins
            .get(&name)
            .cloned()
            .unwrap_or_default();
        let raise_trait = self.lang_item_name(LangItemKind::Raise).to_owned();
        visit_expr_mut(&mut body, &mut |expression| {
            let replacement = match expression {
                Expr::Call(callee, arguments) if arguments.is_empty() => {
                    let Expr::Member(receiver, member) = callee.as_ref() else {
                        return;
                    };
                    let (member, forced_trait) = if member == "$lang$raise" {
                        ("raise", Some(raise_trait.as_str()))
                    } else if member.starts_with("$lang$") {
                        return;
                    } else {
                        (member.as_str(), None)
                    };
                    let Expr::Name(receiver_name) = receiver.as_ref() else {
                        return;
                    };
                    let Some(source_ty) = parameter_types.get(receiver_name) else {
                        return;
                    };
                    let receiver_ty = self.lower_source_type(source_ty);
                    let mut candidates =
                        self.trait_method_function_candidates(&receiver_ty, member, &origin);
                    candidates.retain(|(key, canonical)| {
                        forced_trait.is_none_or(|name| key.trait_ref.name == name)
                            && self
                                .functions
                                .get(canonical)
                                .or_else(|| self.function_templates.get(canonical))
                                .is_some_and(|function| {
                                    function.effects.custom.iter().any(|effect| {
                                        source_effect_identity(effect) == handler.identity
                                    })
                                })
                    });
                    let [(_, canonical)] = candidates.as_slice() else {
                        return;
                    };
                    Some(Expr::Call(
                        Box::new(Expr::Name(canonical.clone())),
                        vec![CallArg {
                            label: None,
                            value: (**receiver).clone(),
                        }],
                    ))
                }
                _ => None,
            };
            if let Some(replacement) = replacement {
                *expression = replacement;
            }
        });
        let mut source_arguments = Vec::new();
        for (arguments, declared) in groups.iter().zip(&function.groups) {
            for (argument, declared) in arguments.iter().zip(declared) {
                if argument
                    .label
                    .as_deref()
                    .is_some_and(|label| label != declared.name)
                {
                    self.error(format!(
                        "unknown argument label on effectful call `{name}`: expected `{}`",
                        declared.name
                    ));
                    return Some(Err(()));
                }
                source_arguments.push(CallArg {
                    label: None,
                    value: argument.value.clone(),
                });
            }
        }
        let mut omitted_parameters = HashSet::new();
        let mut static_function_values = HashMap::new();
        for (index, (parameter, argument)) in parameters
            .iter()
            .flatten()
            .zip(&source_arguments)
            .enumerate()
        {
            if !matches!(parameter.ty, Type::Function { .. }) {
                continue;
            }
            let Expr::Name(argument_name) = &argument.value else {
                continue;
            };
            let target = handler
                .function_aliases
                .borrow()
                .get(argument_name.as_str())
                .cloned()
                .unwrap_or_else(|| argument_name.clone());
            if self.functions.contains_key(&target)
                || handler.resumable_closures.borrow().contains_key(&target)
                || handler.dynamic_callables.borrow().contains_key(&target)
            {
                omitted_parameters.insert(index);
                static_function_values.insert(parameter.name.clone(), target);
            }
        }
        if !static_function_values.is_empty() {
            rewrite_static_function_values(&mut body, &static_function_values);
        }
        if let Some(parameter_index) =
            parameters
                .iter()
                .flatten()
                .enumerate()
                .find_map(|(index, parameter)| {
                    if omitted_parameters.contains(&index) {
                        return None;
                    }
                    let Type::Function { effects, .. } = &parameter.ty else {
                        return None;
                    };
                    effects
                        .custom
                        .iter()
                        .any(|effect| source_effect_identity(effect) == handler.identity)
                        .then_some(index)
                })
        {
            let parameter = function
                .groups
                .iter()
                .flatten()
                .nth(parameter_index)
                .expect("hygienic and source parameter lists have identical shapes");
            self.error(format!(
                "dynamic effectful callable parameter `{}` requires the handler-aware runtime ABI",
                parameter.name
            ));
            return Some(Err(()));
        }
        if let Some(alias) = source_arguments
            .iter()
            .enumerate()
            .filter(|(index, _)| !omitted_parameters.contains(index))
            .find_map(|(_, argument)| {
                handler_alias_reference(&argument.value, &handler.function_aliases.borrow())
            })
        {
            self.error(format!(
                "effectful function alias `{alias}` cannot escape its handler or be used as a runtime value"
            ));
            return Some(Err(()));
        }
        let Some(input) = logical_function_result_source(&function) else {
            self.error(format!(
                "resumable function `{name}` requires an explicit return type"
            ));
            handler.inlining.borrow_mut().remove(&name);
            return Some(Err(()));
        };
        let Some(answer) = handler.result_source.clone() else {
            self.error("a resumable named call requires a contextual handler answer type");
            handler.inlining.borrow_mut().remove(&name);
            return Some(Err(()));
        };
        let continuation_name = format!("$handler$call$continuation${specialization}");
        let continuation_value_name = format!("$handler$call$continuation$value${specialization}");
        let continuation_body =
            match continuation(self, Expr::Name(continuation_value_name.clone())) {
                Ok(body) => body,
                Err(()) => return Some(Err(())),
            };
        handler.inlining.borrow_mut().insert(
            name.to_owned(),
            SourceInlineFrame {
                recursive_name,
                input: input.clone(),
                answer: answer.clone(),
            },
        );
        let continuation_binding = Binding {
            mutable: true,
            name: continuation_name.clone(),
            annotation: Some(Type::Function {
                groups: vec![vec![input.clone()]],
                effects: FunctionEffects::default(),
                result: Box::new(answer.clone()),
            }),
            value: Expr::Closure(
                vec![Param {
                    mode: PassMode::Inferred,
                    access: None,
                    passing: None,
                    region: None,
                    name: continuation_value_name,
                    ty: input.clone(),
                }],
                Box::new(continuation_body),
            ),
        };
        let erased_continuation_name =
            format!("$handler$erased$call$continuation${specialization}");
        let erased_continuation_binding = Binding {
            mutable: true,
            name: erased_continuation_name.clone(),
            annotation: Some(Type::Named(
                self.lang_item_name(LangItemKind::Continuation).to_owned(),
                vec![input.clone(), answer.clone()],
            )),
            value: Expr::Call(
                Box::new(Expr::Name("$handler$erase$continuation".to_owned())),
                vec![CallArg {
                    label: None,
                    value: Expr::Name(continuation_name.clone()),
                }],
            ),
        };
        let frame_continuation_name = format!("$handler$frame$continuation${specialization}");
        let tail_name = format!("$handler$tail${specialization}");
        let continuation_for_return = frame_continuation_name.clone();
        let tail_continuation: SourceContinuation = Rc::new(move |_, value| {
            Ok(Expr::Call(
                Box::new(Expr::Name(tail_name.clone())),
                vec![CallArg {
                    label: None,
                    value: Expr::Call(
                        Box::new(Expr::Name("$handler$invoke$continuation".to_owned())),
                        vec![
                            CallArg {
                                label: None,
                                value: Expr::Name(continuation_for_return.clone()),
                            },
                            CallArg { label: None, value },
                        ],
                    ),
                }],
            ))
        });
        let return_name = format!("$handler$return${specialization}");
        rewrite_handler_returns(&mut body, &return_name);
        handler
            .return_continuations
            .borrow_mut()
            .insert(return_name.clone(), tail_continuation.clone());
        let transformed_body = match self.transform_handler_expr(
            body,
            handler.clone(),
            resume.clone(),
            tail_continuation,
        ) {
            Ok(body) => body,
            Err(()) => {
                handler
                    .return_continuations
                    .borrow_mut()
                    .remove(&return_name);
                handler.inlining.borrow_mut().remove(&name);
                return Some(Err(()));
            }
        };
        handler
            .return_continuations
            .borrow_mut()
            .remove(&return_name);

        let mut flattened_parameters = parameters
            .iter()
            .flatten()
            .cloned()
            .enumerate()
            .filter_map(|(index, parameter)| {
                (!omitted_parameters.contains(&index)).then_some(parameter)
            })
            .collect::<Vec<_>>();
        flattened_parameters.push(Param {
            mode: PassMode::Move,
            access: None,
            passing: None,
            region: None,
            name: frame_continuation_name,
            ty: Type::Named(
                self.lang_item_name(LangItemKind::Continuation).to_owned(),
                vec![input.clone(), answer.clone()],
            ),
        });
        let mut flattened_arguments = source_arguments
            .into_iter()
            .enumerate()
            .filter_map(|(index, argument)| {
                (!omitted_parameters.contains(&index)).then_some(argument)
            })
            .collect::<Vec<_>>();
        flattened_arguments.push(CallArg {
            label: None,
            value: Expr::Name(erased_continuation_name),
        });
        let frame_name = format!("$handler$frame${specialization}");
        self.handler_frame_parameter_modes.insert(
            frame_name.clone(),
            flattened_parameters
                .iter()
                .map(|parameter| {
                    source_borrow_channel_mode(parameter.mode, &parameter.ty)
                        .unwrap_or(parameter.mode)
                })
                .collect(),
        );
        let mut frame_effects = function.effects.clone();
        frame_effects
            .custom
            .retain(|effect| source_effect_identity(effect) != handler.identity);
        if handler.lexical_unsafe_depth.get() > 0 {
            self.strip_authorized_unsafe_effects(&mut frame_effects);
        }
        let frame_result = self.effect_abi_result_source(answer, &frame_effects);
        let frame_annotation = Some(Type::Function {
            groups: vec![flattened_parameters
                .iter()
                .map(|parameter| parameter.ty.clone())
                .collect()],
            effects: frame_effects,
            result: Box::new(frame_result),
        });
        let frame = Binding {
            mutable: true,
            name: frame_name.clone(),
            annotation: frame_annotation,
            value: Expr::Closure(flattened_parameters, Box::new(transformed_body)),
        };
        let call = Expr::Call(Box::new(Expr::Name(frame_name)), flattened_arguments);
        let result = Ok(Expr::Block(
            vec![
                Stmt::Let(continuation_binding),
                Stmt::Let(erased_continuation_binding),
                Stmt::Let(frame),
            ],
            Some(Box::new(call)),
        ));
        handler.inlining.borrow_mut().remove(&name);
        Some(result)
    }

    fn transform_handler_block(
        &mut self,
        mut statements: Vec<Stmt>,
        tail: Option<Expr>,
        handler: Rc<AlgebraicHandler>,
        resume: Option<SourceResume>,
        continuation: SourceContinuation,
    ) -> Result<Expr, ()> {
        if statements.is_empty() {
            return self.transform_handler_expr(
                tail.unwrap_or(Expr::Unit),
                handler,
                resume,
                continuation,
            );
        }
        let first = statements.remove(0);
        match first {
            Stmt::Let(mut binding) => {
                if let Some(Type::Function {
                    groups: callable_groups,
                    effects,
                    ..
                }) = binding.annotation.as_ref()
                {
                    if effects
                        .custom
                        .iter()
                        .any(|effect| source_effect_identity(effect) == handler.identity)
                        && matches!(binding.value, Expr::If { .. })
                    {
                        let mut targets = Vec::new();
                        if let Some(selection) =
                            static_callable_selection(&binding.value, &mut targets)
                        {
                            let group_lengths =
                                callable_groups.iter().map(Vec::len).collect::<Vec<_>>();
                            let mut union = Vec::new();
                            let mut sources = Vec::new();
                            let mut tag_bindings = Vec::new();
                            let mut valid = true;
                            for (index, target) in targets.into_iter().enumerate() {
                                if let Some(dynamic) =
                                    handler.dynamic_callables.borrow().get(&target).cloned()
                                {
                                    if dynamic.group_lengths != group_lengths {
                                        valid = false;
                                        break;
                                    }
                                    let hidden = format!(
                                        "$handler$dynamic$tag${}${index}",
                                        self.next_closure
                                    );
                                    tag_bindings.push(Stmt::Let(Binding {
                                        mutable: false,
                                        name: hidden.clone(),
                                        annotation: Some(Type::I32),
                                        value: Expr::Name(target),
                                    }));
                                    for candidate in &dynamic.targets {
                                        if !union.contains(candidate) {
                                            union.push(candidate.clone());
                                        }
                                    }
                                    sources.push((hidden, dynamic.targets));
                                    continue;
                                }
                                let resolved = handler
                                    .function_aliases
                                    .borrow()
                                    .get(&target)
                                    .cloned()
                                    .unwrap_or(target);
                                if !(self.functions.contains_key(&resolved)
                                    || handler.resumable_closures.borrow().contains_key(&resolved))
                                {
                                    valid = false;
                                    break;
                                }
                                if !union.contains(&resolved) {
                                    union.push(resolved.clone());
                                }
                                sources.push((String::new(), vec![resolved]));
                            }
                            if valid && union.len() >= 2 {
                                let selection =
                                    expand_dynamic_callable_selection(selection, &sources, &union);
                                let name = binding.name.clone();
                                let callable = SourceDynamicCallable {
                                    targets: union,
                                    group_lengths,
                                };
                                let next_handler = handler.clone();
                                let next_resume = resume.clone();
                                let next_continuation = continuation.clone();
                                let transformed = self.transform_handler_expr(
                                    selection,
                                    handler,
                                    resume,
                                    Rc::new(move |analyzer, selection| {
                                        let previous = next_handler
                                            .dynamic_callables
                                            .borrow_mut()
                                            .insert(name.clone(), callable.clone());
                                        let rest = analyzer.transform_handler_block(
                                            statements.clone(),
                                            tail.clone(),
                                            next_handler.clone(),
                                            next_resume.clone(),
                                            next_continuation.clone(),
                                        );
                                        let mut callables =
                                            next_handler.dynamic_callables.borrow_mut();
                                        if let Some(previous) = previous {
                                            callables.insert(name.clone(), previous);
                                        } else {
                                            callables.remove(&name);
                                        }
                                        drop(callables);
                                        let rest = rest?;
                                        Ok(Expr::Block(
                                            vec![Stmt::Let(Binding {
                                                mutable: false,
                                                name: name.clone(),
                                                annotation: Some(Type::I32),
                                                value: selection,
                                            })],
                                            Some(Box::new(rest)),
                                        ))
                                    }),
                                );
                                return transformed.map(|transformed| {
                                    Expr::Block(tag_bindings, Some(Box::new(transformed)))
                                });
                            }
                        }
                    }
                }
                if let Some(transformed) =
                    self.transform_resumable_closure_binding(&binding, handler.clone())
                {
                    let (binding, closure) = transformed?;
                    let name = binding.name.clone();
                    let previous = handler
                        .resumable_closures
                        .borrow_mut()
                        .insert(name.clone(), closure);
                    let rest = self.transform_handler_block(
                        statements,
                        tail,
                        handler.clone(),
                        resume,
                        continuation,
                    );
                    let mut closures = handler.resumable_closures.borrow_mut();
                    if let Some(previous) = previous {
                        closures.insert(name, previous);
                    } else {
                        closures.remove(&name);
                    }
                    drop(closures);
                    return rest
                        .map(|rest| Expr::Block(vec![Stmt::Let(binding)], Some(Box::new(rest))));
                }
                if let Expr::Name(target) = &binding.value {
                    let dynamic = handler.dynamic_callables.borrow().get(target).cloned();
                    if let Some(dynamic) = dynamic {
                        let name = binding.name.clone();
                        binding.annotation = Some(Type::I32);
                        let previous = handler
                            .dynamic_callables
                            .borrow_mut()
                            .insert(name.clone(), dynamic);
                        let rest = self.transform_handler_block(
                            statements,
                            tail,
                            handler.clone(),
                            resume,
                            continuation,
                        );
                        let mut callables = handler.dynamic_callables.borrow_mut();
                        if let Some(previous) = previous {
                            callables.insert(name, previous);
                        } else {
                            callables.remove(&name);
                        }
                        drop(callables);
                        return rest.map(|rest| {
                            Expr::Block(vec![Stmt::Let(binding)], Some(Box::new(rest)))
                        });
                    }
                }
                if let Expr::Name(target) = &binding.value {
                    let resolved_target = handler
                        .function_aliases
                        .borrow()
                        .get(target)
                        .cloned()
                        .unwrap_or_else(|| target.clone());
                    let aliases_handler_effect =
                        self.functions
                            .get(&resolved_target)
                            .is_some_and(|function| {
                                function.effects.custom.iter().any(|effect| {
                                    source_effect_identity(effect) == handler.identity
                                })
                            });
                    if aliases_handler_effect {
                        if binding.mutable || binding.annotation.is_some() {
                            self.error(format!(
                                "effectful function alias `{}` must be an inferred immutable binding",
                                binding.name
                            ));
                            return Err(());
                        }
                        let alias = binding.name.clone();
                        let previous = handler
                            .function_aliases
                            .borrow_mut()
                            .insert(alias.clone(), resolved_target);
                        let transformed = self.transform_handler_block(
                            statements,
                            tail,
                            handler.clone(),
                            resume,
                            continuation,
                        );
                        let mut aliases = handler.function_aliases.borrow_mut();
                        if let Some(previous) = previous {
                            aliases.insert(alias, previous);
                        } else {
                            aliases.remove(&alias);
                        }
                        return transformed;
                    }
                }
                if binding.name.starts_with("$handler$frame$")
                    || binding.name.starts_with("$handler$continuation$")
                    || binding.name.starts_with("$handler$call$continuation$")
                {
                    if let Expr::Closure(parameters, body) = binding.value {
                        let identity: SourceContinuation = Rc::new(|_, value| Ok(value));
                        let transformed = self.transform_handler_expr(
                            *body,
                            handler.clone(),
                            resume.clone(),
                            identity,
                        )?;
                        binding.value = Expr::Closure(parameters, Box::new(transformed));
                    }
                }
                let name = binding.name.clone();
                let annotation = binding.annotation.clone();
                let mutable = binding.mutable;
                let next_handler = handler.clone();
                let next_resume = resume.clone();
                self.transform_handler_expr(
                    binding.value,
                    handler,
                    resume,
                    Rc::new(move |analyzer, value| {
                        let rest = analyzer.transform_handler_block(
                            statements.clone(),
                            tail.clone(),
                            next_handler.clone(),
                            next_resume.clone(),
                            continuation.clone(),
                        )?;
                        Ok(Expr::Block(
                            vec![Stmt::Let(Binding {
                                mutable,
                                name: name.clone(),
                                annotation: annotation.clone(),
                                value,
                            })],
                            Some(Box::new(rest)),
                        ))
                    }),
                )
            }
            Stmt::Expr(statement) => {
                let next_handler = handler.clone();
                let next_resume = resume.clone();
                self.transform_handler_expr(
                    statement,
                    handler,
                    resume,
                    Rc::new(move |analyzer, value| {
                        let rest = analyzer.transform_handler_block(
                            statements.clone(),
                            tail.clone(),
                            next_handler.clone(),
                            next_resume.clone(),
                            continuation.clone(),
                        )?;
                        Ok(Expr::Block(vec![Stmt::Expr(value)], Some(Box::new(rest))))
                    }),
                )
            }
        }
    }
}

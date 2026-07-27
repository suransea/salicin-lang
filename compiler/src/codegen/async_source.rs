use crate::ast::{CallArg, Expr, Stmt};

use super::hir::Ty;

pub(super) struct AsyncSourcePlan {
    pub(super) factory_body: Expr,
    pub(super) has_await: bool,
    pub(super) continuation: Option<AsyncContinuationSource>,
    pub(super) retained: Vec<AsyncRetainedSource>,
    pub(super) loop_step: Option<AsyncLoopStepSource>,
    pub(super) loop_condition: Option<AsyncLoopConditionSource>,
}

pub(super) struct AsyncContinuationSource {
    pub(super) name: String,
    pub(super) mutable: bool,
    pub(super) body: Expr,
}

pub(super) struct AsyncRetainedSource {
    pub(super) name: String,
    pub(super) referent: Option<String>,
    pub(super) borrowed: bool,
}

pub(super) struct AsyncLoopStepSource {
    pub(super) binding: String,
    pub(super) break_value: Expr,
    pub(super) output_hint: Option<Ty>,
    pub(super) probe_awaits: Vec<(String, Expr)>,
    pub(super) carry_names: Vec<String>,
    pub(super) continue_constructor: String,
    pub(super) break_constructor: String,
}

pub(super) struct AsyncLoopConditionSource {
    pub(super) expression: Expr,
    pub(super) post_test: bool,
}

#[derive(Clone, Copy)]
pub(super) enum AsyncLoopKind {
    Loop,
    While,
    DoWhile,
}

impl AsyncLoopKind {
    pub(super) fn description(self) -> &'static str {
        match self {
            Self::Loop => "`loop`",
            Self::While => "pre-test `while`",
            Self::DoWhile => "post-test `while`",
        }
    }
}

pub(super) struct AsyncLoopSuspensionSource {
    pub(super) kind: AsyncLoopKind,
    pub(super) condition_suspends: bool,
    pub(super) body_suspends: bool,
    pub(super) has_continue: bool,
    pub(super) has_fallthrough: bool,
    pub(super) has_value_break: bool,
}

pub(super) fn recurring_suspended_loop_source(
    expression: &Expr,
) -> Option<AsyncLoopSuspensionSource> {
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

pub(super) fn async_loop_source(
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

pub(super) fn iteration_body_definitely_exits(expression: &Expr) -> bool {
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

pub(super) fn general_unit_recurring_loop_source(
    body: &Expr,
    id: usize,
) -> Option<AsyncSourcePlan> {
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

pub(super) fn append_async_iteration_fallthrough(expression: &mut Expr, continue_step: &Expr) {
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

pub(super) fn multiple_await_recurring_loop_source(
    body: &Expr,
    id: usize,
) -> Option<AsyncSourcePlan> {
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

pub(super) fn simple_recurring_async_loop_source(
    body: &Expr,
    id: usize,
) -> Option<AsyncSourcePlan> {
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

pub(super) enum SimpleLoopControl {
    Break(Expr),
    Continue,
    Fallthrough(Expr),
}

impl SimpleLoopControl {
    fn continues(&self) -> bool {
        matches!(self, Self::Continue | Self::Fallthrough(_))
    }
}

pub(super) fn simple_loop_decision(
    expression: &Expr,
) -> Option<(Expr, SimpleLoopControl, SimpleLoopControl)> {
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

pub(super) fn simple_loop_control(expression: &Expr) -> Option<SimpleLoopControl> {
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

pub(super) fn is_simple_loop_fallthrough(expression: &Expr) -> bool {
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

pub(super) fn rewrite_async_loop_continue_carry(
    expression: &mut Expr,
    constructor: &str,
    carry: &Expr,
) {
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

pub(super) fn split_async_source(body: &Expr) -> AsyncSourcePlan {
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

pub(super) fn hoist_control_await(expression: &Expr) -> Option<Expr> {
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

pub(super) fn terminating_loop_iteration(body: &Expr) -> Option<(Expr, Option<Expr>)> {
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

pub(super) fn branch_await_future(expression: &Expr) -> Option<Expr> {
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

pub(super) fn heterogeneous_branch_factory(
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

pub(super) fn wrap_heterogeneous_branch_factory(
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

pub(super) fn tail_await_operand(expression: &Expr) -> Option<Expr> {
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

pub(super) fn non_borrow_binding_names(statements: &[Stmt]) -> Vec<String> {
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

pub(super) fn borrowed_binding_names(statements: &[Stmt]) -> std::collections::HashSet<String> {
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

pub(super) fn referenced_names(expression: &Expr) -> std::collections::HashSet<String> {
    let mut expression = expression.clone();
    let mut names = std::collections::HashSet::new();
    super::source_rewrite::visit_expr_mut(&mut expression, &mut |expression| {
        if let Expr::Name(name) = expression.unlocated() {
            names.insert(name.clone());
        }
    });
    names
}

pub(super) fn async_place_root(expression: &Expr) -> Option<String> {
    match expression.unlocated() {
        Expr::Name(name) => Some(name.clone()),
        Expr::Member(base, _) | Expr::Index { base, .. } => async_place_root(base),
        _ => None,
    }
}

pub(super) fn async_initializer_root(expression: &Expr) -> Option<String> {
    match expression.unlocated() {
        Expr::Borrow { value, .. } => async_place_root(value),
        expression => async_place_root(expression),
    }
}

pub(super) fn resolve_async_dependency(name: &str, dependencies: &[(String, String)]) -> String {
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

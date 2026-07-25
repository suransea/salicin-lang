use crate::ast::{Binding, CallArg, Expr, Stmt};

use super::Analyzer;

impl Analyzer {
    pub(super) fn lower_lexical_defers(&mut self, expression: &Expr) -> Expr {
        self.rewrite_defer_scopes(expression.clone())
    }

    fn rewrite_defer_scopes(&mut self, expression: Expr) -> Expr {
        match expression {
            Expr::Located {
                line,
                column,
                end_line,
                end_column,
                value,
            } => Expr::Located {
                line,
                column,
                end_line,
                end_column,
                value: Box::new(self.rewrite_defer_scopes(*value)),
            },
            Expr::Block(statements, tail) => self.rewrite_defer_block(statements, tail),
            Expr::Tuple(values) => Expr::Tuple(
                values
                    .into_iter()
                    .map(|value| self.rewrite_defer_scopes(value))
                    .collect(),
            ),
            Expr::Array(values) => Expr::Array(
                values
                    .into_iter()
                    .map(|value| self.rewrite_defer_scopes(value))
                    .collect(),
            ),
            Expr::Unary(operator, value) => {
                Expr::Unary(operator, Box::new(self.rewrite_defer_scopes(*value)))
            }
            Expr::Borrow {
                mutable,
                access,
                value,
            } => Expr::Borrow {
                mutable,
                access,
                value: Box::new(self.rewrite_defer_scopes(*value)),
            },
            Expr::Binary(left, operator, right) => Expr::Binary(
                Box::new(self.rewrite_defer_scopes(*left)),
                operator,
                Box::new(self.rewrite_defer_scopes(*right)),
            ),
            Expr::Coalesce(left, right) => Expr::Coalesce(
                Box::new(self.rewrite_defer_scopes(*left)),
                Box::new(self.rewrite_defer_scopes(*right)),
            ),
            Expr::Try(value) => Expr::Try(Box::new(self.rewrite_defer_scopes(*value))),
            Expr::DoBlock { body } => Expr::DoBlock {
                body: Box::new(self.rewrite_defer_scopes(*body)),
            },
            Expr::Async { body } => Expr::Async {
                body: Box::new(self.rewrite_defer_scopes(*body)),
            },
            Expr::Await(value) => Expr::Await(Box::new(self.rewrite_defer_scopes(*value))),
            Expr::Throw(value) => Expr::Throw(Box::new(self.rewrite_defer_scopes(*value))),
            Expr::Assign(left, right) => Expr::Assign(
                Box::new(self.rewrite_defer_scopes(*left)),
                Box::new(self.rewrite_defer_scopes(*right)),
            ),
            Expr::CompoundAssign(left, operator, right) => Expr::CompoundAssign(
                Box::new(self.rewrite_defer_scopes(*left)),
                operator,
                Box::new(self.rewrite_defer_scopes(*right)),
            ),
            Expr::Call(callee, arguments) => Expr::Call(
                Box::new(self.rewrite_defer_scopes(*callee)),
                arguments
                    .into_iter()
                    .map(|argument| CallArg {
                        label: argument.label,
                        value: self.rewrite_defer_scopes(argument.value),
                    })
                    .collect(),
            ),
            Expr::StructLiteral {
                constructor,
                fields,
            } => Expr::StructLiteral {
                constructor: Box::new(self.rewrite_defer_scopes(*constructor)),
                fields: fields
                    .into_iter()
                    .map(|field| CallArg {
                        label: field.label,
                        value: self.rewrite_defer_scopes(field.value),
                    })
                    .collect(),
            },
            Expr::Member(base, member) => {
                Expr::Member(Box::new(self.rewrite_defer_scopes(*base)), member)
            }
            Expr::ChainMember(base, member) => {
                Expr::ChainMember(Box::new(self.rewrite_defer_scopes(*base)), member)
            }
            Expr::Index { base, index } => Expr::Index {
                base: Box::new(self.rewrite_defer_scopes(*base)),
                index: Box::new(self.rewrite_defer_scopes(*index)),
            },
            Expr::Unsafe(value) => Expr::Unsafe(Box::new(self.rewrite_defer_scopes(*value))),
            Expr::Closure(parameters, body) => {
                Expr::Closure(parameters, Box::new(self.rewrite_defer_scopes(*body)))
            }
            Expr::PatternClosure {
                pattern,
                guard,
                body,
            } => Expr::PatternClosure {
                pattern,
                guard: guard.map(|guard| Box::new(self.rewrite_defer_scopes(*guard))),
                body: Box::new(self.rewrite_defer_scopes(*body)),
            },
            Expr::If {
                condition,
                then_branch,
                else_branch,
            } => Expr::If {
                condition: Box::new(self.rewrite_defer_scopes(*condition)),
                then_branch: Box::new(self.rewrite_defer_scopes(*then_branch)),
                else_branch: else_branch.map(|branch| Box::new(self.rewrite_defer_scopes(*branch))),
            },
            Expr::Return(value) => {
                Expr::Return(value.map(|value| Box::new(self.rewrite_defer_scopes(*value))))
            }
            Expr::While {
                condition,
                body,
                post_test,
            } => Expr::While {
                condition: Box::new(self.rewrite_defer_scopes(*condition)),
                body: Box::new(self.rewrite_defer_scopes(*body)),
                post_test,
            },
            Expr::Loop { body } => Expr::Loop {
                body: Box::new(self.rewrite_defer_scopes(*body)),
            },
            Expr::Break(value) => {
                Expr::Break(value.map(|value| Box::new(self.rewrite_defer_scopes(*value))))
            }
            Expr::Match { scrutinee, arms } => Expr::Match {
                scrutinee: Box::new(self.rewrite_defer_scopes(*scrutinee)),
                arms: arms
                    .into_iter()
                    .map(|mut arm| {
                        arm.guard = arm.guard.map(|guard| self.rewrite_defer_scopes(guard));
                        arm.body = self.rewrite_defer_scopes(arm.body);
                        arm
                    })
                    .collect(),
            },
            other => other,
        }
    }

    fn rewrite_defer_block(&mut self, statements: Vec<Stmt>, tail: Option<Box<Expr>>) -> Expr {
        let mut rewritten = Vec::with_capacity(statements.len());
        let mut active = Vec::new();

        for statement in statements {
            match statement {
                Stmt::Expr(expression) => {
                    let expression = self.rewrite_defer_scopes(expression);
                    if let Some(action) = take_defer_action(&expression) {
                        let name = self.fresh_defer_name("action");
                        rewritten.push(Stmt::Let(Binding {
                            mutable: false,
                            name: name.clone(),
                            annotation: None,
                            value: action,
                            value_source: None,
                        }));
                        active.push(name);
                    } else {
                        rewritten.push(Stmt::Expr(
                            self.inject_defer_exits(expression, &active, true),
                        ));
                    }
                }
                Stmt::Let(mut binding) => {
                    binding.value = self.rewrite_defer_scopes(binding.value);
                    binding.value = self.inject_defer_exits(binding.value, &active, true);
                    rewritten.push(Stmt::Let(binding));
                }
            }
        }

        let tail = tail.map(|tail| {
            let tail = self.rewrite_defer_scopes(*tail);
            Box::new(self.inject_defer_exits(tail, &active, true))
        });
        let result_name = if tail.is_some() && !active.is_empty() {
            Some(self.fresh_defer_name("result"))
        } else {
            None
        };
        finish_defer_scope(rewritten, tail, &active, result_name)
    }

    fn inject_defer_exits(
        &mut self,
        expression: Expr,
        actions: &[String],
        loop_control_exits_scope: bool,
    ) -> Expr {
        if actions.is_empty() {
            return expression;
        }
        match expression {
            Expr::Located {
                line,
                column,
                end_line,
                end_column,
                value,
            } => Expr::Located {
                line,
                column,
                end_line,
                end_column,
                value: Box::new(self.inject_defer_exits(*value, actions, loop_control_exits_scope)),
            },
            Expr::Return(value) => {
                self.defer_before_exit(value.map(|value| *value), actions, ExitKind::Return)
            }
            Expr::Throw(value) => self.defer_before_exit(Some(*value), actions, ExitKind::Throw),
            Expr::Break(value) if loop_control_exits_scope => {
                self.defer_before_exit(value.map(|value| *value), actions, ExitKind::Break)
            }
            Expr::Continue if loop_control_exits_scope => {
                self.defer_before_exit(None, actions, ExitKind::Continue)
            }
            Expr::Block(statements, tail) => Expr::Block(
                statements
                    .into_iter()
                    .map(|statement| match statement {
                        Stmt::Let(mut binding) => {
                            binding.value = self.inject_defer_exits(
                                binding.value,
                                actions,
                                loop_control_exits_scope,
                            );
                            Stmt::Let(binding)
                        }
                        Stmt::Expr(value) => Stmt::Expr(self.inject_defer_exits(
                            value,
                            actions,
                            loop_control_exits_scope,
                        )),
                    })
                    .collect(),
                tail.map(|tail| {
                    Box::new(self.inject_defer_exits(*tail, actions, loop_control_exits_scope))
                }),
            ),
            Expr::If {
                condition,
                then_branch,
                else_branch,
            } => Expr::If {
                condition,
                then_branch: Box::new(self.inject_defer_exits(
                    *then_branch,
                    actions,
                    loop_control_exits_scope,
                )),
                else_branch: else_branch.map(|branch| {
                    Box::new(self.inject_defer_exits(*branch, actions, loop_control_exits_scope))
                }),
            },
            Expr::While {
                condition,
                body,
                post_test,
            } => Expr::While {
                condition,
                body: Box::new(self.inject_defer_exits(*body, actions, false)),
                post_test,
            },
            Expr::Loop { body } => Expr::Loop {
                body: Box::new(self.inject_defer_exits(*body, actions, false)),
            },
            Expr::Match { scrutinee, arms } => Expr::Match {
                scrutinee,
                arms: arms
                    .into_iter()
                    .map(|mut arm| {
                        arm.body =
                            self.inject_defer_exits(arm.body, actions, loop_control_exits_scope);
                        arm
                    })
                    .collect(),
            },
            Expr::Closure(_, _) | Expr::PatternClosure { .. } | Expr::Async { .. } => expression,
            other => other,
        }
    }

    fn defer_before_exit(
        &mut self,
        value: Option<Expr>,
        actions: &[String],
        exit: ExitKind,
    ) -> Expr {
        let mut statements = Vec::new();
        let value_name = value.map(|value| {
            let name = self.fresh_defer_name("exit");
            statements.push(Stmt::Let(Binding {
                mutable: false,
                name: name.clone(),
                annotation: None,
                value,
                value_source: None,
            }));
            name
        });
        statements.extend(defer_calls(actions).map(Stmt::Expr));
        let value = value_name.map(|name| Box::new(Expr::Name(name)));
        let exit = match exit {
            ExitKind::Return => Expr::Return(value),
            ExitKind::Throw => Expr::Throw(value.expect("throw has a value")),
            ExitKind::Break => Expr::Break(value),
            ExitKind::Continue => Expr::Continue,
        };
        Expr::Block(statements, Some(Box::new(exit)))
    }

    fn fresh_defer_name(&mut self, role: &str) -> String {
        let index = self.next_closure;
        self.next_closure += 1;
        format!("$defer${role}${index}")
    }
}

#[derive(Clone, Copy)]
enum ExitKind {
    Return,
    Throw,
    Break,
    Continue,
}

fn take_defer_action(expression: &Expr) -> Option<Expr> {
    let Expr::Call(callee, arguments) = expression.unlocated() else {
        return None;
    };
    if !matches!(callee.unlocated(), Expr::Name(name) if name == "core::control::defer") {
        return None;
    }
    let [argument] = arguments.as_slice() else {
        return None;
    };
    Some(argument.value.clone())
}

fn defer_calls<'a>(actions: &'a [String]) -> impl Iterator<Item = Expr> + 'a {
    actions
        .iter()
        .rev()
        .map(|name| Expr::Call(Box::new(Expr::Name(name.clone())), Vec::new()))
}

fn finish_defer_scope(
    mut statements: Vec<Stmt>,
    tail: Option<Box<Expr>>,
    actions: &[String],
    result_name: Option<String>,
) -> Expr {
    if actions.is_empty() {
        return Expr::Block(statements, tail);
    }
    let tail = if let Some(tail) = tail {
        let result = result_name.expect("defer result name exists");
        statements.push(Stmt::Let(Binding {
            mutable: false,
            name: result.clone(),
            annotation: None,
            value: *tail,
            value_source: None,
        }));
        statements.extend(defer_calls(actions).map(Stmt::Expr));
        Some(Box::new(Expr::Name(result)))
    } else {
        statements.extend(defer_calls(actions).map(Stmt::Expr));
        None
    };
    Expr::Block(statements, tail)
}

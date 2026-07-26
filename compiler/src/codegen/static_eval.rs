use std::collections::HashMap;

use crate::ast::{BinaryOp, Expr, Function, PassMode, Pattern, StaticExpr, Stmt, Type, UnaryOp};

use super::ctfe_value::CtfeValue;
use super::hir::Ty;
use super::Analyzer;

const STATIC_EVALUATION_FUEL: usize = 1_024;

impl Analyzer {
    pub(super) fn evaluate_static_usize(&mut self, expression: &StaticExpr) -> Option<u64> {
        let mut fuel = STATIC_EVALUATION_FUEL;
        let mut active_calls = Vec::new();
        match self.evaluate_static_expression(
            expression,
            &HashMap::new(),
            &mut fuel,
            &mut active_calls,
        ) {
            Ok(value) if value.ty == Ty::USize => value.usize_value(),
            Ok(value) if value.ty == Ty::Bool => {
                self.error("compile-time array length evaluated to `bool`, expected `usize`");
                None
            }
            Ok(value) => {
                self.error(format!(
                    "compile-time array length evaluated to `{}`, expected `usize`",
                    value.ty
                ));
                None
            }
            Err(message) => {
                self.error(format!("compile-time evaluation failed: {message}"));
                None
            }
        }
    }

    fn evaluate_static_expression(
        &self,
        expression: &StaticExpr,
        locals: &HashMap<String, CtfeValue>,
        fuel: &mut usize,
        active_calls: &mut Vec<(String, Vec<CtfeValue>)>,
    ) -> Result<CtfeValue, String> {
        Self::consume_static_fuel(fuel)?;
        match expression {
            StaticExpr::USize(value) => Ok(CtfeValue::usize(*value)),
            StaticExpr::Bool(value) => Ok(CtfeValue::bool(*value)),
            StaticExpr::Name(name) => locals
                .get(name)
                .cloned()
                .ok_or_else(|| format!("unknown static value `{name}`")),
            StaticExpr::Unary(operator, operand) => {
                let operand =
                    self.evaluate_static_expression(operand, locals, fuel, active_calls)?;
                Self::evaluate_static_unary(*operator, operand)
            }
            StaticExpr::Binary(left, operator, right) => {
                let left = self.evaluate_static_expression(left, locals, fuel, active_calls)?;
                if left.bool_value() == Some(false) && *operator == BinaryOp::And {
                    return Ok(CtfeValue::bool(false));
                }
                if left.bool_value() == Some(true) && *operator == BinaryOp::Or {
                    return Ok(CtfeValue::bool(true));
                }
                let right = self.evaluate_static_expression(right, locals, fuel, active_calls)?;
                Self::evaluate_static_binary(left, *operator, right)
            }
            StaticExpr::Call {
                function,
                arguments,
            } => {
                let arguments = arguments
                    .iter()
                    .map(|argument| {
                        self.evaluate_static_expression(argument, locals, fuel, active_calls)
                    })
                    .collect::<Result<Vec<_>, _>>()?;
                self.evaluate_static_call(function, &arguments, fuel, active_calls)
            }
        }
    }

    fn evaluate_static_call(
        &self,
        name: &str,
        arguments: &[CtfeValue],
        fuel: &mut usize,
        active_calls: &mut Vec<(String, Vec<CtfeValue>)>,
    ) -> Result<CtfeValue, String> {
        Self::consume_static_fuel(fuel)?;
        let call = (name.to_owned(), arguments.to_vec());
        if active_calls.contains(&call) {
            return Err(format!(
                "recursive ctfe call `{name}` repeated with the same arguments"
            ));
        }
        let function = self
            .functions
            .get(name)
            .or_else(|| self.function_templates.get(name))
            .cloned()
            .ok_or_else(|| format!("unknown function `{name}` in static expression"))?;
        self.validate_static_function(&function, arguments)?;

        let parameters = &function.groups[0];
        let mut locals = parameters
            .iter()
            .zip(arguments)
            .map(|(parameter, value)| (parameter.name.clone(), value.clone()))
            .collect::<HashMap<_, _>>();
        active_calls.push(call);
        let result = self.evaluate_static_body(
            function
                .body
                .as_ref()
                .expect("validated static function body"),
            &mut locals,
            fuel,
            active_calls,
        );
        active_calls.pop();
        result
    }

    fn validate_static_function(
        &self,
        function: &Function,
        arguments: &[CtfeValue],
    ) -> Result<(), String> {
        if function.foreign.is_some() || function.builtin || function.body.is_none() {
            return Err(format!(
                "function `{}` has no source body available to ctfe",
                function.name
            ));
        }
        if !function.compile_groups.is_empty() {
            return Err(format!(
                "generic ctfe function `{}` is not supported yet",
                function.name
            ));
        }
        if function.groups.len() != 1 || function.groups[0].len() != arguments.len() {
            return Err(format!(
                "ctfe function `{}` must have one fully-applied parameter group",
                function.name
            ));
        }
        if function.effects != Default::default() {
            return Err(format!(
                "effectful function `{}` cannot run during ctfe",
                function.name
            ));
        }
        if !function.where_predicates.is_empty() {
            return Err(format!(
                "constrained function `{}` cannot run during ctfe yet",
                function.name
            ));
        }
        for (parameter, argument) in function.groups[0].iter().zip(arguments) {
            if !matches!(
                parameter.mode,
                PassMode::Inferred | PassMode::Copy | PassMode::Move
            ) {
                return Err(format!(
                    "ctfe parameter `{}.{}` cannot borrow runtime storage",
                    function.name, parameter.name
                ));
            }
            let compatible = matches!(
                (&parameter.ty, &argument.ty),
                (Type::USize, Ty::USize) | (Type::Bool, Ty::Bool)
            );
            if !compatible {
                return Err(format!(
                    "ctfe argument for `{}.{}` does not match its `usize`/`bool` parameter type",
                    function.name, parameter.name
                ));
            }
        }
        if !matches!(function.return_type, Some(Type::USize | Type::Bool)) {
            return Err(format!(
                "ctfe function `{}` must explicitly return `usize` or `bool`",
                function.name
            ));
        }
        Ok(())
    }

    fn evaluate_static_body(
        &self,
        expression: &Expr,
        locals: &mut HashMap<String, CtfeValue>,
        fuel: &mut usize,
        active_calls: &mut Vec<(String, Vec<CtfeValue>)>,
    ) -> Result<CtfeValue, String> {
        Self::consume_static_fuel(fuel)?;
        match expression.unlocated() {
            Expr::Integer(value) => u64::try_from(*value)
                .map(CtfeValue::usize)
                .map_err(|_| "integer value does not fit in `usize`".to_owned()),
            Expr::Bool(value) => Ok(CtfeValue::bool(*value)),
            Expr::Name(name) => locals
                .get(name)
                .cloned()
                .ok_or_else(|| format!("unknown local `{name}` in ctfe function")),
            Expr::Unary(operator, operand) => {
                let operand = self.evaluate_static_body(operand, locals, fuel, active_calls)?;
                Self::evaluate_static_unary(*operator, operand)
            }
            Expr::Binary(left, operator, right) => {
                let left = self.evaluate_static_body(left, locals, fuel, active_calls)?;
                if left.bool_value() == Some(false) && *operator == BinaryOp::And {
                    return Ok(CtfeValue::bool(false));
                }
                if left.bool_value() == Some(true) && *operator == BinaryOp::Or {
                    return Ok(CtfeValue::bool(true));
                }
                let right = self.evaluate_static_body(right, locals, fuel, active_calls)?;
                Self::evaluate_static_binary(left, *operator, right)
            }
            Expr::Call(callee, arguments) => {
                let Expr::Name(function) = callee.unlocated() else {
                    return Err("ctfe calls must name a top-level function".to_owned());
                };
                if arguments.iter().any(|argument| argument.label.is_some()) {
                    return Err("labeled ctfe calls are not supported yet".to_owned());
                }
                let arguments = arguments
                    .iter()
                    .map(|argument| {
                        self.evaluate_static_body(&argument.value, locals, fuel, active_calls)
                    })
                    .collect::<Result<Vec<_>, _>>()?;
                self.evaluate_static_call(function, &arguments, fuel, active_calls)
            }
            Expr::Block(statements, tail) => {
                let mut block_locals = locals.clone();
                for statement in statements {
                    match statement {
                        Stmt::Let(binding) if !binding.mutable => {
                            let value = self.evaluate_static_body(
                                &binding.value,
                                &mut block_locals,
                                fuel,
                                active_calls,
                            )?;
                            block_locals.insert(binding.name.clone(), value);
                        }
                        Stmt::Let(_) => {
                            return Err("mutable bindings are not permitted during ctfe".to_owned());
                        }
                        Stmt::Expr(expression) => {
                            self.evaluate_static_body(
                                expression,
                                &mut block_locals,
                                fuel,
                                active_calls,
                            )?;
                        }
                    }
                }
                let tail = tail
                    .as_deref()
                    .ok_or_else(|| "ctfe block must produce a value".to_owned())?;
                self.evaluate_static_body(tail, &mut block_locals, fuel, active_calls)
            }
            Expr::If {
                condition,
                then_branch,
                else_branch,
            } => {
                let condition = self
                    .evaluate_static_body(condition, locals, fuel, active_calls)?
                    .bool_value()
                    .ok_or_else(|| "ctfe `if` condition must be `bool`".to_owned())?;
                if condition {
                    self.evaluate_static_body(then_branch, locals, fuel, active_calls)
                } else {
                    let branch = else_branch
                        .as_deref()
                        .ok_or_else(|| "value-producing ctfe `if` requires `else`".to_owned())?;
                    self.evaluate_static_body(branch, locals, fuel, active_calls)
                }
            }
            Expr::Match { scrutinee, arms } => {
                let value = self.evaluate_static_body(scrutinee, locals, fuel, active_calls)?;
                for arm in arms {
                    let mut arm_locals = locals.clone();
                    if !Self::static_pattern_matches(&arm.pattern, &value, &mut arm_locals)? {
                        continue;
                    }
                    if let Some(guard) = &arm.guard {
                        let guard = self
                            .evaluate_static_body(
                                guard,
                                &mut arm_locals,
                                fuel,
                                active_calls,
                            )?
                            .bool_value()
                            .ok_or_else(|| "ctfe match guard must be `bool`".to_owned())?;
                        if !guard {
                            continue;
                        }
                    }
                    return self.evaluate_static_body(
                        &arm.body,
                        &mut arm_locals,
                        fuel,
                        active_calls,
                    );
                }
                Err("non-exhaustive match during ctfe".to_owned())
            }
            _ => Err(
                "expression is not in the pure ctfe subset (no mutation, borrowing, loops, handlers, or closures)"
                    .to_owned(),
            ),
        }
    }

    fn static_pattern_matches(
        pattern: &Pattern,
        value: &CtfeValue,
        locals: &mut HashMap<String, CtfeValue>,
    ) -> Result<bool, String> {
        match pattern {
            Pattern::Bool(expected) if value.ty == Ty::Bool => {
                Ok(value.bool_value() == Some(*expected))
            }
            Pattern::Integer(expected) if value.ty == Ty::USize && !expected.negative => {
                Ok(value.usize_value().is_some_and(|actual| {
                    u64::try_from(expected.magnitude).is_ok_and(|expected| expected == actual)
                }))
            }
            Pattern::Integer(_) if value.ty == Ty::USize => Ok(false),
            Pattern::Wildcard => Ok(true),
            Pattern::Binding(name) => {
                locals.insert(name.clone(), value.clone());
                Ok(true)
            }
            Pattern::Integer(_)
            | Pattern::Bool(_)
            | Pattern::Tuple(_)
            | Pattern::Constructor { .. } => {
                Err("pattern is outside the `usize`/`bool` ctfe subset".to_owned())
            }
        }
    }

    fn evaluate_static_unary(operator: UnaryOp, operand: CtfeValue) -> Result<CtfeValue, String> {
        match operator {
            UnaryOp::Not if operand.ty == Ty::Bool => Ok(CtfeValue::bool(
                !operand.bool_value().expect("bool value has bool payload"),
            )),
            UnaryOp::Neg if operand.ty == Ty::USize => {
                Err("negation cannot produce a compile-time `usize`".to_owned())
            }
            UnaryOp::Deref => Err("dereference is not permitted during ctfe".to_owned()),
            _ => Err("invalid unary operand during ctfe".to_owned()),
        }
    }

    fn evaluate_static_binary(
        left: CtfeValue,
        operator: BinaryOp,
        right: CtfeValue,
    ) -> Result<CtfeValue, String> {
        if left.ty == Ty::USize && right.ty == Ty::USize {
            let left = left.usize_value().expect("usize value has usize payload");
            let right = right.usize_value().expect("usize value has usize payload");
            return match operator {
                BinaryOp::Add => left
                    .checked_add(right)
                    .map(CtfeValue::usize)
                    .ok_or_else(|| "`usize` overflow in ctfe addition".to_owned()),
                BinaryOp::Sub => left
                    .checked_sub(right)
                    .map(CtfeValue::usize)
                    .ok_or_else(|| "`usize` underflow in ctfe subtraction".to_owned()),
                BinaryOp::Mul => left
                    .checked_mul(right)
                    .map(CtfeValue::usize)
                    .ok_or_else(|| "`usize` overflow in ctfe multiplication".to_owned()),
                BinaryOp::Div | BinaryOp::Rem if right == 0 => {
                    Err("division by zero during ctfe".to_owned())
                }
                BinaryOp::Div => Ok(CtfeValue::usize(left / right)),
                BinaryOp::Rem => Ok(CtfeValue::usize(left % right)),
                BinaryOp::BitAnd => Ok(CtfeValue::usize(left & right)),
                BinaryOp::BitOr => Ok(CtfeValue::usize(left | right)),
                BinaryOp::BitXor => Ok(CtfeValue::usize(left ^ right)),
                BinaryOp::Shl => u32::try_from(right)
                    .ok()
                    .and_then(|right| left.checked_shl(right))
                    .map(CtfeValue::usize)
                    .ok_or_else(|| "invalid left shift during ctfe".to_owned()),
                BinaryOp::Shr => u32::try_from(right)
                    .ok()
                    .and_then(|right| left.checked_shr(right))
                    .map(CtfeValue::usize)
                    .ok_or_else(|| "invalid right shift during ctfe".to_owned()),
                BinaryOp::Eq => Ok(CtfeValue::bool(left == right)),
                BinaryOp::Ne => Ok(CtfeValue::bool(left != right)),
                BinaryOp::Lt => Ok(CtfeValue::bool(left < right)),
                BinaryOp::Le => Ok(CtfeValue::bool(left <= right)),
                BinaryOp::Gt => Ok(CtfeValue::bool(left > right)),
                BinaryOp::Ge => Ok(CtfeValue::bool(left >= right)),
                BinaryOp::And | BinaryOp::Or => {
                    Err("invalid operand sorts in ctfe expression".to_owned())
                }
            };
        }
        if left.ty == Ty::Bool && right.ty == Ty::Bool {
            let left = left.bool_value().expect("bool value has bool payload");
            let right = right.bool_value().expect("bool value has bool payload");
            return match operator {
                BinaryOp::Eq => Ok(CtfeValue::bool(left == right)),
                BinaryOp::Ne => Ok(CtfeValue::bool(left != right)),
                BinaryOp::And => Ok(CtfeValue::bool(left && right)),
                BinaryOp::Or => Ok(CtfeValue::bool(left || right)),
                _ => Err("invalid operand sorts in ctfe expression".to_owned()),
            };
        }
        Err("invalid operand sorts in ctfe expression".to_owned())
    }

    fn consume_static_fuel(fuel: &mut usize) -> Result<(), String> {
        *fuel = fuel
            .checked_sub(1)
            .ok_or_else(|| "evaluation exceeded the 1024-step limit".to_owned())?;
        Ok(())
    }
}

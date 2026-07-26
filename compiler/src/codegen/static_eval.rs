use std::collections::HashMap;

use crate::ast::{BinaryOp, Expr, Function, PassMode, Pattern, StaticExpr, Stmt, Type, UnaryOp};

use super::Analyzer;

const STATIC_EVALUATION_FUEL: usize = 1_024;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
enum Scalar {
    USize(u64),
    Bool(bool),
}

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
            Ok(Scalar::USize(value)) => Some(value),
            Ok(Scalar::Bool(_)) => {
                self.error("compile-time array length evaluated to `bool`, expected `usize`");
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
        locals: &HashMap<String, Scalar>,
        fuel: &mut usize,
        active_calls: &mut Vec<(String, Vec<Scalar>)>,
    ) -> Result<Scalar, String> {
        Self::consume_static_fuel(fuel)?;
        match expression {
            StaticExpr::USize(value) => Ok(Scalar::USize(*value)),
            StaticExpr::Bool(value) => Ok(Scalar::Bool(*value)),
            StaticExpr::Name(name) => locals
                .get(name)
                .copied()
                .ok_or_else(|| format!("unknown static value `{name}`")),
            StaticExpr::Unary(operator, operand) => {
                let operand =
                    self.evaluate_static_expression(operand, locals, fuel, active_calls)?;
                Self::evaluate_static_unary(*operator, operand)
            }
            StaticExpr::Binary(left, operator, right) => {
                let left = self.evaluate_static_expression(left, locals, fuel, active_calls)?;
                if matches!((left, operator), (Scalar::Bool(false), BinaryOp::And)) {
                    return Ok(Scalar::Bool(false));
                }
                if matches!((left, operator), (Scalar::Bool(true), BinaryOp::Or)) {
                    return Ok(Scalar::Bool(true));
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
        arguments: &[Scalar],
        fuel: &mut usize,
        active_calls: &mut Vec<(String, Vec<Scalar>)>,
    ) -> Result<Scalar, String> {
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
            .map(|(parameter, value)| (parameter.name.clone(), *value))
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
        arguments: &[Scalar],
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
                (&parameter.ty, argument),
                (Type::USize, Scalar::USize(_)) | (Type::Bool, Scalar::Bool(_))
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
        locals: &mut HashMap<String, Scalar>,
        fuel: &mut usize,
        active_calls: &mut Vec<(String, Vec<Scalar>)>,
    ) -> Result<Scalar, String> {
        Self::consume_static_fuel(fuel)?;
        match expression.unlocated() {
            Expr::Integer(value) => u64::try_from(*value)
                .map(Scalar::USize)
                .map_err(|_| "integer value does not fit in `usize`".to_owned()),
            Expr::Bool(value) => Ok(Scalar::Bool(*value)),
            Expr::Name(name) => locals
                .get(name)
                .copied()
                .ok_or_else(|| format!("unknown local `{name}` in ctfe function")),
            Expr::Unary(operator, operand) => {
                let operand = self.evaluate_static_body(operand, locals, fuel, active_calls)?;
                Self::evaluate_static_unary(*operator, operand)
            }
            Expr::Binary(left, operator, right) => {
                let left = self.evaluate_static_body(left, locals, fuel, active_calls)?;
                if matches!((left, operator), (Scalar::Bool(false), BinaryOp::And)) {
                    return Ok(Scalar::Bool(false));
                }
                if matches!((left, operator), (Scalar::Bool(true), BinaryOp::Or)) {
                    return Ok(Scalar::Bool(true));
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
                let Scalar::Bool(condition) =
                    self.evaluate_static_body(condition, locals, fuel, active_calls)?
                else {
                    return Err("ctfe `if` condition must be `bool`".to_owned());
                };
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
                    if !Self::static_pattern_matches(&arm.pattern, value, &mut arm_locals)? {
                        continue;
                    }
                    if let Some(guard) = &arm.guard {
                        let Scalar::Bool(guard) = self.evaluate_static_body(
                            guard,
                            &mut arm_locals,
                            fuel,
                            active_calls,
                        )?
                        else {
                            return Err("ctfe match guard must be `bool`".to_owned());
                        };
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
        value: Scalar,
        locals: &mut HashMap<String, Scalar>,
    ) -> Result<bool, String> {
        match (pattern, value) {
            (Pattern::Bool(expected), Scalar::Bool(actual)) => Ok(*expected == actual),
            (Pattern::Integer(expected), Scalar::USize(actual)) if !expected.negative => {
                Ok(u64::try_from(expected.magnitude).is_ok_and(|expected| expected == actual))
            }
            (Pattern::Integer(_), Scalar::USize(_)) => Ok(false),
            (Pattern::Wildcard, _) => Ok(true),
            (Pattern::Binding(name), value) => {
                locals.insert(name.clone(), value);
                Ok(true)
            }
            (Pattern::Integer(_), Scalar::Bool(_))
            | (Pattern::Bool(_), Scalar::USize(_))
            | (Pattern::Tuple(_), _)
            | (Pattern::Constructor { .. }, _) => {
                Err("pattern is outside the `usize`/`bool` ctfe subset".to_owned())
            }
        }
    }

    fn evaluate_static_unary(operator: UnaryOp, operand: Scalar) -> Result<Scalar, String> {
        match (operator, operand) {
            (UnaryOp::Not, Scalar::Bool(value)) => Ok(Scalar::Bool(!value)),
            (UnaryOp::Neg, Scalar::USize(_)) => {
                Err("negation cannot produce a compile-time `usize`".to_owned())
            }
            (UnaryOp::Deref, _) => Err("dereference is not permitted during ctfe".to_owned()),
            _ => Err("invalid unary operand during ctfe".to_owned()),
        }
    }

    fn evaluate_static_binary(
        left: Scalar,
        operator: BinaryOp,
        right: Scalar,
    ) -> Result<Scalar, String> {
        match (left, operator, right) {
            (Scalar::USize(left), BinaryOp::Add, Scalar::USize(right)) => left
                .checked_add(right)
                .map(Scalar::USize)
                .ok_or_else(|| "`usize` overflow in ctfe addition".to_owned()),
            (Scalar::USize(left), BinaryOp::Sub, Scalar::USize(right)) => left
                .checked_sub(right)
                .map(Scalar::USize)
                .ok_or_else(|| "`usize` underflow in ctfe subtraction".to_owned()),
            (Scalar::USize(left), BinaryOp::Mul, Scalar::USize(right)) => left
                .checked_mul(right)
                .map(Scalar::USize)
                .ok_or_else(|| "`usize` overflow in ctfe multiplication".to_owned()),
            (Scalar::USize(_), BinaryOp::Div | BinaryOp::Rem, Scalar::USize(0)) => {
                Err("division by zero during ctfe".to_owned())
            }
            (Scalar::USize(left), BinaryOp::Div, Scalar::USize(right)) => {
                Ok(Scalar::USize(left / right))
            }
            (Scalar::USize(left), BinaryOp::Rem, Scalar::USize(right)) => {
                Ok(Scalar::USize(left % right))
            }
            (Scalar::USize(left), BinaryOp::BitAnd, Scalar::USize(right)) => {
                Ok(Scalar::USize(left & right))
            }
            (Scalar::USize(left), BinaryOp::BitOr, Scalar::USize(right)) => {
                Ok(Scalar::USize(left | right))
            }
            (Scalar::USize(left), BinaryOp::BitXor, Scalar::USize(right)) => {
                Ok(Scalar::USize(left ^ right))
            }
            (Scalar::USize(left), BinaryOp::Shl, Scalar::USize(right)) => u32::try_from(right)
                .ok()
                .and_then(|right| left.checked_shl(right))
                .map(Scalar::USize)
                .ok_or_else(|| "invalid left shift during ctfe".to_owned()),
            (Scalar::USize(left), BinaryOp::Shr, Scalar::USize(right)) => u32::try_from(right)
                .ok()
                .and_then(|right| left.checked_shr(right))
                .map(Scalar::USize)
                .ok_or_else(|| "invalid right shift during ctfe".to_owned()),
            (Scalar::USize(left), BinaryOp::Eq, Scalar::USize(right)) => {
                Ok(Scalar::Bool(left == right))
            }
            (Scalar::USize(left), BinaryOp::Ne, Scalar::USize(right)) => {
                Ok(Scalar::Bool(left != right))
            }
            (Scalar::USize(left), BinaryOp::Lt, Scalar::USize(right)) => {
                Ok(Scalar::Bool(left < right))
            }
            (Scalar::USize(left), BinaryOp::Le, Scalar::USize(right)) => {
                Ok(Scalar::Bool(left <= right))
            }
            (Scalar::USize(left), BinaryOp::Gt, Scalar::USize(right)) => {
                Ok(Scalar::Bool(left > right))
            }
            (Scalar::USize(left), BinaryOp::Ge, Scalar::USize(right)) => {
                Ok(Scalar::Bool(left >= right))
            }
            (Scalar::Bool(left), BinaryOp::Eq, Scalar::Bool(right)) => {
                Ok(Scalar::Bool(left == right))
            }
            (Scalar::Bool(left), BinaryOp::Ne, Scalar::Bool(right)) => {
                Ok(Scalar::Bool(left != right))
            }
            (Scalar::Bool(left), BinaryOp::And, Scalar::Bool(right)) => {
                Ok(Scalar::Bool(left && right))
            }
            (Scalar::Bool(left), BinaryOp::Or, Scalar::Bool(right)) => {
                Ok(Scalar::Bool(left || right))
            }
            _ => Err("invalid operand sorts in ctfe expression".to_owned()),
        }
    }

    fn consume_static_fuel(fuel: &mut usize) -> Result<(), String> {
        *fuel = fuel
            .checked_sub(1)
            .ok_or_else(|| "evaluation exceeded the 1024-step limit".to_owned())?;
        Ok(())
    }
}

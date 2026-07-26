use std::collections::{HashMap, HashSet};

use crate::ast::{
    BinaryOp, Expr, Function, PassMode, Pattern, PatternFields, StaticExpr, Stmt, Type, UnaryOp,
};

use super::ctfe_value::{CtfeValue, CtfeValueKind, IntegerEvalError};
use super::hir::Ty;
use super::lower::flatten_call;
use super::Analyzer;

const STATIC_EVALUATION_FUEL: usize = 1_024;
const MAX_CTFE_AGGREGATE_ELEMENTS: usize = 65_536;
const MAX_CTFE_VALUE_NESTING: usize = 64;

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
        let result_ty = self.validate_static_function(&function, arguments)?;

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
            Some(&result_ty),
            &mut locals,
            fuel,
            active_calls,
        );
        active_calls.pop();
        let result = result?;
        Self::expect_static_type(result, &result_ty, "ctfe function result")
    }

    fn validate_static_function(
        &self,
        function: &Function,
        arguments: &[CtfeValue],
    ) -> Result<Ty, String> {
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
            let parameter_ty = self.static_value_type(&parameter.ty).ok_or_else(|| {
                format!(
                    "ctfe parameter `{}.{}` has unsupported type `{}`",
                    function.name,
                    parameter.name,
                    self.source_type_name(&parameter.ty)
                )
            })?;
            self.validate_static_value_type(&parameter_ty)?;
            if argument.ty != parameter_ty {
                return Err(format!(
                    "ctfe argument for `{}.{}` has type `{}`, expected `{parameter_ty}`",
                    function.name, parameter.name, argument.ty
                ));
            }
        }
        let return_type = function.return_type.as_ref().ok_or_else(|| {
            format!(
                "ctfe function `{}` must have an explicit return type",
                function.name
            )
        })?;
        let result = self.static_value_type(return_type).ok_or_else(|| {
            format!(
                "ctfe function `{}` has unsupported result type `{}`",
                function.name,
                self.source_type_name(return_type)
            )
        })?;
        self.validate_static_value_type(&result)?;
        Ok(result)
    }

    fn evaluate_static_body(
        &self,
        expression: &Expr,
        expected: Option<&Ty>,
        locals: &mut HashMap<String, CtfeValue>,
        fuel: &mut usize,
        active_calls: &mut Vec<(String, Vec<CtfeValue>)>,
    ) -> Result<CtfeValue, String> {
        Self::consume_static_fuel(fuel)?;
        let value = match expression.unlocated() {
            Expr::Integer(magnitude) => {
                let ty = expected
                    .filter(|ty| ty.is_integer())
                    .cloned()
                    .unwrap_or(Ty::I32);
                CtfeValue::integer_literal(ty, *magnitude, false)?
            }
            Expr::Bool(value) => CtfeValue::bool(*value),
            Expr::Unit => CtfeValue::unit(),
            Expr::Tuple(fields) => {
                if fields.len() > MAX_CTFE_AGGREGATE_ELEMENTS {
                    return Err(format!(
                        "ctfe tuple exceeds the {MAX_CTFE_AGGREGATE_ELEMENTS}-element limit"
                    ));
                }
                let expected_fields = match expected {
                    Some(Ty::Tuple(expected_fields)) if expected_fields.len() == fields.len() => {
                        Some(expected_fields.as_slice())
                    }
                    Some(Ty::Tuple(expected_fields)) => {
                        return Err(format!(
                            "ctfe tuple length mismatch: expected {}, found {}",
                            expected_fields.len(),
                            fields.len()
                        ));
                    }
                    Some(expected) => {
                        return Err(format!(
                            "ctfe tuple cannot be used where `{expected}` is expected"
                        ));
                    }
                    None => None,
                };
                let values = fields
                    .iter()
                    .enumerate()
                    .map(|(index, field)| {
                        self.evaluate_static_body(
                            field,
                            expected_fields.map(|fields| &fields[index]),
                            locals,
                            fuel,
                            active_calls,
                        )
                    })
                    .collect::<Result<Vec<_>, _>>()?;
                let ty = Ty::Tuple(values.iter().map(|value| value.ty.clone()).collect());
                self.validate_static_value_type(&ty)?;
                CtfeValue {
                    ty,
                    kind: CtfeValueKind::Tuple(values),
                }
            }
            Expr::Array(elements) => {
                if elements.len() > MAX_CTFE_AGGREGATE_ELEMENTS {
                    return Err(format!(
                        "ctfe array exceeds the {MAX_CTFE_AGGREGATE_ELEMENTS}-element limit"
                    ));
                }
                let (element_ty, expected_length) = match expected {
                    Some(Ty::Array(element, length)) => {
                        (Some(element.as_ref().clone()), Some(*length))
                    }
                    Some(expected) => {
                        return Err(format!(
                            "ctfe array cannot be used where `{expected}` is expected"
                        ));
                    }
                    None => (None, None),
                };
                if let Some(length) = expected_length {
                    if elements.len() as u64 != length {
                        return Err(format!(
                            "ctfe array length mismatch: expected {length}, found {}",
                            elements.len()
                        ));
                    }
                }
                let Some((first, rest)) = elements.split_first() else {
                    let Some(element_ty) = element_ty else {
                        return Err(
                            "empty ctfe array requires an expected fixed-array type".to_owned()
                        );
                    };
                    let ty = Ty::Array(Box::new(element_ty), 0);
                    self.validate_static_value_type(&ty)?;
                    return Ok(CtfeValue {
                        ty,
                        kind: CtfeValueKind::Array(Vec::new()),
                    });
                };
                let first = self.evaluate_static_body(
                    first,
                    element_ty.as_ref(),
                    locals,
                    fuel,
                    active_calls,
                )?;
                let element_ty = element_ty.unwrap_or_else(|| first.ty.clone());
                let mut values = vec![Self::expect_static_type(
                    first,
                    &element_ty,
                    "ctfe array element",
                )?];
                for element in rest {
                    let value = self.evaluate_static_body(
                        element,
                        Some(&element_ty),
                        locals,
                        fuel,
                        active_calls,
                    )?;
                    values.push(value);
                }
                let ty = Ty::Array(Box::new(element_ty), values.len() as u64);
                self.validate_static_value_type(&ty)?;
                CtfeValue {
                    ty,
                    kind: CtfeValueKind::Array(values),
                }
            }
            Expr::StructLiteral {
                constructor,
                fields,
            } => {
                let name = self.static_struct_constructor_name(constructor, expected)?;
                let ty = Ty::Struct(name.clone());
                self.validate_static_value_type(&ty)?;
                let layout = self
                    .struct_layouts
                    .get(&name)
                    .ok_or_else(|| format!("unknown struct `{name}` during ctfe"))?;
                if fields.len() != layout.fields.len() {
                    return Err(format!(
                        "ctfe struct `{}` field count mismatch: expected {}, found {}",
                        layout.source_name,
                        layout.fields.len(),
                        fields.len()
                    ));
                }
                let mut values = vec![None; layout.fields.len()];
                for argument in fields {
                    let label = argument.label.as_deref().ok_or_else(|| {
                        format!(
                            "ctfe struct `{}` fields must be labeled",
                            layout.source_name
                        )
                    })?;
                    let index = layout
                        .fields
                        .iter()
                        .position(|field| field.name == label)
                        .ok_or_else(|| {
                            format!(
                                "unknown field `{label}` in ctfe struct `{}`",
                                layout.source_name
                            )
                        })?;
                    if values[index].is_some() {
                        return Err(format!(
                            "duplicate field `{label}` in ctfe struct `{}`",
                            layout.source_name
                        ));
                    }
                    let value = self.evaluate_static_body(
                        &argument.value,
                        Some(&layout.fields[index].ty),
                        locals,
                        fuel,
                        active_calls,
                    )?;
                    values[index] = Some(Self::expect_static_type(
                        value,
                        &layout.fields[index].ty,
                        &format!("ctfe struct field `{label}`"),
                    )?);
                }
                let values = values
                    .into_iter()
                    .enumerate()
                    .map(|(index, value)| {
                        value.ok_or_else(|| {
                            format!(
                                "missing field `{}` in ctfe struct `{}`",
                                layout.fields[index].name, layout.source_name
                            )
                        })
                    })
                    .collect::<Result<Vec<_>, _>>()?;
                CtfeValue {
                    ty,
                    kind: CtfeValueKind::Struct {
                        name,
                        fields: values,
                    },
                }
            }
            Expr::Name(name) => locals
                .get(name)
                .cloned()
                .ok_or_else(|| format!("unknown local `{name}` in ctfe function"))?,
            Expr::Unary(UnaryOp::Neg, operand)
                if matches!(operand.unlocated(), Expr::Integer(_)) =>
            {
                let Expr::Integer(magnitude) = operand.unlocated() else {
                    unreachable!("negative literal guard")
                };
                let ty = expected
                    .filter(|ty| ty.is_integer())
                    .cloned()
                    .unwrap_or(Ty::I32);
                CtfeValue::integer_literal(ty, *magnitude, true)?
            }
            Expr::Unary(operator, operand) => {
                let operand_expected = match operator {
                    UnaryOp::Not => Some(Ty::Bool),
                    UnaryOp::Neg => expected.filter(|ty| ty.is_integer()).cloned(),
                    UnaryOp::Deref => None,
                };
                let operand = self.evaluate_static_body(
                    operand,
                    operand_expected.as_ref(),
                    locals,
                    fuel,
                    active_calls,
                )?;
                Self::evaluate_static_unary(*operator, operand)?
            }
            Expr::Binary(left, operator, right) => {
                let operand_ty =
                    self.static_binary_operand_type(left, *operator, right, expected, locals);
                let left = self.evaluate_static_body(
                    left,
                    operand_ty.as_ref(),
                    locals,
                    fuel,
                    active_calls,
                )?;
                if left.bool_value() == Some(false) && *operator == BinaryOp::And {
                    return Ok(CtfeValue::bool(false));
                }
                if left.bool_value() == Some(true) && *operator == BinaryOp::Or {
                    return Ok(CtfeValue::bool(true));
                }
                let right = self.evaluate_static_body(
                    right,
                    operand_ty.as_ref(),
                    locals,
                    fuel,
                    active_calls,
                )?;
                Self::evaluate_static_binary(left, *operator, right)?
            }
            Expr::Call(callee, arguments) => {
                let Expr::Name(function_name) = callee.unlocated() else {
                    return Err("ctfe calls must name a top-level function".to_owned());
                };
                if arguments.iter().any(|argument| argument.label.is_some()) {
                    return Err("labeled ctfe calls are not supported yet".to_owned());
                }
                let function = self
                    .functions
                    .get(function_name)
                    .or_else(|| self.function_templates.get(function_name))
                    .cloned()
                    .ok_or_else(|| {
                        format!("unknown function `{function_name}` in static expression")
                    })?;
                if function.groups.len() != 1 || function.groups[0].len() != arguments.len() {
                    return Err(format!(
                        "ctfe function `{function_name}` must have one fully-applied parameter group"
                    ));
                }
                let arguments = arguments
                    .iter()
                    .zip(&function.groups[0])
                    .map(|(argument, parameter)| {
                        let parameter_ty =
                            self.static_value_type(&parameter.ty).ok_or_else(|| {
                                format!(
                                    "ctfe parameter `{function_name}.{}` has unsupported type `{}`",
                                    parameter.name,
                                    self.source_type_name(&parameter.ty)
                                )
                            })?;
                        self.evaluate_static_body(
                            &argument.value,
                            Some(&parameter_ty),
                            locals,
                            fuel,
                            active_calls,
                        )
                    })
                    .collect::<Result<Vec<_>, _>>()?;
                self.evaluate_static_call(function_name, &arguments, fuel, active_calls)?
            }
            Expr::Member(base, member) => {
                let base = self.evaluate_static_body(base, None, locals, fuel, active_calls)?;
                let (index, field_ty) = match &base.ty {
                    Ty::Tuple(fields) => {
                        let index = member.parse::<usize>().map_err(|_| {
                            format!(
                                "ctfe tuple projection requires a decimal index, found `{member}`"
                            )
                        })?;
                        let field_ty = fields.get(index).ok_or_else(|| {
                            format!(
                                "ctfe tuple index {index} is out of bounds for tuple of length {}",
                                fields.len()
                            )
                        })?;
                        (index, field_ty)
                    }
                    Ty::Struct(name) => {
                        let layout = self
                            .struct_layouts
                            .get(name)
                            .ok_or_else(|| format!("unknown struct `{name}` during ctfe"))?;
                        let index = layout
                            .fields
                            .iter()
                            .position(|field| field.name == *member)
                            .ok_or_else(|| {
                                format!(
                                    "unknown field `{member}` in ctfe struct `{}`",
                                    layout.source_name
                                )
                            })?;
                        (index, &layout.fields[index].ty)
                    }
                    _ => {
                        return Err(format!(
                            "ctfe projection `{member}` requires a tuple or struct, found `{}`",
                            base.ty
                        ));
                    }
                };
                let value = base
                    .projection(index)
                    .cloned()
                    .ok_or_else(|| "invalid ctfe aggregate projection".to_owned())?;
                Self::expect_static_type(value, field_ty, "ctfe aggregate projection")?
            }
            Expr::Index { base, index } => {
                let base = self.evaluate_static_body(base, None, locals, fuel, active_calls)?;
                let CtfeValueKind::Array(elements) = &base.kind else {
                    return Err(format!(
                        "ctfe indexing requires a fixed array, found `{}`",
                        base.ty
                    ));
                };
                let index = self
                    .evaluate_static_body(index, Some(&Ty::USize), locals, fuel, active_calls)?
                    .usize_value()
                    .ok_or_else(|| "ctfe array index must be `usize`".to_owned())?;
                let index = usize::try_from(index)
                    .map_err(|_| "ctfe array index is out of bounds".to_owned())?;
                elements.get(index).cloned().ok_or_else(|| {
                    format!(
                        "ctfe array index {index} is out of bounds for length {}",
                        elements.len()
                    )
                })?
            }
            Expr::Block(statements, tail) => {
                let mut block_locals = locals.clone();
                for statement in statements {
                    match statement {
                        Stmt::Let(binding) if !binding.mutable => {
                            let annotation = binding
                                .annotation
                                .as_ref()
                                .map(|annotation| {
                                    self.static_value_type(annotation).ok_or_else(|| {
                                        format!(
                                            "ctfe local `{}` has unsupported type `{}`",
                                            binding.name,
                                            self.source_type_name(annotation)
                                        )
                                    })
                                })
                                .transpose()?;
                            if let Some(annotation) = &annotation {
                                self.validate_static_value_type(annotation)?;
                            }
                            let value = self.evaluate_static_body(
                                &binding.value,
                                annotation.as_ref(),
                                &mut block_locals,
                                fuel,
                                active_calls,
                            )?;
                            if let Some(annotation) = &annotation {
                                Self::expect_static_type(
                                    value.clone(),
                                    annotation,
                                    &format!("ctfe local `{}`", binding.name),
                                )?;
                            }
                            block_locals.insert(binding.name.clone(), value);
                        }
                        Stmt::Let(_) => {
                            return Err("mutable bindings are not permitted during ctfe".to_owned());
                        }
                        Stmt::Expr(expression) => {
                            self.evaluate_static_body(
                                expression,
                                None,
                                &mut block_locals,
                                fuel,
                                active_calls,
                            )?;
                        }
                    }
                }
                match tail.as_deref() {
                    Some(tail) => self.evaluate_static_body(
                        tail,
                        expected,
                        &mut block_locals,
                        fuel,
                        active_calls,
                    )?,
                    None => CtfeValue::unit(),
                }
            }
            Expr::If {
                condition,
                then_branch,
                else_branch,
            } => {
                let condition = self
                    .evaluate_static_body(condition, Some(&Ty::Bool), locals, fuel, active_calls)?
                    .bool_value()
                    .ok_or_else(|| "ctfe `if` condition must be `bool`".to_owned())?;
                if condition {
                    self.evaluate_static_body(then_branch, expected, locals, fuel, active_calls)?
                } else if let Some(branch) = else_branch.as_deref() {
                    self.evaluate_static_body(branch, expected, locals, fuel, active_calls)?
                } else {
                    CtfeValue::unit()
                }
            }
            Expr::Match { scrutinee, arms } => {
                let value =
                    self.evaluate_static_body(scrutinee, None, locals, fuel, active_calls)?;
                let mut selected = None;
                for arm in arms {
                    let mut arm_locals = locals.clone();
                    if !self.static_pattern_matches(&arm.pattern, &value, &mut arm_locals)? {
                        continue;
                    }
                    if let Some(guard) = &arm.guard {
                        let guard = self
                            .evaluate_static_body(
                                guard,
                                Some(&Ty::Bool),
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
                    selected = Some(self.evaluate_static_body(
                        &arm.body,
                        expected,
                        &mut arm_locals,
                        fuel,
                        active_calls,
                    )?);
                    break;
                }
                selected.ok_or_else(|| "non-exhaustive match during ctfe".to_owned())?
            }
            _ => {
                return Err(
                    "expression is not in the pure ctfe subset (no mutation, borrowing, loops, handlers, or closures)"
                        .to_owned(),
                );
            }
        };
        if let Some(expected) = expected {
            Self::expect_static_type(value, expected, "ctfe expression")
        } else {
            Ok(value)
        }
    }

    fn static_binary_operand_type(
        &self,
        left: &Expr,
        operator: BinaryOp,
        right: &Expr,
        expected: Option<&Ty>,
        locals: &HashMap<String, CtfeValue>,
    ) -> Option<Ty> {
        match operator {
            BinaryOp::And | BinaryOp::Or => Some(Ty::Bool),
            BinaryOp::Eq
            | BinaryOp::Ne
            | BinaryOp::Lt
            | BinaryOp::Le
            | BinaryOp::Gt
            | BinaryOp::Ge => self
                .static_expression_type_hint(left, locals)
                .or_else(|| self.static_expression_type_hint(right, locals))
                .or(Some(Ty::I32)),
            _ => expected
                .filter(|ty| ty.is_integer())
                .cloned()
                .or_else(|| self.static_expression_type_hint(left, locals))
                .or_else(|| self.static_expression_type_hint(right, locals))
                .filter(Ty::is_integer)
                .or(Some(Ty::I32)),
        }
    }

    fn static_expression_type_hint(
        &self,
        expression: &Expr,
        locals: &HashMap<String, CtfeValue>,
    ) -> Option<Ty> {
        match expression.unlocated() {
            Expr::Name(name) => locals.get(name).map(|value| value.ty.clone()),
            Expr::Bool(_) => Some(Ty::Bool),
            Expr::Unit => Some(Ty::Unit),
            Expr::Tuple(fields) => Some(Ty::Tuple(
                fields
                    .iter()
                    .map(|field| self.static_expression_type_hint(field, locals))
                    .collect::<Option<Vec<_>>>()?,
            )),
            Expr::Array(elements) => {
                let element = elements
                    .first()
                    .and_then(|element| self.static_expression_type_hint(element, locals))?;
                Some(Ty::Array(Box::new(element), elements.len() as u64))
            }
            Expr::Member(base, member) => match self.static_expression_type_hint(base, locals)? {
                Ty::Tuple(fields) => fields.get(member.parse::<usize>().ok()?).cloned(),
                Ty::Struct(name) => self
                    .struct_layouts
                    .get(&name)?
                    .fields
                    .iter()
                    .find(|field| field.name == *member)
                    .map(|field| field.ty.clone()),
                _ => None,
            },
            Expr::Index { base, .. } => {
                let Ty::Array(element, _) = self.static_expression_type_hint(base, locals)? else {
                    return None;
                };
                Some(*element)
            }
            Expr::Unary(UnaryOp::Not, _) => Some(Ty::Bool),
            Expr::Unary(UnaryOp::Neg, operand) => self.static_expression_type_hint(operand, locals),
            Expr::Binary(left, operator, right) => match operator {
                BinaryOp::Eq
                | BinaryOp::Ne
                | BinaryOp::Lt
                | BinaryOp::Le
                | BinaryOp::Gt
                | BinaryOp::Ge
                | BinaryOp::And
                | BinaryOp::Or => Some(Ty::Bool),
                _ => self
                    .static_expression_type_hint(left, locals)
                    .or_else(|| self.static_expression_type_hint(right, locals)),
            },
            Expr::Call(callee, _) => {
                let Expr::Name(name) = callee.unlocated() else {
                    return None;
                };
                let function = self
                    .functions
                    .get(name)
                    .or_else(|| self.function_templates.get(name))?;
                function
                    .return_type
                    .as_ref()
                    .and_then(|source| self.static_value_type(source))
            }
            Expr::StructLiteral { constructor, .. } => self
                .static_struct_constructor_name(constructor, None)
                .ok()
                .map(Ty::Struct),
            Expr::Block(_, Some(tail)) => self.static_expression_type_hint(tail, locals),
            Expr::If {
                then_branch,
                else_branch,
                ..
            } => self
                .static_expression_type_hint(then_branch, locals)
                .or_else(|| {
                    else_branch
                        .as_deref()
                        .and_then(|branch| self.static_expression_type_hint(branch, locals))
                }),
            _ => None,
        }
    }

    fn static_pattern_matches(
        &self,
        pattern: &Pattern,
        value: &CtfeValue,
        locals: &mut HashMap<String, CtfeValue>,
    ) -> Result<bool, String> {
        match pattern {
            Pattern::Bool(expected) if value.ty == Ty::Bool => {
                Ok(value.bool_value() == Some(*expected))
            }
            Pattern::Integer(expected) if value.ty.is_integer() => {
                match CtfeValue::integer_literal(
                    value.ty.clone(),
                    expected.magnitude,
                    expected.negative,
                ) {
                    Ok(expected) => Ok(&expected == value),
                    Err(_) => Ok(false),
                }
            }
            Pattern::Tuple(fields) => match (&value.kind, &value.ty) {
                (CtfeValueKind::Tuple(values), Ty::Tuple(types))
                    if fields.len() == values.len() && values.len() == types.len() =>
                {
                    for (field, value) in fields.iter().zip(values) {
                        if !self.static_pattern_matches(field, value, locals)? {
                            return Ok(false);
                        }
                    }
                    Ok(true)
                }
                _ => Err(format!(
                    "tuple pattern cannot match ctfe value of type `{}`",
                    value.ty
                )),
            },
            Pattern::Wildcard => Ok(true),
            Pattern::Binding(name) => {
                locals.insert(name.clone(), value.clone());
                Ok(true)
            }
            Pattern::Constructor { path, fields } => {
                let (
                    Ty::Struct(name),
                    CtfeValueKind::Struct {
                        name: value_name,
                        fields: values,
                    },
                ) = (&value.ty, &value.kind)
                else {
                    return Err(format!(
                        "struct pattern cannot match ctfe value of type `{}`",
                        value.ty
                    ));
                };
                if name != value_name {
                    return Err("malformed ctfe struct value has inconsistent identity".to_owned());
                }
                let layout = self
                    .struct_layouts
                    .get(name)
                    .ok_or_else(|| format!("unknown struct `{name}` during ctfe"))?;
                let template_name = self
                    .nominal_instances
                    .get(name)
                    .map(|instance| instance.key.template.as_str())
                    .unwrap_or(name);
                if path.last().is_none_or(|candidate| {
                    candidate != name
                        && candidate != &layout.source_name
                        && candidate != template_name
                }) {
                    return Err(format!(
                        "pattern type mismatch: expected struct `{}`, found `{}`",
                        layout.source_name,
                        path.join(".")
                    ));
                }
                let patterns = match fields {
                    PatternFields::Unit => {
                        if !layout.fields.is_empty() {
                            return Err(format!(
                                "ctfe struct pattern `{}` requires {} fields",
                                layout.source_name,
                                layout.fields.len()
                            ));
                        }
                        Vec::new()
                    }
                    PatternFields::Positional(patterns) => {
                        if patterns.len() != layout.fields.len() {
                            return Err(format!(
                                "ctfe struct pattern `{}` requires {} fields, found {}",
                                layout.source_name,
                                layout.fields.len(),
                                patterns.len()
                            ));
                        }
                        patterns.iter().enumerate().collect::<Vec<_>>()
                    }
                    PatternFields::Named(patterns) => {
                        if patterns.len() != layout.fields.len() {
                            return Err(format!(
                                "ctfe struct pattern `{}` requires {} fields, found {}",
                                layout.source_name,
                                layout.fields.len(),
                                patterns.len()
                            ));
                        }
                        let mut seen = HashSet::new();
                        let mut ordered = Vec::with_capacity(patterns.len());
                        for field in patterns {
                            let index = layout
                                .fields
                                .iter()
                                .position(|candidate| candidate.name == field.name)
                                .ok_or_else(|| {
                                    format!(
                                        "unknown field `{}` in ctfe struct pattern `{}`",
                                        field.name, layout.source_name
                                    )
                                })?;
                            if !seen.insert(index) {
                                return Err(format!(
                                    "duplicate field `{}` in ctfe struct pattern `{}`",
                                    field.name, layout.source_name
                                ));
                            }
                            ordered.push((index, &field.pattern));
                        }
                        ordered
                    }
                };
                if values.len() != layout.fields.len() {
                    return Err("malformed ctfe struct value has invalid field count".to_owned());
                }
                for (index, pattern) in patterns {
                    if !self.static_pattern_matches(pattern, &values[index], locals)? {
                        return Ok(false);
                    }
                }
                Ok(true)
            }
            Pattern::Integer(_) | Pattern::Bool(_) => {
                Err("pattern type does not match the ctfe scrutinee".to_owned())
            }
        }
    }

    fn evaluate_static_unary(operator: UnaryOp, operand: CtfeValue) -> Result<CtfeValue, String> {
        match operator {
            UnaryOp::Not if operand.ty == Ty::Bool => Ok(CtfeValue::bool(
                !operand.bool_value().expect("bool value has bool payload"),
            )),
            UnaryOp::Neg if operand.ty.is_integer() => operand
                .checked_integer_neg()
                .map_err(|error| Self::static_integer_error(error, &operand.ty)),
            UnaryOp::Deref => Err("dereference is not permitted during ctfe".to_owned()),
            _ => Err("invalid unary operand during ctfe".to_owned()),
        }
    }

    fn evaluate_static_binary(
        left: CtfeValue,
        operator: BinaryOp,
        right: CtfeValue,
    ) -> Result<CtfeValue, String> {
        if left.ty.is_integer() && right.ty.is_integer() {
            return left
                .checked_integer_binary(operator, &right)
                .map_err(|error| Self::static_integer_error(error, &left.ty));
        }
        if left.ty == Ty::Bool && right.ty == Ty::Bool {
            let left = left.bool_value().expect("bool value has bool payload");
            let right = right.bool_value().expect("bool value has bool payload");
            return match operator {
                BinaryOp::Eq => Ok(CtfeValue::bool(left == right)),
                BinaryOp::Ne => Ok(CtfeValue::bool(left != right)),
                BinaryOp::And => Ok(CtfeValue::bool(left && right)),
                BinaryOp::Or => Ok(CtfeValue::bool(left || right)),
                _ => Err("invalid operand types in ctfe expression".to_owned()),
            };
        }
        if left.ty == Ty::Unit && right.ty == Ty::Unit {
            return match operator {
                BinaryOp::Eq => Ok(CtfeValue::bool(true)),
                BinaryOp::Ne => Ok(CtfeValue::bool(false)),
                _ => Err("invalid operand types in ctfe expression".to_owned()),
            };
        }
        if left.ty == right.ty
            && matches!(
                &left.kind,
                CtfeValueKind::Tuple(_) | CtfeValueKind::Array(_) | CtfeValueKind::Struct { .. }
            )
        {
            return match operator {
                BinaryOp::Eq => Ok(CtfeValue::bool(left == right)),
                BinaryOp::Ne => Ok(CtfeValue::bool(left != right)),
                _ => Err("invalid composite operator during ctfe".to_owned()),
            };
        }
        Err(format!(
            "ctfe operator requires compatible value types, found `{}` and `{}`",
            left.ty, right.ty
        ))
    }

    fn static_integer_error(error: IntegerEvalError, ty: &Ty) -> String {
        match error {
            IntegerEvalError::TypeMismatch => {
                "integer operands have different types during ctfe".to_owned()
            }
            IntegerEvalError::InvalidOperator => {
                format!("invalid integer operator for `{ty}` during ctfe")
            }
            IntegerEvalError::InvalidNegation => {
                format!("negation is not defined for `{ty}` during ctfe")
            }
            IntegerEvalError::Overflow => {
                format!("integer arithmetic overflows `{ty}` during ctfe")
            }
            IntegerEvalError::DivisionByZero => "division by zero during ctfe".to_owned(),
            IntegerEvalError::RemainderByZero => "remainder by zero during ctfe".to_owned(),
            IntegerEvalError::InvalidShift { count, width } => format!(
                "shift count `{count}` is out of range for `{ty}` ({width}-bit) during ctfe"
            ),
        }
    }

    fn expect_static_type(
        value: CtfeValue,
        expected: &Ty,
        context: &str,
    ) -> Result<CtfeValue, String> {
        if value.ty == *expected {
            Ok(value)
        } else {
            Err(format!(
                "{context} has type `{}`, expected `{expected}`",
                value.ty
            ))
        }
    }

    fn static_struct_constructor_name(
        &self,
        constructor: &Expr,
        expected: Option<&Ty>,
    ) -> Result<String, String> {
        let mut groups = Vec::new();
        let root = flatten_call(constructor, &mut groups);
        let Expr::Name(source_name) = root.unlocated() else {
            return Err("ctfe struct literal requires a named constructor".to_owned());
        };
        if let Some(Ty::Struct(expected_name)) = expected {
            let layout = self
                .struct_layouts
                .get(expected_name)
                .ok_or_else(|| format!("unknown struct `{expected_name}` during ctfe"))?;
            let template_name = self
                .nominal_instances
                .get(expected_name)
                .map(|instance| instance.key.template.as_str())
                .unwrap_or(expected_name);
            if source_name != expected_name
                && source_name != &layout.source_name
                && source_name != template_name
            {
                return Err(format!(
                    "ctfe struct constructor `{source_name}` does not construct `{}`",
                    layout.source_name
                ));
            }
            return Ok(expected_name.clone());
        }
        if let Some(expected) = expected {
            return Err(format!(
                "ctfe struct literal cannot be used where `{}` is expected",
                expected
            ));
        }
        if groups.is_empty() && self.struct_layouts.contains_key(source_name) {
            return Ok(source_name.clone());
        }
        let template = self
            .struct_templates
            .get(source_name)
            .ok_or_else(|| format!("unknown struct `{source_name}` during ctfe"))?;
        if groups.len() != template.compile_groups.len()
            || groups
                .iter()
                .zip(&template.compile_groups)
                .any(|(arguments, parameters)| arguments.len() != parameters.len())
        {
            return Err(format!(
                "ctfe struct constructor `{source_name}` has invalid compile-time arguments"
            ));
        }
        let mut source_arguments = Vec::new();
        for (arguments, parameters) in groups.iter().zip(&template.compile_groups) {
            for (argument, parameter) in arguments.iter().zip(parameters) {
                let source = self
                    .probe_compile_argument_source(parameter, &argument.value, &HashMap::new())
                    .ok_or_else(|| {
                        format!(
                            "ctfe struct constructor `{source_name}` has an unsupported compile-time argument"
                        )
                    })?;
                source_arguments.push(source);
            }
        }
        match self.probe_source_ty(&Type::Named(source_name.clone(), source_arguments)) {
            Some(Ty::Struct(name)) if self.struct_layouts.contains_key(&name) => Ok(name),
            _ => Err(format!(
                "concrete struct `{source_name}` was not materialized before ctfe"
            )),
        }
    }

    fn static_value_type(&self, source: &Type) -> Option<Ty> {
        self.probe_source_ty(source)
    }

    fn source_type_name(&self, source: &Type) -> String {
        self.static_value_type(source)
            .map(|ty| ty.to_string())
            .unwrap_or_else(|| format!("{source:?}"))
    }

    fn validate_static_value_type(&self, ty: &Ty) -> Result<(), String> {
        fn visit(
            analyzer: &Analyzer,
            ty: &Ty,
            depth: usize,
            nodes: &mut usize,
            visiting: &mut HashSet<String>,
        ) -> Result<(), String> {
            *nodes = nodes
                .checked_add(1)
                .ok_or_else(|| "ctfe value node count overflowed".to_owned())?;
            if *nodes > MAX_CTFE_AGGREGATE_ELEMENTS {
                return Err(format!(
                    "ctfe value exceeds the {MAX_CTFE_AGGREGATE_ELEMENTS}-node limit"
                ));
            }
            match ty {
                Ty::Tuple(fields) => {
                    if depth >= MAX_CTFE_VALUE_NESTING {
                        return Err(format!(
                            "ctfe value exceeds the {MAX_CTFE_VALUE_NESTING}-level nesting limit"
                        ));
                    }
                    if fields.len() > MAX_CTFE_AGGREGATE_ELEMENTS {
                        return Err(format!(
                            "ctfe tuple exceeds the {MAX_CTFE_AGGREGATE_ELEMENTS}-element limit"
                        ));
                    }
                    for field in fields {
                        visit(analyzer, field, depth + 1, nodes, visiting)?;
                    }
                }
                Ty::Array(element, length) => {
                    if depth >= MAX_CTFE_VALUE_NESTING {
                        return Err(format!(
                            "ctfe value exceeds the {MAX_CTFE_VALUE_NESTING}-level nesting limit"
                        ));
                    }
                    let length = usize::try_from(*length).map_err(|_| {
                        format!(
                            "ctfe array exceeds the {MAX_CTFE_AGGREGATE_ELEMENTS}-element limit"
                        )
                    })?;
                    if length > MAX_CTFE_AGGREGATE_ELEMENTS {
                        return Err(format!(
                            "ctfe array exceeds the {MAX_CTFE_AGGREGATE_ELEMENTS}-element limit"
                        ));
                    }
                    for _ in 0..length {
                        visit(analyzer, element, depth + 1, nodes, visiting)?;
                    }
                }
                Ty::Struct(name) => {
                    if depth >= MAX_CTFE_VALUE_NESTING {
                        return Err(format!(
                            "ctfe value exceeds the {MAX_CTFE_VALUE_NESTING}-level nesting limit"
                        ));
                    }
                    if analyzer.type_has_custom_drop(ty) {
                        return Err(format!(
                            "ctfe value type `{ty}` implements `droppable` and requires runtime destruction"
                        ));
                    }
                    if !visiting.insert(name.clone()) {
                        return Err(format!(
                            "ctfe value type `{ty}` has a recursive nominal layout"
                        ));
                    }
                    let layout = analyzer
                        .struct_layouts
                        .get(name)
                        .ok_or_else(|| format!("ctfe value references unknown struct `{name}`"))?;
                    if layout.fields.len() > MAX_CTFE_AGGREGATE_ELEMENTS {
                        return Err(format!(
                            "ctfe struct exceeds the {MAX_CTFE_AGGREGATE_ELEMENTS}-field limit"
                        ));
                    }
                    for field in &layout.fields {
                        visit(analyzer, &field.ty, depth + 1, nodes, visiting)?;
                    }
                    visiting.remove(name);
                }
                Ty::I8
                | Ty::I16
                | Ty::I32
                | Ty::I64
                | Ty::I128
                | Ty::ISize
                | Ty::U8
                | Ty::U16
                | Ty::U32
                | Ty::U64
                | Ty::U128
                | Ty::USize
                | Ty::Bool
                | Ty::Unit => {}
                Ty::Enum(_) => {
                    return Err(format!(
                        "ctfe enum value type `{ty}` is not supported until CTFE-6"
                    ));
                }
                Ty::Pointer { .. }
                | Ty::Reference { .. }
                | Ty::Slice(_)
                | Ty::Function(_)
                | Ty::Callable(_)
                | Ty::EffectRow { .. }
                | Ty::Continuation { .. }
                | Ty::EffectCallable { .. } => {
                    return Err(format!(
                        "ctfe value type `{ty}` depends on runtime storage or an address"
                    ));
                }
                Ty::Never | Ty::Error => {
                    return Err(format!("`{ty}` is not a materializable ctfe value type"));
                }
            }
            Ok(())
        }

        let mut nodes = 0;
        visit(self, ty, 0, &mut nodes, &mut HashSet::new())
    }

    fn consume_static_fuel(fuel: &mut usize) -> Result<(), String> {
        *fuel = fuel
            .checked_sub(1)
            .ok_or_else(|| "evaluation exceeded the 1024-step limit".to_owned())?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use crate::ast::Program;

    use super::{Analyzer, Ty, MAX_CTFE_AGGREGATE_ELEMENTS, MAX_CTFE_VALUE_NESTING};

    #[test]
    fn aggregate_type_limits_are_checked_before_ctfe_construction() {
        let analyzer = Analyzer::new(&Program::new(Vec::new()));
        let oversized = Ty::Array(Box::new(Ty::U8), (MAX_CTFE_AGGREGATE_ELEMENTS as u64) + 1);
        assert!(analyzer
            .validate_static_value_type(&oversized)
            .unwrap_err()
            .contains("element limit"));

        let mut nested = Ty::U8;
        for _ in 0..=MAX_CTFE_VALUE_NESTING {
            nested = Ty::Tuple(vec![nested]);
        }
        assert!(analyzer
            .validate_static_value_type(&nested)
            .unwrap_err()
            .contains("nesting limit"));
    }
}

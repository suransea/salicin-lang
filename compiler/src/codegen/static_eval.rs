use std::collections::{HashMap, HashSet};

use crate::ast::{
    BinaryOp, CallArg, Expr, Function, PassMode, Pattern, PatternFields, StaticExpr, Stmt, Type,
    UnaryOp,
};
use crate::core::LangItemKind;

use super::ctfe_value::{CtfeValue, CtfeValueKind, IntegerEvalError};
use super::hir::{LayoutQueryKind, Ty};
use super::lower::{flatten_call, InferredTypeArgument};
use super::target::NATIVE_TARGET;
use super::Analyzer;

const STATIC_EVALUATION_FUEL: usize = 16_384;
const MAX_CTFE_ACTIVE_CALLS: usize = 128;
const MAX_CTFE_AGGREGATE_ELEMENTS: usize = 65_536;
const MAX_CTFE_VALUE_NESTING: usize = 64;

enum StaticFunctionFlow {
    Value(CtfeValue),
    Return(CtfeValue),
}

impl Analyzer {
    pub(super) fn evaluate_static_globals(&mut self) {
        for name in self.collection.global_order.clone() {
            if self.lowering.ctfe_global_values.contains_key(&name) {
                continue;
            }
            let mut fuel = STATIC_EVALUATION_FUEL;
            let mut active_calls = Vec::new();
            if let Err(message) = self.evaluate_static_global(&name, &mut fuel, &mut active_calls) {
                self.error(format!(
                    "global constant `{name}` evaluation failed: {message}"
                ));
            }
        }
    }

    fn evaluate_static_global(
        &mut self,
        name: &str,
        fuel: &mut usize,
        active_calls: &mut Vec<(String, Vec<CtfeValue>)>,
    ) -> Result<CtfeValue, String> {
        if let Some(value) = self.lowering.ctfe_global_values.get(name) {
            return Ok(value.clone());
        }
        if !self.lowering.ctfe_active_globals.insert(name.to_owned()) {
            return Err(format!("cyclic global constant involving `{name}`"));
        }
        let result = (|| {
            let binding = self
                .collection
                .globals
                .get(name)
                .cloned()
                .ok_or_else(|| format!("unknown global constant `{name}`"))?;
            let expected = self
                .lowering
                .hir_globals
                .get(name)
                .map(|global| global.ty.clone())
                .ok_or_else(|| format!("global constant `{name}` was not typed"))?;
            self.validate_static_value_type(&expected)?;
            let value = self.evaluate_static_body(
                &binding.value,
                Some(&expected),
                &mut HashMap::new(),
                fuel,
                active_calls,
            )?;
            Self::expect_static_type(value, &expected, &format!("global constant `{name}`"))
        })();
        self.lowering.ctfe_active_globals.remove(name);
        if let Ok(value) = &result {
            self.lowering
                .ctfe_global_values
                .insert(name.to_owned(), value.clone());
        }
        result
    }

    pub(super) fn evaluate_static_usize(&mut self, expression: &StaticExpr) -> Option<u64> {
        let mut fuel = STATIC_EVALUATION_FUEL;
        let mut active_calls = Vec::new();
        match self.evaluate_static_expression(
            expression,
            Some(&Ty::USize),
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
        &mut self,
        expression: &StaticExpr,
        expected: Option<&Ty>,
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
                    self.evaluate_static_expression(operand, expected, locals, fuel, active_calls)?;
                Self::evaluate_static_unary(*operator, operand)
            }
            StaticExpr::Binary(left, operator, right) => {
                let operand_expected = (!matches!(
                    operator,
                    BinaryOp::Eq
                        | BinaryOp::Ne
                        | BinaryOp::Lt
                        | BinaryOp::Le
                        | BinaryOp::Gt
                        | BinaryOp::Ge
                        | BinaryOp::And
                        | BinaryOp::Or
                ))
                .then_some(expected)
                .flatten();
                let left = self.evaluate_static_expression(
                    left,
                    operand_expected,
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
                let right = self.evaluate_static_expression(
                    right,
                    operand_expected.or(Some(&left.ty)),
                    locals,
                    fuel,
                    active_calls,
                )?;
                Self::evaluate_static_binary(left, *operator, right)
            }
            StaticExpr::Call { function, groups } => {
                let expression =
                    groups
                        .iter()
                        .fold(Expr::Name(function.clone()), |callee, group| {
                            Expr::Call(
                                Box::new(callee),
                                group
                                    .iter()
                                    .map(|argument| CallArg {
                                        label: argument.label.clone(),
                                        value: Self::source_static_expression(&argument.value),
                                    })
                                    .collect(),
                            )
                        });
                self.evaluate_static_body(
                    &expression,
                    expected,
                    &mut locals.clone(),
                    fuel,
                    active_calls,
                )
            }
        }
    }

    fn source_static_expression(expression: &StaticExpr) -> Expr {
        match expression {
            StaticExpr::USize(value) => Expr::Integer(u128::from(*value)),
            StaticExpr::Bool(value) => Expr::Bool(*value),
            StaticExpr::Name(name) => Expr::Name(name.clone()),
            StaticExpr::Unary(operator, operand) => {
                Expr::Unary(*operator, Box::new(Self::source_static_expression(operand)))
            }
            StaticExpr::Binary(left, operator, right) => Expr::Binary(
                Box::new(Self::source_static_expression(left)),
                *operator,
                Box::new(Self::source_static_expression(right)),
            ),
            StaticExpr::Call { function, groups } => {
                groups
                    .iter()
                    .fold(Expr::Name(function.clone()), |callee, group| {
                        Expr::Call(
                            Box::new(callee),
                            group
                                .iter()
                                .map(|argument| CallArg {
                                    label: argument.label.clone(),
                                    value: Self::source_static_expression(&argument.value),
                                })
                                .collect(),
                        )
                    })
            }
        }
    }

    fn evaluate_static_call(
        &mut self,
        name: &str,
        arguments: &[CtfeValue],
        fuel: &mut usize,
        active_calls: &mut Vec<(String, Vec<CtfeValue>)>,
    ) -> Result<CtfeValue, String> {
        Self::consume_static_fuel(fuel)?;
        let display_name = self.diagnostic_function_name(name);
        if active_calls.len() >= MAX_CTFE_ACTIVE_CALLS {
            return Err(format!(
                "evaluation exceeded the {MAX_CTFE_ACTIVE_CALLS}-active-call limit"
            ));
        }
        let call = (name.to_owned(), arguments.to_vec());
        if active_calls.contains(&call) {
            return Err(format!(
                "recursive ctfe call `{display_name}` repeated with the same arguments"
            ));
        }
        let function = self
            .collection
            .functions
            .get(name)
            .or_else(|| self.collection.function_templates.get(name))
            .cloned()
            .ok_or_else(|| format!("unknown function `{display_name}` in static expression"))?;
        let result_ty = self.validate_static_function(&function, arguments)?;

        let parameters = function.groups.iter().flatten();
        let mut locals = parameters
            .zip(arguments)
            .map(|(parameter, value)| (parameter.name.clone(), value.clone()))
            .collect::<HashMap<_, _>>();
        active_calls.push(call);
        let result = self.evaluate_static_function_expression(
            function
                .body
                .as_ref()
                .expect("validated static function body"),
            Some(&result_ty),
            Some(&result_ty),
            &mut locals,
            fuel,
            active_calls,
        );
        active_calls.pop();
        let result = match result? {
            StaticFunctionFlow::Value(value) | StaticFunctionFlow::Return(value) => value,
        };
        Self::expect_static_type(result, &result_ty, "ctfe function result")
    }

    fn validate_static_function(
        &mut self,
        function: &Function,
        arguments: &[CtfeValue],
    ) -> Result<Ty, String> {
        let display_name = self.diagnostic_function_name(&function.name);
        if function.foreign.is_some() || function.builtin || function.body.is_none() {
            return Err(format!(
                "function `{}` has no source body available to ctfe",
                display_name
            ));
        }
        if !function.compile_groups.is_empty() {
            return Err(format!(
                "generic ctfe function `{}` is not supported yet",
                display_name
            ));
        }
        if function.groups.iter().map(Vec::len).sum::<usize>() != arguments.len() {
            return Err(format!(
                "ctfe function `{}` must be fully applied",
                display_name
            ));
        }
        if function.effects != Default::default() {
            return Err(format!(
                "effectful function `{}` cannot run during ctfe",
                display_name
            ));
        }
        for (parameter, argument) in function.groups.iter().flatten().zip(arguments) {
            if !matches!(
                parameter.mode,
                PassMode::Inferred | PassMode::Copy | PassMode::Move
            ) {
                return Err(format!(
                    "ctfe parameter `{}.{}` cannot borrow runtime storage",
                    display_name, parameter.name
                ));
            }
            let parameter_ty = self.static_value_type(&parameter.ty).ok_or_else(|| {
                format!(
                    "ctfe parameter `{}.{}` has unsupported type `{}`",
                    display_name,
                    parameter.name,
                    self.source_type_name(&parameter.ty)
                )
            })?;
            self.validate_static_value_type(&parameter_ty)?;
            if argument.ty != parameter_ty {
                return Err(format!(
                    "ctfe argument for `{}.{}` has type `{}`, expected `{parameter_ty}`",
                    display_name, parameter.name, argument.ty
                ));
            }
        }
        let result = if let Some(return_type) = function.return_type.as_ref() {
            self.static_value_type(return_type).ok_or_else(|| {
                format!(
                    "ctfe function `{}` has unsupported result type `{}`",
                    display_name,
                    self.source_type_name(return_type)
                )
            })?
        } else {
            self.lowering
                .signatures
                .get(&function.name)
                .and_then(|signature| signature.result.clone())
                .ok_or_else(|| {
                    format!(
                        "ctfe function `{}` has no concrete inferred result type",
                        display_name
                    )
                })?
        };
        self.validate_static_value_type(&result)?;
        Ok(result)
    }

    fn evaluate_static_body(
        &mut self,
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
            Expr::String(value) => {
                let ty = self
                    .string_ty()
                    .ok_or_else(|| "the core `string` type is unavailable".to_owned())?;
                self.lowering.string_literals.insert(value.clone());
                CtfeValue {
                    ty,
                    kind: CtfeValueKind::String(value.clone()),
                }
            }
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
                    .collection
                    .struct_layouts
                    .get(&name)
                    .cloned()
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
            Expr::Name(name) => {
                if let Some((enum_name, variant)) =
                    self.static_enum_variant(expression.unlocated(), expected, locals)?
                {
                    self.evaluate_static_enum_constructor(
                        &enum_name,
                        variant,
                        &[],
                        locals,
                        fuel,
                        active_calls,
                    )?
                } else {
                    if let Some(value) = locals.get(name).cloned() {
                        value
                    } else if self.collection.globals.contains_key(name) {
                        self.evaluate_static_global(name, fuel, active_calls)?
                    } else {
                        return Err(format!("unknown local or global `{name}` during ctfe"));
                    }
                }
            }
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
                if let Some((name, variant)) = self.static_enum_variant(callee, expected, locals)? {
                    self.evaluate_static_enum_constructor(
                        &name,
                        variant,
                        arguments,
                        locals,
                        fuel,
                        active_calls,
                    )?
                } else {
                    self.evaluate_static_source_call(
                        expression,
                        expected,
                        locals,
                        fuel,
                        active_calls,
                    )?
                }
            }
            Expr::Member(base, member) => {
                if let Some((name, variant)) =
                    self.static_enum_variant(expression.unlocated(), expected, locals)?
                {
                    self.evaluate_static_enum_constructor(
                        &name,
                        variant,
                        &[],
                        locals,
                        fuel,
                        active_calls,
                    )?
                } else {
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
                            let layout =
                                self.collection.struct_layouts.get(name).ok_or_else(|| {
                                    format!("unknown struct `{name}` during ctfe")
                                })?;
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

    fn evaluate_static_function_expression(
        &mut self,
        expression: &Expr,
        expected: Option<&Ty>,
        return_expected: Option<&Ty>,
        locals: &mut HashMap<String, CtfeValue>,
        fuel: &mut usize,
        active_calls: &mut Vec<(String, Vec<CtfeValue>)>,
    ) -> Result<StaticFunctionFlow, String> {
        Self::consume_static_fuel(fuel)?;
        match expression.unlocated() {
            Expr::Return(value) => {
                let value = match value.as_deref() {
                    Some(value) => self.evaluate_static_body(
                        value,
                        return_expected,
                        locals,
                        fuel,
                        active_calls,
                    )?,
                    None => CtfeValue::unit(),
                };
                Ok(StaticFunctionFlow::Return(value))
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
                            let flow = self.evaluate_static_function_expression(
                                &binding.value,
                                annotation.as_ref(),
                                return_expected,
                                &mut block_locals,
                                fuel,
                                active_calls,
                            )?;
                            let StaticFunctionFlow::Value(value) = flow else {
                                return Ok(flow);
                            };
                            block_locals.insert(binding.name.clone(), value);
                        }
                        Stmt::Let(_) => {
                            return Err("mutable bindings are not permitted during ctfe".to_owned());
                        }
                        Stmt::Expr(statement) => {
                            let flow = self.evaluate_static_function_expression(
                                statement,
                                None,
                                return_expected,
                                &mut block_locals,
                                fuel,
                                active_calls,
                            )?;
                            if matches!(flow, StaticFunctionFlow::Return(_)) {
                                return Ok(flow);
                            }
                        }
                    }
                }
                match tail.as_deref() {
                    Some(tail) => self.evaluate_static_function_expression(
                        tail,
                        expected,
                        return_expected,
                        &mut block_locals,
                        fuel,
                        active_calls,
                    ),
                    None => Ok(StaticFunctionFlow::Value(CtfeValue::unit())),
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
                    self.evaluate_static_function_expression(
                        then_branch,
                        expected,
                        return_expected,
                        locals,
                        fuel,
                        active_calls,
                    )
                } else if let Some(branch) = else_branch.as_deref() {
                    self.evaluate_static_function_expression(
                        branch,
                        expected,
                        return_expected,
                        locals,
                        fuel,
                        active_calls,
                    )
                } else {
                    Ok(StaticFunctionFlow::Value(CtfeValue::unit()))
                }
            }
            Expr::Match { scrutinee, arms } => {
                let value =
                    self.evaluate_static_body(scrutinee, None, locals, fuel, active_calls)?;
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
                    return self.evaluate_static_function_expression(
                        &arm.body,
                        expected,
                        return_expected,
                        &mut arm_locals,
                        fuel,
                        active_calls,
                    );
                }
                Err("non-exhaustive match during ctfe".to_owned())
            }
            _ => self
                .evaluate_static_body(expression, expected, locals, fuel, active_calls)
                .map(StaticFunctionFlow::Value),
        }
    }

    fn evaluate_static_source_call(
        &mut self,
        expression: &Expr,
        expected: Option<&Ty>,
        locals: &mut HashMap<String, CtfeValue>,
        fuel: &mut usize,
        active_calls: &mut Vec<(String, Vec<CtfeValue>)>,
    ) -> Result<CtfeValue, String> {
        let mut groups = Vec::new();
        let root = flatten_call(expression, &mut groups);
        if let Expr::Name(name) = root.unlocated() {
            let kind = if self.is_lang_item_name(name, LangItemKind::SizeOf) {
                Some(LayoutQueryKind::Size)
            } else if self.is_lang_item_name(name, LangItemKind::AlignOf) {
                Some(LayoutQueryKind::Align)
            } else {
                None
            };
            if let Some(kind) = kind {
                let [group] = groups.as_slice() else {
                    return Err(format!(
                        "ctfe layout query `{name}` requires one compile-time argument group"
                    ));
                };
                let [argument] = *group else {
                    return Err(format!(
                        "ctfe layout query `{name}` requires exactly one type argument"
                    ));
                };
                if argument.label.is_some() {
                    return Err(format!(
                        "ctfe layout query `{name}` does not accept a runtime argument label"
                    ));
                }
                let substitutions = active_calls
                    .last()
                    .and_then(|(function, _)| {
                        self.collection.function_type_substitutions.get(function)
                    })
                    .cloned()
                    .unwrap_or_default();
                let source = self
                    .probe_type_argument_source(&argument.value, &substitutions)
                    .ok_or_else(|| format!("ctfe layout query `{name}` expects a type argument"))?;
                let queried = self
                    .static_value_type(&source)
                    .ok_or_else(|| format!("ctfe layout query `{name}` has an invalid type"))?;
                let value = CtfeValue {
                    ty: Ty::U64,
                    kind: CtfeValueKind::LayoutQuery { queried, kind },
                };
                return if let Some(expected) = expected {
                    Self::expect_static_type(value, expected, "ctfe layout query")
                } else {
                    Ok(value)
                };
            }
        }
        if let Expr::Name(name) = root.unlocated() {
            if self.collection.function_templates.contains_key(name)
                && self
                    .collection
                    .functions
                    .get(name)
                    .is_none_or(|function| groups.len() != function.groups.len())
                && self
                    .collection
                    .function_templates
                    .get(name)
                    .is_some_and(|function| groups.len() == function.groups.len())
            {
                return self.evaluate_inferred_static_function_call(
                    name,
                    &groups,
                    expected,
                    locals,
                    fuel,
                    active_calls,
                );
            }
        }
        let (function_name, runtime_start, receiver) = match root.unlocated() {
            Expr::Name(name) => {
                let (function, runtime_start) =
                    self.resolve_static_named_function(name, &groups, 0)?;
                (function, runtime_start, None)
            }
            Expr::Member(base, member) => {
                if let Some(target) = self.static_nominal_type_head(base)? {
                    let overload_key = (target.clone(), member.clone(), false);
                    let function = if let Some(candidates) = self
                        .collection
                        .inherent_overloads
                        .get(&overload_key)
                        .cloned()
                    {
                        let matches = self.matching_function_overloads(&candidates, &groups, 0);
                        match matches.as_slice() {
                            [function] => function.clone(),
                            [] => {
                                return Err(format!(
                                    "no ctfe associated-function overload `{member}` matches the supplied labels"
                                ));
                            }
                            _ => {
                                return Err(format!(
                                    "ctfe associated-function overload `{member}` is ambiguous"
                                ));
                            }
                        }
                    } else {
                        let inherent = self
                            .collection
                            .inherent_members
                            .get(&target)
                            .and_then(|members| members.functions.get(member))
                            .cloned();
                        if let Some(inherent) = inherent {
                            inherent
                        } else {
                            let target_ty = if self.collection.struct_layouts.contains_key(&target)
                            {
                                Ty::Struct(target.clone())
                            } else {
                                Ty::Enum(target.clone())
                            };
                            let origin = active_calls
                                .last()
                                .and_then(|(function, _)| {
                                    self.collection.function_origins.get(function)
                                })
                                .cloned()
                                .or_else(|| self.current_origin.as_deref().cloned());
                            let candidates = origin
                                .as_ref()
                                .map(|origin| {
                                    self.trait_associated_function_candidates(
                                        &target_ty, member, origin,
                                    )
                                })
                                .unwrap_or_default();
                            let matches = self.matching_function_overloads(&candidates, &groups, 0);
                            match matches.as_slice() {
                                [function] => function.clone(),
                                [] => {
                                    return Err(format!(
                                        "no statically resolved ctfe associated function `{member}` exists for `{}`",
                                        self.diagnostic_type_name(&target_ty)
                                    ));
                                }
                                _ => {
                                    return Err(format!(
                                        "ctfe trait associated function `{member}` is ambiguous"
                                    ));
                                }
                            }
                        }
                    };
                    let (function, runtime_start) =
                        self.resolve_static_named_function(&function, &groups, 0)?;
                    (function, runtime_start, None)
                } else {
                    let receiver =
                        self.evaluate_static_body(base, None, locals, fuel, active_calls)?;
                    let target = match &receiver.ty {
                        Ty::Struct(name) | Ty::Enum(name) => name.clone(),
                        ty if ty.is_integer() => ty.to_string(),
                        _ => {
                            return Err(format!(
                            "ctfe method `{member}` requires an extendable receiver, found `{}`",
                            receiver.ty
                        ));
                        }
                    };
                    let overload_key = (target.clone(), member.clone(), true);
                    let function = if let Some(candidates) = self
                        .collection
                        .inherent_overloads
                        .get(&overload_key)
                        .cloned()
                    {
                        let matches = self.matching_function_overloads(&candidates, &groups, 1);
                        match matches.as_slice() {
                            [function] => function.clone(),
                            [] => {
                                return Err(format!(
                                "no ctfe method overload `{member}` matches the supplied labels"
                            ));
                            }
                            _ => {
                                return Err(format!(
                                    "ctfe method overload `{member}` is ambiguous"
                                ));
                            }
                        }
                    } else {
                        let inherent = self
                            .collection
                            .inherent_members
                            .get(&target)
                            .and_then(|members| members.methods.get(member))
                            .cloned();
                        if let Some(inherent) = inherent {
                            inherent
                        } else {
                            let origin = active_calls
                                .last()
                                .and_then(|(function, _)| {
                                    self.collection.function_origins.get(function)
                                })
                                .cloned()
                                .or_else(|| self.current_origin.as_deref().cloned());
                            let candidates = origin
                                .as_ref()
                                .map(|origin| {
                                    self.trait_method_function_candidates(
                                        &receiver.ty,
                                        member,
                                        origin,
                                    )
                                    .into_iter()
                                    .map(|(_, function)| function)
                                    .collect::<Vec<_>>()
                                })
                                .unwrap_or_default();
                            let matches = self.matching_function_overloads(&candidates, &groups, 1);
                            match matches.as_slice() {
                                [function] => function.clone(),
                                [] => {
                                    return Err(format!(
                                    "no statically resolved ctfe method `{member}` exists for `{}`",
                                    receiver.ty
                                ));
                                }
                                _ => {
                                    return Err(format!(
                                        "ctfe trait method `{member}` is ambiguous"
                                    ));
                                }
                            }
                        }
                    };
                    let (function, runtime_start) =
                        self.resolve_static_named_function(&function, &groups, 1)?;
                    (function, runtime_start, Some(receiver))
                }
            }
            _ => {
                return Err(
                    "ctfe calls require a named function or statically resolved method".to_owned(),
                );
            }
        };
        let display_name = self.diagnostic_function_name(&function_name);
        let runtime_groups = &groups[runtime_start..];
        if let Some((source, target)) = self
            .collection
            .integer_conversion_intrinsics
            .get(&function_name)
            .cloned()
        {
            let receiver = receiver.ok_or_else(|| {
                format!("ctfe integer conversion `{display_name}` requires a receiver")
            })?;
            if runtime_groups.iter().any(|group| !group.is_empty()) {
                return Err(format!(
                    "ctfe integer conversion `{display_name}` accepts no runtime arguments"
                ));
            }
            let option = self
                .lowering
                .signatures
                .get(&function_name)
                .and_then(|signature| signature.result.as_ref())
                .ok_or_else(|| {
                    format!("ctfe integer conversion `{display_name}` has no result type")
                })?
                .clone();
            let result =
                self.evaluate_checked_integer_conversion(receiver, &source, &target, &option)?;
            return if let Some(expected) = expected {
                Self::expect_static_type(
                    result,
                    expected,
                    &format!("ctfe call `{display_name}` result"),
                )
            } else {
                Ok(result)
            };
        }
        if let Some((source, target)) = self
            .collection
            .integer_magnitude_intrinsics
            .get(&function_name)
            .cloned()
        {
            let receiver = receiver.ok_or_else(|| {
                format!("ctfe integer magnitude `{display_name}` requires a receiver")
            })?;
            if runtime_groups.iter().any(|group| !group.is_empty()) {
                return Err(format!(
                    "ctfe integer magnitude `{display_name}` accepts no runtime arguments"
                ));
            }
            let receiver = Self::expect_static_type(
                receiver,
                &source,
                &format!("ctfe integer magnitude `{display_name}` receiver"),
            )?;
            let magnitude = receiver
                .signed_integer_value()
                .ok_or_else(|| format!("`{source}` has no signed magnitude intrinsic"))?
                .unsigned_abs();
            let result = CtfeValue::integer_bits(target, magnitude);
            return if let Some(expected) = expected {
                Self::expect_static_type(
                    result,
                    expected,
                    &format!("ctfe call `{display_name}` result"),
                )
            } else {
                Ok(result)
            };
        }
        let function = self
            .collection
            .functions
            .get(&function_name)
            .cloned()
            .ok_or_else(|| format!("unknown function `{display_name}` in static expression"))?;
        let arguments = self.evaluate_static_call_arguments(
            &function_name,
            &function,
            runtime_groups,
            receiver,
            locals,
            fuel,
            active_calls,
        )?;
        let result = self.evaluate_static_call(&function_name, &arguments, fuel, active_calls)?;
        if let Some(expected) = expected {
            Self::expect_static_type(
                result,
                expected,
                &format!("ctfe call `{display_name}` result"),
            )
        } else {
            Ok(result)
        }
    }

    fn evaluate_checked_integer_conversion(
        &self,
        value: CtfeValue,
        source: &Ty,
        target: &Ty,
        option: &Ty,
    ) -> Result<CtfeValue, String> {
        let value = Self::expect_static_type(value, source, "ctfe integer conversion receiver")?;
        let converted = if source.is_signed() {
            let signed = value
                .signed_integer_value()
                .ok_or_else(|| format!("`{source}` is not a signed integer"))?;
            if target.is_signed() {
                let minimum = NATIVE_TARGET
                    .signed_min(target)
                    .expect("signed integer target has a minimum");
                let maximum = NATIVE_TARGET
                    .signed_max(target)
                    .expect("signed integer target has a maximum");
                (minimum..=maximum)
                    .contains(&signed)
                    .then(|| CtfeValue::integer_bits(target.clone(), signed as u128))
            } else {
                u128::try_from(signed).ok().and_then(|unsigned| {
                    (unsigned <= NATIVE_TARGET.unsigned_max(target)?)
                        .then(|| CtfeValue::integer_bits(target.clone(), unsigned))
                })
            }
        } else {
            let unsigned = value
                .unsigned_integer_value()
                .ok_or_else(|| format!("`{source}` is not an unsigned integer"))?;
            let maximum = if target.is_signed() {
                NATIVE_TARGET
                    .signed_max(target)
                    .map(|maximum| maximum as u128)
            } else {
                NATIVE_TARGET.unsigned_max(target)
            };
            maximum
                .filter(|maximum| unsigned <= *maximum)
                .map(|_| CtfeValue::integer_bits(target.clone(), unsigned))
        };
        self.integer_conversion_option(option, target, converted)
    }

    fn integer_conversion_option(
        &self,
        option: &Ty,
        target: &Ty,
        converted: Option<CtfeValue>,
    ) -> Result<CtfeValue, String> {
        let Ty::Enum(name) = option else {
            return Err(format!(
                "checked integer conversion has non-option result `{option}`"
            ));
        };
        let layout = self
            .collection
            .enum_layouts
            .get(name)
            .ok_or_else(|| format!("missing `option({target})` layout during ctfe"))?;
        let variant_name = if converted.is_some() { "some" } else { "none" };
        let variant = layout
            .variants
            .iter()
            .position(|candidate| candidate.name == variant_name)
            .ok_or_else(|| format!("missing `{variant_name}` option variant during ctfe"))?;
        Ok(CtfeValue {
            ty: option.clone(),
            kind: CtfeValueKind::Enum {
                name: name.clone(),
                variant,
                fields: converted.into_iter().collect(),
            },
        })
    }

    fn evaluate_inferred_static_function_call(
        &mut self,
        name: &str,
        groups: &[&[CallArg]],
        expected: Option<&Ty>,
        locals: &mut HashMap<String, CtfeValue>,
        fuel: &mut usize,
        active_calls: &mut Vec<(String, Vec<CtfeValue>)>,
    ) -> Result<CtfeValue, String> {
        let template = self
            .collection
            .function_templates
            .get(name)
            .cloned()
            .ok_or_else(|| format!("unknown generic function `{name}` during ctfe"))?;
        let compile_parameters = template
            .compile_groups
            .iter()
            .flatten()
            .map(|parameter| parameter.name.clone())
            .collect::<HashSet<_>>();
        let mut inferred = HashMap::<String, InferredTypeArgument>::new();
        if let (Some(expected), Some(result)) = (expected, template.return_type.as_ref()) {
            self.unify_template_ty(
                result,
                expected,
                self.source_type_for_ty(expected).as_ref(),
                &compile_parameters,
                &mut inferred,
                "ctfe expected result type",
            )?;
        }
        let mut values = Vec::new();
        for (group_index, (arguments, parameters)) in
            groups.iter().zip(&template.groups).enumerate()
        {
            if arguments.len() != parameters.len() {
                return Err(format!(
                    "ctfe call to `{name}` group {} has {} arguments, expected {}",
                    group_index + 1,
                    arguments.len(),
                    parameters.len()
                ));
            }
            let labeled = arguments
                .first()
                .is_some_and(|argument| argument.label.is_some());
            let mut ordered = vec![None; parameters.len()];
            for (position, argument) in arguments.iter().enumerate() {
                let parameter_index = if let Some(label) = &argument.label {
                    parameters
                        .iter()
                        .position(|parameter| parameter.name == *label)
                        .ok_or_else(|| {
                            format!("unknown ctfe argument label `{label}` in call to `{name}`")
                        })?
                } else {
                    if labeled {
                        return Err(format!(
                            "ctfe call to `{name}` cannot mix labeled and positional arguments"
                        ));
                    }
                    position
                };
                let argument_expected = self.resolved_template_ty(
                    &parameters[parameter_index].ty,
                    &compile_parameters,
                    &inferred,
                );
                let value = self.evaluate_static_body(
                    &argument.value,
                    argument_expected.as_ref(),
                    locals,
                    fuel,
                    active_calls,
                )?;
                self.unify_template_ty(
                    &parameters[parameter_index].ty,
                    &value.ty,
                    self.source_type_for_ty(&value.ty).as_ref(),
                    &compile_parameters,
                    &mut inferred,
                    &format!(
                        "ctfe argument for parameter `{}`",
                        parameters[parameter_index].name
                    ),
                )?;
                if ordered[parameter_index].replace(value).is_some() {
                    return Err(format!(
                        "duplicate ctfe argument for `{}.{}`",
                        name, parameters[parameter_index].name
                    ));
                }
            }
            values.extend(
                ordered
                    .into_iter()
                    .enumerate()
                    .map(|(index, value)| {
                        value.ok_or_else(|| {
                            format!(
                                "missing ctfe argument for `{}.{}`",
                                name, parameters[index].name
                            )
                        })
                    })
                    .collect::<Result<Vec<_>, _>>()?,
            );
        }
        let ordered_parameters = template
            .compile_groups
            .iter()
            .flatten()
            .cloned()
            .collect::<Vec<_>>();
        let diagnostics = self.diagnostics.len();
        let (source_arguments, arguments) = self
            .finish_type_argument_inference(name, &ordered_parameters, &inferred, false)
            .filter(|_| self.diagnostics.len() == diagnostics)
            .ok_or_else(|| format!("could not infer generic ctfe call to `{name}`"))?;
        let canonical = self
            .ensure_function_instance(name, source_arguments, arguments)
            .filter(|_| self.diagnostics.len() == diagnostics)
            .ok_or_else(|| format!("could not instantiate generic ctfe function `{name}`"))?;
        let result = self.evaluate_static_call(&canonical, &values, fuel, active_calls)?;
        if let Some(expected) = expected {
            Self::expect_static_type(
                result,
                expected,
                &format!(
                    "ctfe call `{}` result",
                    self.diagnostic_function_name(&canonical)
                ),
            )
        } else {
            Ok(result)
        }
    }

    fn static_nominal_type_head(&mut self, expression: &Expr) -> Result<Option<String>, String> {
        let mut groups = Vec::new();
        let root = flatten_call(expression, &mut groups);
        let Expr::Name(name) = root.unlocated() else {
            return Ok(None);
        };
        if groups.is_empty()
            && (self.collection.struct_layouts.contains_key(name)
                || self.collection.enum_layouts.contains_key(name))
        {
            return Ok(Some(name.clone()));
        }
        let compile_groups = self
            .collection
            .struct_templates
            .get(name)
            .map(|template| template.compile_groups.clone())
            .or_else(|| {
                self.collection
                    .enum_templates
                    .get(name)
                    .map(|template| template.compile_groups.clone())
            });
        let Some(compile_groups) = compile_groups else {
            return Ok(None);
        };
        if groups.len() != compile_groups.len() {
            return Ok(None);
        }
        let mut source_arguments = Vec::new();
        for (arguments, parameters) in groups.iter().zip(&compile_groups) {
            let sources = self
                .probe_compile_group_sources(parameters, arguments, &HashMap::new())
                .ok_or_else(|| {
                    format!("ctfe nominal type `{name}` has invalid compile-time arguments")
                })?;
            source_arguments.extend(sources);
        }
        match self.lower_source_type(&Type::Named(name.clone(), source_arguments)) {
            Ty::Struct(name) | Ty::Enum(name) => Ok(Some(name)),
            _ => Err(format!(
                "concrete nominal type `{name}` was not materialized before ctfe"
            )),
        }
    }

    fn resolve_static_named_function(
        &mut self,
        name: &str,
        groups: &[&[CallArg]],
        parameter_offset: usize,
    ) -> Result<(String, usize), String> {
        let name = if let Some(candidates) = self.collection.function_overloads.get(name).cloned() {
            let matches = self.matching_function_overloads(&candidates, groups, parameter_offset);
            match matches.as_slice() {
                [function] => function.clone(),
                [] => {
                    return Err(format!(
                        "no ctfe function overload `{name}` matches the supplied labels"
                    ));
                }
                _ => return Err(format!("ctfe function overload `{name}` is ambiguous")),
            }
        } else {
            name.to_owned()
        };
        if let Some(function) = self.collection.functions.get(&name) {
            if groups.len() + parameter_offset == function.groups.len() {
                return Ok((name, 0));
            }
        }
        let template = self
            .collection
            .function_templates
            .get(&name)
            .cloned()
            .ok_or_else(|| format!("unknown function `{name}` in static expression"))?;
        let compile_count = template.compile_groups.len();
        if groups.len() + parameter_offset != compile_count + template.groups.len() {
            return Err(format!(
                "generic ctfe function `{name}` must have all compile-time and runtime parameter groups explicitly applied"
            ));
        }
        let mut source_arguments = Vec::new();
        let mut arguments = Vec::new();
        for (group, parameters) in groups[..compile_count].iter().zip(&template.compile_groups) {
            if group.len() != parameters.len() {
                return Err(format!(
                    "compile-time parameter group in ctfe call to `{name}` has {} arguments, expected {}",
                    group.len(),
                    parameters.len()
                ));
            }
            let sources = self
                .probe_compile_group_sources(parameters, group, &HashMap::new())
                .ok_or_else(|| {
                    format!("ctfe call to `{name}` has an invalid compile-time argument")
                })?;
            for (parameter, source) in parameters.iter().zip(sources) {
                let argument = self.probe_compile_argument_ty(parameter, &source).ok_or_else(|| {
                    format!(
                        "ctfe call to `{name}` has an invalid argument for compile-time parameter `{}`",
                        parameter.name
                    )
                })?;
                source_arguments.push(source);
                arguments.push(argument);
            }
        }
        let diagnostics = self.diagnostics.len();
        let canonical = self
            .ensure_function_instance(&name, source_arguments, arguments)
            .filter(|_| self.diagnostics.len() == diagnostics)
            .ok_or_else(|| format!("could not instantiate generic ctfe function `{name}`"))?;
        Ok((canonical, compile_count))
    }

    #[allow(clippy::too_many_arguments)]
    fn evaluate_static_call_arguments(
        &mut self,
        function_name: &str,
        function: &Function,
        groups: &[&[CallArg]],
        receiver: Option<CtfeValue>,
        locals: &mut HashMap<String, CtfeValue>,
        fuel: &mut usize,
        active_calls: &mut Vec<(String, Vec<CtfeValue>)>,
    ) -> Result<Vec<CtfeValue>, String> {
        let display_name = self.diagnostic_function_name(function_name);
        let parameter_offset = usize::from(receiver.is_some());
        if groups.len() + parameter_offset != function.groups.len() {
            return Err(format!(
                "ctfe function `{display_name}` must be fully applied: expected {} runtime groups, found {}",
                function.groups.len() - parameter_offset,
                groups.len()
            ));
        }
        let mut values = Vec::new();
        if let Some(receiver) = receiver {
            let [parameter] = function.groups[0].as_slice() else {
                return Err(format!(
                    "ctfe method `{display_name}` must have one receiver parameter"
                ));
            };
            let expected = self.static_value_type(&parameter.ty).ok_or_else(|| {
                format!(
                    "ctfe receiver `{display_name}.{}` has unsupported type `{}`",
                    parameter.name,
                    self.source_type_name(&parameter.ty)
                )
            })?;
            values.push(Self::expect_static_type(
                receiver,
                &expected,
                "ctfe method receiver",
            )?);
        }
        for (group_index, (arguments, parameters)) in groups
            .iter()
            .zip(&function.groups[parameter_offset..])
            .enumerate()
        {
            if arguments.len() != parameters.len() {
                return Err(format!(
                    "ctfe call to `{display_name}` group {} has {} arguments, expected {}",
                    group_index + 1,
                    arguments.len(),
                    parameters.len()
                ));
            }
            let labeled = arguments
                .first()
                .is_some_and(|argument| argument.label.is_some());
            if arguments
                .iter()
                .any(|argument| argument.label.is_some() != labeled)
            {
                return Err(format!(
                    "ctfe call to `{display_name}` cannot mix labeled and positional arguments"
                ));
            }
            let mut ordered = vec![None; parameters.len()];
            for (position, argument) in arguments.iter().enumerate() {
                let parameter_index = if let Some(label) = &argument.label {
                    parameters
                        .iter()
                        .position(|parameter| parameter.name == *label)
                        .ok_or_else(|| {
                            format!(
                                "unknown ctfe argument label `{label}` in call to `{display_name}`"
                            )
                        })?
                } else {
                    position
                };
                if ordered[parameter_index].is_some() {
                    return Err(format!(
                        "duplicate ctfe argument for `{}.{}`",
                        display_name, parameters[parameter_index].name
                    ));
                }
                let parameter = &parameters[parameter_index];
                let expected = self.static_value_type(&parameter.ty).ok_or_else(|| {
                    format!(
                        "ctfe parameter `{display_name}.{}` has unsupported type `{}`",
                        parameter.name,
                        self.source_type_name(&parameter.ty)
                    )
                })?;
                let value = self.evaluate_static_body(
                    &argument.value,
                    Some(&expected),
                    locals,
                    fuel,
                    active_calls,
                )?;
                ordered[parameter_index] = Some(value);
            }
            for (index, value) in ordered.into_iter().enumerate() {
                values.push(value.ok_or_else(|| {
                    format!(
                        "missing ctfe argument for `{}.{}`",
                        display_name, parameters[index].name
                    )
                })?);
            }
        }
        Ok(values)
    }

    fn static_binary_operand_type(
        &mut self,
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
        &mut self,
        expression: &Expr,
        locals: &HashMap<String, CtfeValue>,
    ) -> Option<Ty> {
        match expression.unlocated() {
            Expr::Name(name) => locals.get(name).map(|value| value.ty.clone()).or_else(|| {
                self.lowering
                    .hir_globals
                    .get(name)
                    .map(|global| global.ty.clone())
            }),
            Expr::Bool(_) | Expr::String(_) => Some(Ty::Bool),
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
            Expr::Member(base, member) => {
                if let Ok(Some((name, _))) =
                    self.static_enum_variant(expression.unlocated(), None, locals)
                {
                    return Some(Ty::Enum(name));
                }
                match self.static_expression_type_hint(base, locals)? {
                    Ty::Tuple(fields) => fields.get(member.parse::<usize>().ok()?).cloned(),
                    Ty::Struct(name) => self
                        .collection
                        .struct_layouts
                        .get(&name)?
                        .fields
                        .iter()
                        .find(|field| field.name == *member)
                        .map(|field| field.ty.clone()),
                    _ => None,
                }
            }
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
                if let Ok(Some((name, _))) = self.static_enum_variant(callee, None, locals) {
                    return Some(Ty::Enum(name));
                }
                let Expr::Name(name) = callee.unlocated() else {
                    return None;
                };
                let function = self
                    .collection
                    .functions
                    .get(name)
                    .or_else(|| self.collection.function_templates.get(name))?;
                let source = function.return_type.clone()?;
                self.static_value_type(&source)
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
        &mut self,
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
            Pattern::Binding(candidate)
                if matches!(
                    (&value.ty, &value.kind),
                    (Ty::Enum(_), CtfeValueKind::Enum { .. })
                ) =>
            {
                let (
                    Ty::Enum(name),
                    CtfeValueKind::Enum {
                        variant: active, ..
                    },
                ) = (&value.ty, &value.kind)
                else {
                    unreachable!("enum binding-pattern guard")
                };
                let layout = self
                    .collection
                    .enum_layouts
                    .get(name)
                    .ok_or_else(|| format!("unknown enum `{name}` during ctfe"))?;
                if let Some(variant) = layout
                    .variants
                    .iter()
                    .position(|variant| variant.name == *candidate && variant.fields.is_empty())
                {
                    Ok(*active == variant)
                } else {
                    locals.insert(candidate.clone(), value.clone());
                    Ok(true)
                }
            }
            Pattern::Binding(name) => {
                locals.insert(name.clone(), value.clone());
                Ok(true)
            }
            Pattern::Constructor { path, fields } => {
                if matches!(
                    (&value.ty, &value.kind),
                    (Ty::Enum(_), CtfeValueKind::Enum { .. })
                ) {
                    return self.static_enum_pattern_matches(path, fields, value, locals);
                }
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
                    .collection
                    .struct_layouts
                    .get(name)
                    .ok_or_else(|| format!("unknown struct `{name}` during ctfe"))?;
                let template_name = self
                    .collection
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

    fn static_enum_pattern_matches(
        &mut self,
        path: &[String],
        fields: &PatternFields,
        value: &CtfeValue,
        locals: &mut HashMap<String, CtfeValue>,
    ) -> Result<bool, String> {
        let (
            Ty::Enum(name),
            CtfeValueKind::Enum {
                name: value_name,
                variant: active,
                fields: values,
            },
        ) = (&value.ty, &value.kind)
        else {
            return Err(format!(
                "enum pattern cannot match ctfe value of type `{}`",
                value.ty
            ));
        };
        if name != value_name {
            return Err("malformed ctfe enum value has inconsistent identity".to_owned());
        }
        let layout = self
            .collection
            .enum_layouts
            .get(name)
            .ok_or_else(|| format!("unknown enum `{name}` during ctfe"))?;
        let variant_name = path
            .last()
            .ok_or_else(|| "empty enum constructor path during ctfe".to_owned())?;
        let template_name = self
            .collection
            .nominal_instances
            .get(name)
            .map(|instance| instance.key.template.as_str())
            .unwrap_or(name);
        if path.len() > 2
            || (path.len() == 2
                && path[0] != *name
                && path[0] != template_name
                && path[0] != "self")
        {
            return Err(format!(
                "ctfe pattern constructor `{}` does not belong to enum `{}`",
                path.join("."),
                self.diagnostic_type_name(&value.ty)
            ));
        }
        let variant = layout
            .variants
            .iter()
            .position(|variant| variant.name == *variant_name)
            .ok_or_else(|| {
                format!(
                    "unknown ctfe pattern variant `{variant_name}` for `{}`",
                    self.diagnostic_type_name(&value.ty)
                )
            })?;
        if variant != *active {
            return Ok(false);
        }
        let variant_layout = &layout.variants[variant];
        if values.len() != variant_layout.fields.len() {
            return Err("malformed ctfe enum value has invalid payload count".to_owned());
        }
        let patterns = match fields {
            PatternFields::Unit => {
                if !variant_layout.fields.is_empty() {
                    return Err(format!(
                        "ctfe enum pattern `{}.{variant_name}` requires {} fields",
                        self.diagnostic_type_name(&value.ty),
                        variant_layout.fields.len()
                    ));
                }
                Vec::new()
            }
            PatternFields::Positional(patterns) => {
                if patterns.len() != variant_layout.fields.len() {
                    return Err(format!(
                        "ctfe enum pattern `{}.{variant_name}` requires {} fields, found {}",
                        self.diagnostic_type_name(&value.ty),
                        variant_layout.fields.len(),
                        patterns.len()
                    ));
                }
                patterns.iter().enumerate().collect::<Vec<_>>()
            }
            PatternFields::Named(patterns) => {
                if !variant_layout.named {
                    return Err(format!(
                        "ctfe enum pattern `{}.{variant_name}` has positional fields",
                        self.diagnostic_type_name(&value.ty)
                    ));
                }
                if patterns.len() != variant_layout.fields.len() {
                    return Err(format!(
                        "ctfe enum pattern `{}.{variant_name}` requires {} fields, found {}",
                        self.diagnostic_type_name(&value.ty),
                        variant_layout.fields.len(),
                        patterns.len()
                    ));
                }
                let mut seen = HashSet::new();
                let mut ordered = Vec::with_capacity(patterns.len());
                for field in patterns {
                    let index = variant_layout
                        .fields
                        .iter()
                        .position(|candidate| candidate.name == field.name)
                        .ok_or_else(|| {
                            format!(
                                "unknown field `{}` in ctfe enum pattern `{}.{variant_name}`",
                                field.name,
                                self.diagnostic_type_name(&value.ty)
                            )
                        })?;
                    if !seen.insert(index) {
                        return Err(format!(
                            "duplicate field `{}` in ctfe enum pattern `{}.{variant_name}`",
                            field.name,
                            self.diagnostic_type_name(&value.ty)
                        ));
                    }
                    ordered.push((index, &field.pattern));
                }
                ordered
            }
        };
        for (index, pattern) in patterns {
            if !self.static_pattern_matches(pattern, &values[index], locals)? {
                return Ok(false);
            }
        }
        Ok(true)
    }

    fn evaluate_static_unary(operator: UnaryOp, operand: CtfeValue) -> Result<CtfeValue, String> {
        if matches!(&operand.kind, CtfeValueKind::LayoutQuery { .. }) {
            return Err(
                "target layout queries may only be standalone global constants in this version"
                    .to_owned(),
            );
        }
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
        if matches!(&left.kind, CtfeValueKind::LayoutQuery { .. })
            || matches!(&right.kind, CtfeValueKind::LayoutQuery { .. })
        {
            return Err(
                "target layout queries may only be standalone global constants in this version"
                    .to_owned(),
            );
        }
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
                CtfeValueKind::Tuple(_)
                    | CtfeValueKind::Array(_)
                    | CtfeValueKind::Struct { .. }
                    | CtfeValueKind::Enum { .. }
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

    fn static_enum_variant(
        &mut self,
        expression: &Expr,
        expected: Option<&Ty>,
        locals: &HashMap<String, CtfeValue>,
    ) -> Result<Option<(String, usize)>, String> {
        if let Expr::Name(variant_name) = expression.unlocated() {
            if locals.contains_key(variant_name) {
                return Ok(None);
            }
            let Some(Ty::Enum(name)) = expected else {
                return Ok(None);
            };
            let layout = self
                .collection
                .enum_layouts
                .get(name)
                .ok_or_else(|| format!("unknown enum `{name}` during ctfe"))?;
            return Ok(layout
                .variants
                .iter()
                .position(|variant| variant.name == *variant_name)
                .map(|variant| (name.clone(), variant)));
        }

        let Expr::Member(type_head, variant_name) = expression.unlocated() else {
            return Ok(None);
        };
        let mut groups = Vec::new();
        let root = flatten_call(type_head, &mut groups);
        let Expr::Name(source_name) = root.unlocated() else {
            return Ok(None);
        };
        if locals.contains_key(source_name) {
            return Ok(None);
        }

        let expected_name = match expected {
            Some(Ty::Enum(name)) => {
                let template_name = self
                    .collection
                    .nominal_instances
                    .get(name)
                    .map(|instance| instance.key.template.as_str())
                    .unwrap_or(name);
                (source_name == name || source_name == template_name).then_some(name.clone())
            }
            _ => None,
        };
        let name = if let Some(name) = expected_name {
            name
        } else if groups.is_empty() && self.collection.enum_layouts.contains_key(source_name) {
            source_name.clone()
        } else {
            let Some(template) = self.collection.enum_templates.get(source_name) else {
                return Ok(None);
            };
            if groups.len() != template.compile_groups.len()
                || groups
                    .iter()
                    .zip(&template.compile_groups)
                    .any(|(arguments, parameters)| arguments.len() != parameters.len())
            {
                return Err(format!(
                    "ctfe enum constructor `{source_name}` has invalid compile-time arguments"
                ));
            }
            let mut source_arguments = Vec::new();
            for (arguments, parameters) in groups.iter().zip(&template.compile_groups) {
                for (argument, parameter) in arguments.iter().zip(parameters) {
                    let source = self
                        .probe_compile_argument_source(parameter, &argument.value, &HashMap::new())
                        .ok_or_else(|| {
                            format!(
                                "ctfe enum constructor `{source_name}` has an unsupported compile-time argument"
                            )
                        })?;
                    source_arguments.push(source);
                }
            }
            match self.lower_source_type(&Type::Named(source_name.clone(), source_arguments)) {
                Ty::Enum(name) if self.collection.enum_layouts.contains_key(&name) => name,
                _ => {
                    return Err(format!(
                        "concrete enum `{source_name}` was not materialized before ctfe"
                    ));
                }
            }
        };
        let layout = self
            .collection
            .enum_layouts
            .get(&name)
            .ok_or_else(|| format!("unknown enum `{name}` during ctfe"))?;
        let variant = layout
            .variants
            .iter()
            .position(|variant| variant.name == *variant_name)
            .ok_or_else(|| {
                format!(
                    "unknown ctfe enum variant `{variant_name}` for `{}`",
                    self.diagnostic_type_name(&Ty::Enum(name.clone()))
                )
            })?;
        Ok(Some((name, variant)))
    }

    fn evaluate_static_enum_constructor(
        &mut self,
        name: &str,
        variant: usize,
        arguments: &[CallArg],
        locals: &mut HashMap<String, CtfeValue>,
        fuel: &mut usize,
        active_calls: &mut Vec<(String, Vec<CtfeValue>)>,
    ) -> Result<CtfeValue, String> {
        let ty = Ty::Enum(name.to_owned());
        self.validate_static_value_type(&ty)?;
        let layout = self
            .collection
            .enum_layouts
            .get(name)
            .ok_or_else(|| format!("unknown enum `{name}` during ctfe"))?;
        let variant_layout = layout
            .variants
            .get(variant)
            .cloned()
            .ok_or_else(|| format!("unknown variant index {variant} for enum `{name}`"))?;
        if arguments.len() != variant_layout.fields.len() {
            return Err(format!(
                "ctfe enum variant `{}.{}` expects {} fields, found {}",
                self.diagnostic_type_name(&ty),
                variant_layout.name,
                variant_layout.fields.len(),
                arguments.len()
            ));
        }
        let labeled = arguments
            .iter()
            .filter(|argument| argument.label.is_some())
            .count();
        if labeled != 0 && labeled != arguments.len() {
            return Err(format!(
                "ctfe enum variant `{}.{}` cannot mix labeled and positional fields",
                self.diagnostic_type_name(&ty),
                variant_layout.name
            ));
        }
        if labeled != 0 && !variant_layout.named {
            return Err(format!(
                "ctfe enum variant `{}.{}` has positional fields",
                self.diagnostic_type_name(&ty),
                variant_layout.name
            ));
        }
        let mut values = vec![None; variant_layout.fields.len()];
        for (source_index, argument) in arguments.iter().enumerate() {
            let index = match argument.label.as_deref() {
                Some(label) => variant_layout
                    .fields
                    .iter()
                    .position(|field| field.name == label)
                    .ok_or_else(|| {
                        format!(
                            "unknown field `{label}` in ctfe enum variant `{}.{}`",
                            self.diagnostic_type_name(&ty),
                            variant_layout.name
                        )
                    })?,
                None => source_index,
            };
            if values[index].is_some() {
                return Err(format!(
                    "duplicate field `{}` in ctfe enum variant `{}.{}`",
                    variant_layout.fields[index].name,
                    self.diagnostic_type_name(&ty),
                    variant_layout.name
                ));
            }
            let value = self.evaluate_static_body(
                &argument.value,
                Some(&variant_layout.fields[index].ty),
                locals,
                fuel,
                active_calls,
            )?;
            values[index] = Some(Self::expect_static_type(
                value,
                &variant_layout.fields[index].ty,
                &format!(
                    "ctfe enum variant field `{}`",
                    variant_layout.fields[index].name
                ),
            )?);
        }
        let values = values
            .into_iter()
            .enumerate()
            .map(|(index, value)| {
                value.ok_or_else(|| {
                    format!(
                        "missing field `{}` in ctfe enum variant `{}.{}`",
                        variant_layout.fields[index].name,
                        self.diagnostic_type_name(&ty),
                        variant_layout.name
                    )
                })
            })
            .collect::<Result<Vec<_>, _>>()?;
        Ok(CtfeValue {
            ty,
            kind: CtfeValueKind::Enum {
                name: name.to_owned(),
                variant,
                fields: values,
            },
        })
    }

    fn static_struct_constructor_name(
        &mut self,
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
                .collection
                .struct_layouts
                .get(expected_name)
                .ok_or_else(|| format!("unknown struct `{expected_name}` during ctfe"))?;
            let template_name = self
                .collection
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
        if groups.is_empty() && self.collection.struct_layouts.contains_key(source_name) {
            return Ok(source_name.clone());
        }
        let template = self
            .collection
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
        match self.lower_source_type(&Type::Named(source_name.clone(), source_arguments)) {
            Ty::Struct(name) if self.collection.struct_layouts.contains_key(&name) => Ok(name),
            _ => Err(format!(
                "concrete struct `{source_name}` was not materialized before ctfe"
            )),
        }
    }

    fn static_value_type(&mut self, source: &Type) -> Option<Ty> {
        let diagnostics = self.diagnostics.len();
        let ty = self.lower_source_type(source);
        (self.diagnostics.len() == diagnostics && ty != Ty::Error).then_some(ty)
    }

    fn source_type_name(&self, source: &Type) -> String {
        self.probe_source_ty(source)
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
                    if analyzer.string_ty().as_ref() == Some(ty) {
                        return Ok(());
                    }
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
                        .collection
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
                Ty::Enum(name) => {
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
                    let layout =
                        analyzer.collection.enum_layouts.get(name).ok_or_else(|| {
                            format!("ctfe value references unknown enum `{name}`")
                        })?;
                    if layout.variants.len() > MAX_CTFE_AGGREGATE_ELEMENTS {
                        return Err(format!(
                            "ctfe enum exceeds the {MAX_CTFE_AGGREGATE_ELEMENTS}-variant limit"
                        ));
                    }
                    for variant in &layout.variants {
                        if variant.fields.len() > MAX_CTFE_AGGREGATE_ELEMENTS {
                            return Err(format!(
                                "ctfe enum variant exceeds the {MAX_CTFE_AGGREGATE_ELEMENTS}-field limit"
                            ));
                        }
                        for field in &variant.fields {
                            visit(analyzer, &field.ty, depth + 1, nodes, visiting)?;
                        }
                    }
                    visiting.remove(name);
                }
                Ty::Pointer { .. }
                | Ty::Reference { .. }
                | Ty::Str
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
        *fuel = fuel.checked_sub(1).ok_or_else(|| {
            format!("evaluation exceeded the {STATIC_EVALUATION_FUEL}-step limit")
        })?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use crate::ast::Program;

    use super::{Analyzer, CtfeValue, Ty, MAX_CTFE_AGGREGATE_ELEMENTS, MAX_CTFE_VALUE_NESTING};

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

        let node_heavy = Ty::Tuple(vec![
            Ty::Array(Box::new(Ty::U8), 32_768),
            Ty::Array(Box::new(Ty::U8), 32_768),
        ]);
        assert!(analyzer
            .validate_static_value_type(&node_heavy)
            .unwrap_err()
            .contains("node limit"));
    }

    #[test]
    fn normalized_value_consumers_reject_exact_type_mismatches() {
        let error = Analyzer::expect_static_type(
            CtfeValue::bool(true),
            &Ty::USize,
            "dependent array length",
        )
        .unwrap_err();
        assert_eq!(
            error,
            "dependent array length has type `bool`, expected `usize`"
        );
    }
}

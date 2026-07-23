use std::collections::HashMap;

use crate::ast::{
    CallArg, CompileParam, CompileParamKind, Expr, FunctionEffects, Type, VariantFields,
};
use crate::core::LangItemKind;

use super::compile_time::{
    effect_identity_sources, effect_row_from_marker, effect_row_from_source, effect_row_source,
    is_compile_value_marker, source_effect_identity, type_constructor_from_marker,
    type_constructor_marker, ACCESS_MUT_MARKER, ACCESS_SHARED_MARKER, PASSING_AUTO_MARKER,
    PASSING_COPY_MARKER, PASSING_MOVE_MARKER,
};
use super::flow::LowerCtx;
use super::hir::{FunctionTy, Ty};
use super::lower::flatten_call;
use super::names::nominal_instance_name;
use super::registry::{NominalInstanceKey, NominalKind};
use super::source_rewrite::substitute_type_parameters;
use super::Analyzer;

impl Analyzer {
    pub(super) fn lower_source_type(&mut self, source: &Type) -> Ty {
        match source {
            Type::I32 => Ty::I32,
            Type::I64 => Ty::I64,
            Type::U32 => Ty::U32,
            Type::U64 => Ty::U64,
            Type::Bool => Ty::Bool,
            Type::Unit => Ty::Unit,
            Type::Function {
                groups,
                effects,
                result,
            } => {
                if !effects.parameters.is_empty() {
                    self.error(format!(
                        "unresolved effect parameter{} `{}` in function type",
                        if effects.parameters.len() == 1 {
                            ""
                        } else {
                            "s"
                        },
                        effects.parameters.join(", ")
                    ));
                    return Ty::Error;
                }
                Ty::Function(FunctionTy {
                    groups: groups
                        .iter()
                        .map(|group| group.iter().map(|ty| self.lower_source_type(ty)).collect())
                        .collect(),
                    unsafe_effect: self.function_effects_unsafe(effects),
                    throws_error: effects
                        .throws
                        .as_deref()
                        .map(|error| Box::new(self.lower_source_type(error))),
                    custom_effects: self.function_effects_custom_identities(effects),
                    result: Box::new(self.lower_source_type(result)),
                })
            }
            Type::Borrow {
                mutable,
                region,
                pointee,
                ..
            } => Ty::Reference {
                pointee: Box::new(self.lower_source_type(pointee)),
                mutable: *mutable,
                region: region.clone(),
            },
            Type::Array(element, length) => {
                let element = self.lower_source_type(element);
                if *length > i32::MAX as u64 {
                    self.error(format!(
                        "array length {length} exceeds the first-version limit of {}",
                        i32::MAX
                    ));
                    Ty::Error
                } else if element == Ty::Unit {
                    self.error("array element type `()` is not supported in the first version");
                    Ty::Error
                } else {
                    let array = Ty::Array(Box::new(element), *length);
                    self.array_types.insert(array.clone());
                    array
                }
            }
            Type::Named(name, arguments) if name == "()" && arguments.is_empty() => Ty::Unit,
            Type::Named(name, _) if effect_row_from_marker(name).is_some() => {
                let Some((unsafe_effect, throws_error, custom_effects)) =
                    effect_row_from_source(source)
                else {
                    self.error("effect row carries more than one thrown error type");
                    return Ty::Error;
                };
                Ty::EffectRow {
                    unsafe_effect,
                    throws_error: throws_error
                        .as_ref()
                        .map(|error| Box::new(self.lower_source_type(error))),
                    custom_effects,
                }
            }
            Type::Named(name, arguments)
                if arguments.is_empty() && is_compile_value_marker(name) =>
            {
                Ty::Struct(name.clone())
            }
            Type::Named(name, arguments) if matches!(name.as_str(), "Ptr" | "MutPtr") => {
                if arguments.len() != 1 {
                    self.error(format!("type `{name}` expects exactly one type argument"));
                    Ty::Error
                } else {
                    Ty::Pointer {
                        pointee: Box::new(self.lower_source_type(&arguments[0])),
                        mutable: name == "MutPtr",
                    }
                }
            }
            Type::Named(name, arguments)
                if name == self.lang_item_name(LangItemKind::Continuation) =>
            {
                if arguments.len() != 2 {
                    self.error("Continuation expects input and output type arguments");
                    Ty::Error
                } else {
                    Ty::Continuation {
                        input: Box::new(self.lower_source_type(&arguments[0])),
                        output: Box::new(self.lower_source_type(&arguments[1])),
                    }
                }
            }
            Type::Named(name, arguments)
                if name == self.lang_item_name(LangItemKind::EffectCallable) =>
            {
                if arguments.len() != 3 {
                    self.error("EffectCallable expects input, output, and answer type arguments");
                    Ty::Error
                } else {
                    Ty::EffectCallable {
                        input: Box::new(self.lower_source_type(&arguments[0])),
                        output: Box::new(self.lower_source_type(&arguments[1])),
                        answer: Box::new(self.lower_source_type(&arguments[2])),
                    }
                }
            }
            Type::Named(name, arguments)
                if arguments.is_empty() && self.abstract_type_parameters.contains_key(name) =>
            {
                Ty::Struct(name.clone())
            }
            Type::Named(name, arguments) if arguments.is_empty() => {
                if self.struct_defs.contains_key(name) {
                    Ty::Struct(name.clone())
                } else if self.enum_defs.contains_key(name) {
                    Ty::Enum(name.clone())
                } else if self.struct_templates.contains_key(name)
                    || self.enum_templates.contains_key(name)
                {
                    self.error(format!(
                        "generic type `{name}` requires explicit type arguments"
                    ));
                    Ty::Error
                } else {
                    self.error(format!("unknown type `{name}`"));
                    Ty::Error
                }
            }
            Type::Named(name, source_arguments) => {
                let kind = if self.struct_templates.contains_key(name) {
                    NominalKind::Struct
                } else if self.enum_templates.contains_key(name) {
                    NominalKind::Enum
                } else if self.struct_defs.contains_key(name) || self.enum_defs.contains_key(name) {
                    self.error(format!(
                        "non-generic type `{name}` does not accept type arguments"
                    ));
                    return Ty::Error;
                } else {
                    self.error(format!("unknown generic type `{name}`"));
                    return Ty::Error;
                };
                let expected = match kind {
                    NominalKind::Struct => self.struct_templates[name]
                        .compile_groups
                        .iter()
                        .flatten()
                        .count(),
                    NominalKind::Enum => self.enum_templates[name]
                        .compile_groups
                        .iter()
                        .flatten()
                        .count(),
                };
                if source_arguments.len() != expected {
                    self.error(format!(
                        "type argument count mismatch for `{name}`: expected {expected}, found {}",
                        source_arguments.len()
                    ));
                    return Ty::Error;
                }
                let mut arguments = Vec::new();
                for argument in source_arguments {
                    let argument = self.lower_source_type(argument);
                    if argument == Ty::Error {
                        return Ty::Error;
                    }
                    arguments.push(argument);
                }
                let Some(canonical) =
                    self.ensure_nominal_instance(kind, name, source_arguments.clone(), arguments)
                else {
                    return Ty::Error;
                };
                match kind {
                    NominalKind::Struct => Ty::Struct(canonical),
                    NominalKind::Enum => Ty::Enum(canonical),
                }
            }
            Type::NamedArgs(name, _) => {
                self.error(format!(
                    "internal error: labeled type arguments for `{name}` were not normalized"
                ));
                Ty::Error
            }
        }
    }

    pub(super) fn source_type_for_ty(&self, ty: &Ty) -> Option<Type> {
        match ty {
            Ty::I32 => Some(Type::I32),
            Ty::I64 => Some(Type::I64),
            Ty::U32 => Some(Type::U32),
            Ty::U64 => Some(Type::U64),
            Ty::Bool => Some(Type::Bool),
            Ty::Unit => Some(Type::Unit),
            Ty::Array(element, length) => Some(Type::Array(
                Box::new(self.source_type_for_ty(element)?),
                *length,
            )),
            Ty::Pointer { pointee, mutable } => Some(Type::Named(
                if *mutable { "MutPtr" } else { "Ptr" }.to_owned(),
                vec![self.source_type_for_ty(pointee)?],
            )),
            Ty::Reference {
                pointee,
                mutable,
                region,
            } => Some(Type::Borrow {
                mutable: *mutable,
                access: None,
                region: region.clone(),
                pointee: Box::new(self.source_type_for_ty(pointee)?),
            }),
            Ty::Struct(name) | Ty::Enum(name) => {
                if is_compile_value_marker(name) {
                    if let Some(constructor) = type_constructor_from_marker(name) {
                        return Some(Type::Named(constructor, Vec::new()));
                    }
                    return Some(Type::Named(name.clone(), Vec::new()));
                }
                if let Some(instance) = self.nominal_instances.get(name) {
                    let arguments = instance
                        .key
                        .arguments
                        .iter()
                        .map(|argument| self.source_type_for_ty(argument))
                        .collect::<Option<Vec<_>>>()?;
                    Some(Type::Named(instance.key.template.clone(), arguments))
                } else if self.abstract_type_parameters.contains_key(name)
                    || self.struct_defs.contains_key(name)
                    || self.enum_defs.contains_key(name)
                {
                    Some(Type::Named(name.clone(), Vec::new()))
                } else {
                    None
                }
            }
            Ty::Function(function) => Some(Type::Function {
                groups: function
                    .groups
                    .iter()
                    .map(|group| {
                        group
                            .iter()
                            .map(|ty| self.source_type_for_ty(ty))
                            .collect::<Option<Vec<_>>>()
                    })
                    .collect::<Option<Vec<_>>>()?,
                effects: FunctionEffects {
                    unsafe_effect: function.unsafe_effect,
                    throws: function
                        .throws_error
                        .as_deref()
                        .and_then(|error| self.source_type_for_ty(error))
                        .map(Box::new),
                    custom: effect_identity_sources(&function.custom_effects),
                    parameters: Vec::new(),
                },
                result: Box::new(self.source_type_for_ty(&function.result)?),
            }),
            Ty::Callable(callable) => {
                self.source_type_for_ty(&Ty::Function(callable.signature.clone()))
            }
            Ty::Continuation { input, output } => Some(Type::Named(
                self.lang_item_name(LangItemKind::Continuation).to_owned(),
                vec![
                    self.source_type_for_ty(input)?,
                    self.source_type_for_ty(output)?,
                ],
            )),
            Ty::EffectCallable {
                input,
                output,
                answer,
            } => Some(Type::Named(
                self.lang_item_name(LangItemKind::EffectCallable).to_owned(),
                vec![
                    self.source_type_for_ty(input)?,
                    self.source_type_for_ty(output)?,
                    self.source_type_for_ty(answer)?,
                ],
            )),
            Ty::EffectRow {
                unsafe_effect,
                throws_error,
                custom_effects,
            } => Some(effect_row_source(
                *unsafe_effect,
                throws_error
                    .as_deref()
                    .and_then(|error| self.source_type_for_ty(error)),
                custom_effects,
            )),
            Ty::Never | Ty::Error => None,
        }
    }

    /// Render an internal type using source-level names for diagnostics.
    ///
    /// Concrete generic nominals use canonical `$mono$type$...` names in HIR
    /// and layout maps. Those names are intentionally stable for compiler
    /// identity, but they are not part of Salicin's user-facing syntax.
    pub(super) fn diagnostic_type_name(&self, ty: &Ty) -> String {
        match ty {
            Ty::I32 => "i32".to_owned(),
            Ty::I64 => "i64".to_owned(),
            Ty::U32 => "u32".to_owned(),
            Ty::U64 => "u64".to_owned(),
            Ty::Bool => "bool".to_owned(),
            Ty::Unit => "()".to_owned(),
            Ty::Array(element, length) => {
                format!("Array({}, {length})", self.diagnostic_type_name(element))
            }
            Ty::Pointer { pointee, mutable } => format!(
                "{}({})",
                if *mutable { "MutPtr" } else { "Ptr" },
                self.diagnostic_type_name(pointee)
            ),
            Ty::Reference {
                pointee,
                mutable,
                region,
            } => {
                let mode = if *mutable { "borrow(mut)" } else { "borrow" };
                let region = region
                    .as_ref()
                    .map_or_else(String::new, |region| format!("('{region})"));
                format!("{mode}{region} {}", self.diagnostic_type_name(pointee))
            }
            Ty::Struct(name) | Ty::Enum(name) => {
                if let Some(parameter) = self.abstract_type_parameters.get(name) {
                    return parameter.clone();
                }
                let Some(instance) = self.nominal_instances.get(name) else {
                    return name.clone();
                };
                if instance.key.arguments.is_empty() {
                    return instance.key.template.clone();
                }
                let arguments = instance
                    .key
                    .arguments
                    .iter()
                    .map(|argument| self.diagnostic_type_name(argument))
                    .collect::<Vec<_>>()
                    .join(", ");
                format!("{}({arguments})", instance.key.template)
            }
            Ty::Never => "Never".to_owned(),
            Ty::Error => "<error>".to_owned(),
            Ty::Function(function) => {
                let mut rendered = String::new();
                for group in &function.groups {
                    rendered.push('(');
                    rendered.push_str(
                        &group
                            .iter()
                            .map(|parameter| self.diagnostic_type_name(parameter))
                            .collect::<Vec<_>>()
                            .join(", "),
                    );
                    rendered.push(')');
                }
                rendered.push_str(": ");
                rendered.push_str(&self.diagnostic_type_name(&function.result));
                let mut effects = function.custom_effects.clone();
                if function.unsafe_effect {
                    effects.insert(0, "Unsafe".to_owned());
                }
                if !effects.is_empty() {
                    rendered.push_str(" with(");
                    rendered.push_str(&effects.join(", "));
                    rendered.push(')');
                }
                rendered
            }
            Ty::Callable(callable) => {
                self.diagnostic_type_name(&Ty::Function(callable.signature.clone()))
            }
            Ty::Continuation { input, output } => format!(
                "Continuation({}, {})",
                self.diagnostic_type_name(input),
                self.diagnostic_type_name(output)
            ),
            Ty::EffectCallable {
                input,
                output,
                answer,
            } => format!(
                "EffectCallable({}, {}, {})",
                self.diagnostic_type_name(input),
                self.diagnostic_type_name(output),
                self.diagnostic_type_name(answer)
            ),
            Ty::EffectRow { .. } => ty.to_string(),
        }
    }

    pub(super) fn type_argument_from_expr(
        &mut self,
        expression: &Expr,
        substitutions: &HashMap<String, Type>,
    ) -> Option<Type> {
        match expression {
            Expr::Type(source) => {
                let mut source = source.clone();
                substitute_type_parameters(&mut source, substitutions);
                Some(source)
            }
            Expr::Unit => Some(Type::Unit),
            Expr::Name(name) => {
                if let Some(replacement) = substitutions.get(name) {
                    return Some(replacement.clone());
                }
                Some(match name.as_str() {
                    "i32" => Type::I32,
                    "i64" => Type::I64,
                    "u32" => Type::U32,
                    "u64" => Type::U64,
                    "bool" => Type::Bool,
                    _ => Type::Named(name.clone(), Vec::new()),
                })
            }
            Expr::Call(_, _) => {
                let mut groups = Vec::new();
                let root = flatten_call(expression, &mut groups);
                let Expr::Name(name) = root else {
                    self.error("generic type arguments require a named type constructor");
                    return None;
                };
                if groups
                    .iter()
                    .flat_map(|group| group.iter())
                    .any(|argument| argument.label.is_some())
                {
                    self.error("generic type arguments cannot contain labeled arguments");
                    return None;
                }
                if name == "Array" {
                    if groups.len() != 1 || groups[0].len() != 2 {
                        self.error("`Array` type arguments require an element type and length");
                        return None;
                    }
                    let call_arguments = groups[0];
                    let element =
                        self.type_argument_from_expr(&call_arguments[0].value, substitutions)?;
                    let Expr::Integer(length) = call_arguments[1].value else {
                        self.error("array type argument length must be a non-negative integer");
                        return None;
                    };
                    let Ok(length) = u64::try_from(length) else {
                        self.error("array type argument length must fit in `u64`");
                        return None;
                    };
                    Some(Type::Array(Box::new(element), length))
                } else {
                    let mut arguments = Vec::new();
                    for argument in groups.iter().flat_map(|group| group.iter()) {
                        arguments
                            .push(self.type_argument_from_expr(&argument.value, substitutions)?);
                    }
                    Some(Type::Named(name.clone(), arguments))
                }
            }
            _ => {
                self.error(format!(
                    "generic type arguments must be type names or type applications, found `{expression:?}`"
                ));
                None
            }
        }
    }

    pub(super) fn type_constructor_argument_from_expr(
        &mut self,
        expression: &Expr,
        parameter_count: usize,
        owner: &str,
        parameter: &str,
    ) -> Option<String> {
        let Expr::Name(name) = expression else {
            self.error(format!(
                "invalid type-constructor argument for `{parameter}` in `{owner}`; expected a generic type constructor"
            ));
            return None;
        };
        let Some(target) =
            self.type_constructor_impl_target(&Type::Named(name.clone(), Vec::new()))
        else {
            self.error(format!(
                "invalid type-constructor argument `{name}` for `{parameter}` in `{owner}`; expected a generic type constructor"
            ));
            return None;
        };
        if target.parameter_count != parameter_count {
            self.error(format!(
                "type-constructor argument `{name}` for `{parameter}` in `{owner}` has {} parameter{}, expected {parameter_count}",
                target.parameter_count,
                if target.parameter_count == 1 { "" } else { "s" }
            ));
            return None;
        }
        Some(name.clone())
    }

    pub(super) fn probe_type_argument_source(
        &self,
        expression: &Expr,
        substitutions: &HashMap<String, Type>,
    ) -> Option<Type> {
        match expression {
            Expr::Unit => Some(Type::Unit),
            Expr::Name(name) => substitutions.get(name).cloned().or_else(|| {
                Some(match name.as_str() {
                    "i32" => Type::I32,
                    "i64" => Type::I64,
                    "u32" => Type::U32,
                    "u64" => Type::U64,
                    "bool" => Type::Bool,
                    _ => Type::Named(name.clone(), Vec::new()),
                })
            }),
            Expr::Call(_, _) => {
                let mut groups = Vec::new();
                let root = flatten_call(expression, &mut groups);
                let Expr::Name(name) = root else {
                    return None;
                };
                if groups
                    .iter()
                    .flat_map(|group| group.iter())
                    .any(|argument| argument.label.is_some())
                {
                    return None;
                }
                if name == "Array" {
                    if groups.len() != 1 || groups[0].len() != 2 {
                        return None;
                    }
                    let arguments = groups[0];
                    let element =
                        self.probe_type_argument_source(&arguments[0].value, substitutions)?;
                    let Expr::Integer(length) = arguments[1].value else {
                        return None;
                    };
                    Some(Type::Array(Box::new(element), u64::try_from(length).ok()?))
                } else {
                    let arguments = groups
                        .iter()
                        .flat_map(|group| group.iter())
                        .map(|argument| {
                            self.probe_type_argument_source(&argument.value, substitutions)
                        })
                        .collect::<Option<Vec<_>>>()?;
                    Some(Type::Named(name.clone(), arguments))
                }
            }
            _ => None,
        }
    }

    pub(super) fn probe_compile_argument_source(
        &self,
        parameter: &CompileParam,
        expression: &Expr,
        substitutions: &HashMap<String, Type>,
    ) -> Option<Type> {
        match parameter.kind {
            CompileParamKind::Type => self.probe_type_argument_source(expression, substitutions),
            CompileParamKind::Access => match expression {
                Expr::Name(name) if name == "shared" => {
                    Some(Type::Named(ACCESS_SHARED_MARKER.to_owned(), Vec::new()))
                }
                Expr::Name(name) if name == "mut" => {
                    Some(Type::Named(ACCESS_MUT_MARKER.to_owned(), Vec::new()))
                }
                _ => None,
            },
            CompileParamKind::Passing => match expression {
                Expr::Name(name) if name == "auto" => {
                    Some(Type::Named(PASSING_AUTO_MARKER.to_owned(), Vec::new()))
                }
                Expr::Name(name) if name == "copy" => {
                    Some(Type::Named(PASSING_COPY_MARKER.to_owned(), Vec::new()))
                }
                Expr::Name(name) if name == "move" => {
                    Some(Type::Named(PASSING_MOVE_MARKER.to_owned(), Vec::new()))
                }
                _ => None,
            },
            CompileParamKind::Effect => match expression {
                Expr::Name(name) if name == "pure" => Some(effect_row_source(false, None, &[])),
                Expr::Name(name) if name == self.lang_item_name(LangItemKind::UnsafeEffect) => {
                    Some(effect_row_source(true, None, &[]))
                }
                Expr::Name(name) if self.effects.contains(name) => {
                    Some(effect_row_source(false, None, std::slice::from_ref(name)))
                }
                Expr::Name(name) if effect_row_from_marker(name).is_some() => {
                    Some(Type::Named(name.clone(), Vec::new()))
                }
                Expr::Call(callee, arguments)
                    if matches!(
                        callee.as_ref(),
                        Expr::Name(name)
                            if name == self.lang_item_name(LangItemKind::UnsafeEffect)
                                && arguments.is_empty()
                    ) =>
                {
                    Some(effect_row_source(true, None, &[]))
                }
                Expr::Call(callee, arguments)
                    if matches!(
                        callee.as_ref(),
                        Expr::Name(name) if self.effects.contains(name)
                    ) =>
                {
                    let Expr::Name(name) = callee.as_ref() else {
                        unreachable!()
                    };
                    let mut source_arguments = Vec::new();
                    for argument in arguments {
                        if argument.label.is_some() {
                            return None;
                        }
                        source_arguments
                            .push(self.probe_type_argument_source(&argument.value, substitutions)?);
                    }
                    let effect = Type::Named(name.clone(), source_arguments);
                    if self.is_standard_unsafe_effect_source(&effect) {
                        Some(effect_row_source(true, None, &[]))
                    } else {
                        Some(effect_row_source(
                            false,
                            None,
                            &[source_effect_identity(&effect)],
                        ))
                    }
                }
                Expr::Call(callee, arguments)
                    if matches!(callee.as_ref(), Expr::Name(name) if effect_row_from_marker(name).is_some())
                        && arguments.len() <= 1
                        && arguments.iter().all(|argument| argument.label.is_none()) =>
                {
                    let Expr::Name(marker) = callee.as_ref() else {
                        unreachable!()
                    };
                    let error = match arguments.first() {
                        Some(argument) => {
                            Some(self.probe_type_argument_source(&argument.value, substitutions)?)
                        }
                        None => None,
                    };
                    Some(Type::Named(marker.clone(), error.into_iter().collect()))
                }
                _ => None,
            },
            CompileParamKind::TypeConstructor { parameter_count } => {
                let Expr::Name(name) = expression else {
                    return None;
                };
                let source = Type::Named(name.clone(), Vec::new());
                self.type_constructor_impl_target(&source)
                    .filter(|target| target.parameter_count == parameter_count)
                    .map(|_| source)
            }
            CompileParamKind::Region | CompileParamKind::EffectConstructor { .. } => None,
        }
    }

    pub(super) fn probe_compile_argument_ty(
        &self,
        parameter: &CompileParam,
        source: &Type,
    ) -> Option<Ty> {
        match parameter.kind {
            CompileParamKind::TypeConstructor { parameter_count } => {
                let Type::Named(name, arguments) = source else {
                    return None;
                };
                if !arguments.is_empty() {
                    return None;
                }
                self.type_constructor_impl_target(source)
                    .filter(|target| target.parameter_count == parameter_count)
                    .map(|_| Ty::Struct(type_constructor_marker(name)))
            }
            CompileParamKind::Type
            | CompileParamKind::Access
            | CompileParamKind::Passing
            | CompileParamKind::Effect => self.probe_source_ty(source),
            CompileParamKind::Region | CompileParamKind::EffectConstructor { .. } => None,
        }
    }

    pub(super) fn probe_compile_group_sources(
        &self,
        parameters: &[CompileParam],
        supplied: &[CallArg],
        substitutions: &HashMap<String, Type>,
    ) -> Option<Vec<Type>> {
        if parameters.len() != supplied.len() {
            return None;
        }
        let labeled = supplied
            .first()
            .is_some_and(|argument| argument.label.is_some());
        let mut sources = vec![None; parameters.len()];
        for (position, argument) in supplied.iter().enumerate() {
            let parameter_index = if labeled {
                let label = argument.label.as_ref()?;
                parameters
                    .iter()
                    .position(|parameter| parameter.name == *label)?
            } else {
                if argument.label.is_some() {
                    return None;
                }
                position
            };
            if sources[parameter_index].is_some() {
                return None;
            }
            let parameter = &parameters[parameter_index];
            let source =
                self.probe_compile_argument_source(parameter, &argument.value, substitutions)?;
            self.probe_compile_argument_ty(parameter, &source)?;
            sources[parameter_index] = Some(source);
        }
        sources.into_iter().collect()
    }

    pub(super) fn group_is_explicit_compile_application(
        &self,
        parameters: &[CompileParam],
        arguments: &[CallArg],
        context: &LowerCtx,
        unit_is_type: bool,
    ) -> bool {
        if parameters
            .iter()
            .all(|parameter| parameter.kind == CompileParamKind::Type)
        {
            return arguments.iter().all(|argument| {
                (unit_is_type && matches!(argument.value, Expr::Unit))
                    || self.expression_is_explicit_type_argument(&argument.value, context)
            });
        }
        if parameters
            .iter()
            .all(|parameter| parameter.kind == CompileParamKind::Access)
        {
            return arguments.iter().all(|argument| {
                matches!(&argument.value, Expr::Name(name) if name == "shared" || name == "mut")
            });
        }
        if parameters
            .iter()
            .all(|parameter| parameter.kind == CompileParamKind::Passing)
        {
            return arguments.iter().all(|argument| {
                matches!(&argument.value, Expr::Name(name)
                    if matches!(name.as_str(), "auto" | "copy" | "move"))
            });
        }
        if parameters
            .iter()
            .all(|parameter| parameter.kind == CompileParamKind::Effect)
        {
            return arguments
                .iter()
                .all(|argument| self.expression_is_explicit_effect_argument(&argument.value));
        }
        parameters.len() == arguments.len()
            && parameters
                .iter()
                .zip(arguments)
                .all(|(parameter, argument)| {
                    self.expression_is_explicit_compile_argument(
                        parameter,
                        &argument.value,
                        context,
                        unit_is_type,
                    )
                })
    }

    pub(super) fn explicit_compile_group_prefix(
        &self,
        compile_groups: &[Vec<CompileParam>],
        groups: &[&[CallArg]],
        context: &LowerCtx,
    ) -> usize {
        let mut compile_index = 0;
        let mut source_index = 0;
        while compile_index < compile_groups.len() && source_index < groups.len() {
            let arguments = groups[source_index];
            let labeled = arguments
                .first()
                .is_some_and(|argument| argument.label.is_some());
            let target = if labeled {
                (compile_index..compile_groups.len()).find(|index| {
                    arguments.iter().all(|argument| {
                        argument.label.as_ref().is_some_and(|label| {
                            compile_groups[*index]
                                .iter()
                                .any(|parameter| parameter.name == *label)
                        })
                    })
                })
            } else if !arguments.is_empty()
                && self.group_is_explicit_compile_application(
                    &compile_groups[compile_index],
                    arguments,
                    context,
                    false,
                )
            {
                Some(compile_index)
            } else {
                None
            };
            let Some(target) = target else {
                break;
            };
            compile_index = target + 1;
            source_index += 1;
        }
        source_index
    }

    fn expression_is_explicit_compile_argument(
        &self,
        parameter: &CompileParam,
        expression: &Expr,
        context: &LowerCtx,
        unit_is_type: bool,
    ) -> bool {
        match parameter.kind {
            CompileParamKind::Type => {
                (unit_is_type && matches!(expression, Expr::Unit))
                    || self.expression_is_explicit_type_argument(expression, context)
            }
            CompileParamKind::Access => {
                matches!(expression, Expr::Name(name) if name == "shared" || name == "mut")
            }
            CompileParamKind::Passing => {
                matches!(expression, Expr::Name(name)
                    if matches!(name.as_str(), "auto" | "copy" | "move"))
            }
            CompileParamKind::Effect => self.expression_is_explicit_effect_argument(expression),
            CompileParamKind::TypeConstructor { parameter_count } => {
                self.expression_is_explicit_type_constructor_argument(expression, parameter_count)
            }
            CompileParamKind::Region => false,
            CompileParamKind::EffectConstructor { .. } => false,
        }
    }

    fn expression_is_explicit_type_constructor_argument(
        &self,
        expression: &Expr,
        parameter_count: usize,
    ) -> bool {
        let Expr::Name(name) = expression else {
            return false;
        };
        let source = Type::Named(name.clone(), Vec::new());
        self.type_constructor_impl_target(&source)
            .is_some_and(|target| target.parameter_count == parameter_count)
    }

    fn expression_is_explicit_effect_argument(&self, expression: &Expr) -> bool {
        match expression {
            Expr::Name(name) => {
                name == "pure"
                    || name == self.lang_item_name(LangItemKind::UnsafeEffect)
                    || self.effects.contains(name)
                    || effect_row_from_marker(name).is_some()
            }
            Expr::Call(callee, arguments) => {
                let Expr::Name(name) = callee.as_ref() else {
                    return false;
                };
                if self.effects.contains(name) {
                    return arguments.iter().all(|argument| argument.label.is_none());
                }
                effect_row_from_marker(name).is_some()
                    && arguments.len() == 1
                    && arguments[0].label.is_none()
            }
            _ => false,
        }
    }

    fn expression_is_explicit_type_argument(&self, expression: &Expr, context: &LowerCtx) -> bool {
        match expression {
            Expr::Name(name) => {
                context.type_substitutions.contains_key(name)
                    || context.has_type_parameter(name)
                    || self.abstract_type_parameters.contains_key(name)
                    || matches!(
                        name.as_str(),
                        "i32" | "i64" | "u32" | "u64" | "bool" | "Never"
                    )
                    || self.struct_defs.contains_key(name)
                    || self.enum_defs.contains_key(name)
                    || self.struct_templates.contains_key(name)
                    || self.enum_templates.contains_key(name)
            }
            Expr::Call(_, _) => {
                let mut groups = Vec::new();
                let root = flatten_call(expression, &mut groups);
                let Expr::Name(name) = root else {
                    return false;
                };
                if groups
                    .iter()
                    .flat_map(|group| group.iter())
                    .any(|argument| argument.label.is_some())
                {
                    return false;
                }
                if name == "Array" {
                    return groups.len() == 1
                        && groups[0].len() == 2
                        && self.expression_is_explicit_type_argument(&groups[0][0].value, context)
                        && matches!(groups[0][1].value, Expr::Integer(_));
                }
                self.struct_templates.contains_key(name) || self.enum_templates.contains_key(name)
            }
            _ => false,
        }
    }

    pub(super) fn probe_source_ty(&self, source: &Type) -> Option<Ty> {
        match source {
            Type::I32 => Some(Ty::I32),
            Type::I64 => Some(Ty::I64),
            Type::U32 => Some(Ty::U32),
            Type::U64 => Some(Ty::U64),
            Type::Bool => Some(Ty::Bool),
            Type::Unit => Some(Ty::Unit),
            Type::Function {
                groups,
                effects,
                result,
            } => {
                if !effects.parameters.is_empty() {
                    return None;
                }
                Some(Ty::Function(FunctionTy {
                    groups: groups
                        .iter()
                        .map(|group| {
                            group
                                .iter()
                                .map(|ty| self.probe_source_ty(ty))
                                .collect::<Option<Vec<_>>>()
                        })
                        .collect::<Option<Vec<_>>>()?,
                    unsafe_effect: self.function_effects_unsafe(effects),
                    throws_error: match effects.throws.as_deref() {
                        Some(error) => Some(Box::new(self.probe_source_ty(error)?)),
                        None => None,
                    },
                    custom_effects: self.function_effects_custom_identities(effects),
                    result: Box::new(self.probe_source_ty(result)?),
                }))
            }
            Type::Borrow {
                mutable,
                region,
                pointee,
                ..
            } => Some(Ty::Reference {
                pointee: Box::new(self.probe_source_ty(pointee)?),
                mutable: *mutable,
                region: region.clone(),
            }),
            Type::Array(element, length) => {
                Some(Ty::Array(Box::new(self.probe_source_ty(element)?), *length))
            }
            Type::Named(name, arguments) if name == "()" && arguments.is_empty() => Some(Ty::Unit),
            Type::Named(name, _) if effect_row_from_marker(name).is_some() => {
                let (unsafe_effect, throws_error, custom_effects) = effect_row_from_source(source)?;
                Some(Ty::EffectRow {
                    unsafe_effect,
                    throws_error: match throws_error.as_ref() {
                        Some(error) => Some(Box::new(self.probe_source_ty(error)?)),
                        None => None,
                    },
                    custom_effects,
                })
            }
            Type::Named(name, arguments)
                if arguments.is_empty() && is_compile_value_marker(name) =>
            {
                Some(Ty::Struct(name.clone()))
            }
            Type::Named(name, arguments)
                if arguments.is_empty()
                    && (self.abstract_type_parameters.contains_key(name)
                        || name.starts_with("$generic$param$")) =>
            {
                Some(Ty::Struct(name.clone()))
            }
            Type::Named(name, arguments) if arguments.is_empty() => {
                if self.struct_defs.contains_key(name) {
                    Some(Ty::Struct(name.clone()))
                } else if self.enum_defs.contains_key(name) {
                    Some(Ty::Enum(name.clone()))
                } else {
                    None
                }
            }
            Type::Named(name, source_arguments) => {
                let (kind, expected) = if let Some(template) = self.struct_templates.get(name) {
                    (
                        NominalKind::Struct,
                        template.compile_groups.iter().flatten().count(),
                    )
                } else if let Some(template) = self.enum_templates.get(name) {
                    (
                        NominalKind::Enum,
                        template.compile_groups.iter().flatten().count(),
                    )
                } else {
                    return None;
                };
                if source_arguments.len() != expected {
                    return None;
                }
                let arguments = source_arguments
                    .iter()
                    .map(|argument| self.probe_source_ty(argument))
                    .collect::<Option<Vec<_>>>()?;
                let key = NominalInstanceKey {
                    kind,
                    template: name.clone(),
                    arguments,
                };
                let canonical = self
                    .nominal_instance_names
                    .get(&key)
                    .cloned()
                    .unwrap_or_else(|| nominal_instance_name(&key));
                Some(match kind {
                    NominalKind::Struct => Ty::Struct(canonical),
                    NominalKind::Enum => Ty::Enum(canonical),
                })
            }
            Type::NamedArgs(_, _) => None,
        }
    }

    pub(super) fn probe_generic_nominal_type_head(
        &self,
        name: &str,
        groups: &[&[CallArg]],
        context: &LowerCtx,
    ) -> Option<(NominalKind, Ty, Type)> {
        let (kind, compile_groups) = if let Some(template) = self.struct_templates.get(name) {
            (NominalKind::Struct, &template.compile_groups)
        } else if let Some(template) = self.enum_templates.get(name) {
            (NominalKind::Enum, &template.compile_groups)
        } else {
            return None;
        };
        if groups.len() != compile_groups.len() {
            return None;
        }
        let mut source_arguments = Vec::new();
        let mut arguments = Vec::new();
        for (parameters, supplied) in compile_groups.iter().zip(groups) {
            if parameters.len() != supplied.len()
                || supplied.iter().any(|argument| argument.label.is_some())
            {
                return None;
            }
            for argument in *supplied {
                let source =
                    self.probe_type_argument_source(&argument.value, &context.type_substitutions)?;
                let ty = self.probe_source_ty(&source)?;
                source_arguments.push(source);
                arguments.push(ty);
            }
        }
        let key = NominalInstanceKey {
            kind,
            template: name.to_owned(),
            arguments,
        };
        let canonical = self
            .nominal_instance_names
            .get(&key)
            .cloned()
            .unwrap_or_else(|| nominal_instance_name(&key));
        let ty = match kind {
            NominalKind::Struct => Ty::Struct(canonical),
            NominalKind::Enum => Ty::Enum(canonical),
        };
        Some((kind, ty, Type::Named(name.to_owned(), source_arguments)))
    }

    pub(super) fn probe_nominal_type_head(
        &self,
        expression: &Expr,
        context: &LowerCtx,
    ) -> Option<(NominalKind, Ty, Type)> {
        let mut groups = Vec::new();
        let root = flatten_call(expression, &mut groups);
        let Expr::Name(name) = root else {
            return None;
        };
        if context.shadows_top_level_name(name) {
            return None;
        }
        if groups.is_empty() {
            if self.struct_defs.contains_key(name) {
                return Some((
                    NominalKind::Struct,
                    Ty::Struct(name.clone()),
                    Type::Named(name.clone(), Vec::new()),
                ));
            }
            if self.enum_defs.contains_key(name) {
                return Some((
                    NominalKind::Enum,
                    Ty::Enum(name.clone()),
                    Type::Named(name.clone(), Vec::new()),
                ));
            }
        }
        self.probe_generic_nominal_type_head(name, &groups, context)
    }

    pub(super) fn probe_enum_variant_fields(
        &self,
        source: &Type,
        variant: &str,
    ) -> Option<VariantFields> {
        let Type::Named(template, _) = source else {
            return None;
        };
        self.enum_templates
            .get(template)
            .or_else(|| self.enum_defs.get(template))?
            .variants
            .iter()
            .find(|candidate| candidate.name == variant)
            .map(|candidate| candidate.fields.clone())
    }
}

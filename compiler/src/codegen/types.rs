use std::collections::HashMap;
use std::fmt;

use crate::ast::{
    CallArg, CompileParam, CompileParamKind, Expr, FunctionEffects, Type, USizeConst, VariantFields,
};
use crate::core::LangItemKind;

use super::compile_time::{
    closed_value_from_marker, closed_value_marker, describe_compile_param_kind,
    effect_identity_sources, effect_row_from_marker, effect_row_from_source, effect_row_source,
    is_compile_value_marker, source_effect_identity, type_constructor_from_marker,
    type_constructor_marker, usize_value_from_marker, usize_value_marker, ACCESS_MUT_MARKER,
    ACCESS_SHARED_MARKER, PARAMETER_MODIFIER_COPY_MARKER, PARAMETER_MODIFIER_MOVE_MARKER,
};
use super::flow::LowerCtx;
use super::hir::{FunctionTy, Ty};
use super::lower::{display_region_argument, flatten_call};
use super::names::nominal_instance_name;
use super::registry::{NominalInstanceKey, NominalKind};
use super::source_rewrite::substitute_type_parameters;
use super::Analyzer;

impl Analyzer {
    pub(super) fn require_same_type(
        &mut self,
        actual: &Ty,
        expected: &Ty,
        context: impl fmt::Display,
    ) {
        if super::hir::type_is_assignable(actual, expected)
            || self.is_uninhabited_type(actual)
            || *actual == Ty::Error
            || *expected == Ty::Error
        {
            return;
        }
        let expected = self.diagnostic_type_name(expected);
        let actual = self.diagnostic_type_name(actual);
        self.error(format!(
            "type mismatch for {context}: expected `{expected}`, found `{actual}`"
        ));
    }

    pub(super) fn unify_types(&mut self, left: &Ty, right: &Ty, context: impl fmt::Display) -> Ty {
        if left == right {
            return left.clone();
        }
        if self.is_uninhabited_type(left) {
            return right.clone();
        }
        if self.is_uninhabited_type(right) {
            return left.clone();
        }
        if *left == Ty::Error || *right == Ty::Error {
            return Ty::Error;
        }
        self.error(format!(
            "type mismatch for {context}: `{left}` and `{right}` cannot be unified"
        ));
        Ty::Error
    }

    pub(super) fn is_uninhabited_type(&self, ty: &Ty) -> bool {
        *ty == Ty::Never
            || matches!(ty, Ty::Enum(name) if self.enum_layouts.get(name).is_some_and(|layout| layout.variants.is_empty()))
    }

    pub(super) fn lower_source_type(&mut self, source: &Type) -> Ty {
        match source {
            Type::I8 => Ty::I8,
            Type::I16 => Ty::I16,
            Type::I32 => Ty::I32,
            Type::I64 => Ty::I64,
            Type::I128 => Ty::I128,
            Type::ISize => Ty::ISize,
            Type::U8 => Ty::U8,
            Type::U16 => Ty::U16,
            Type::U32 => Ty::U32,
            Type::U64 => Ty::U64,
            Type::U128 => Ty::U128,
            Type::USize => Ty::USize,
            Type::Bool => Ty::Bool,
            Type::Unit => Ty::Unit,
            Type::Tuple(fields) => {
                let tuple = Ty::Tuple(
                    fields
                        .iter()
                        .map(|field| self.lower_source_type(field))
                        .collect(),
                );
                self.tuple_types.insert(tuple.clone());
                tuple
            }
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
            Type::ArrayApplication {
                constructor,
                element,
                length,
            } => {
                if !self.is_lang_item_name(constructor, LangItemKind::ArrayTypeForm) {
                    self.error(format!(
                        "array syntax resolved to non-standard constructor `{constructor}`"
                    ));
                    return Ty::Error;
                }
                let USizeConst::Literal(length) = length else {
                    self.error(format!(
                        "array length parameter `{}` was not resolved during generic instantiation",
                        match length {
                            USizeConst::Parameter(name) => name,
                            USizeConst::Literal(_) => unreachable!(),
                        }
                    ));
                    return Ty::Error;
                };
                let element = self.lower_source_type(element);
                if *length > i32::MAX as u64 {
                    self.error(format!(
                        "array length {length} exceeds the first-version limit of {}",
                        i32::MAX
                    ));
                    Ty::Error
                } else {
                    Ty::Array(Box::new(element), *length)
                }
            }
            Type::CompileUSize(value) => {
                self.error(format!(
                    "compile-time `usize` value `{value}` cannot be used as a runtime type"
                ));
                Ty::Error
            }
            Type::Named(name, arguments) if name == "()" && arguments.is_empty() => Ty::Unit,
            Type::Named(name, arguments)
                if arguments.is_empty()
                    && [
                        LangItemKind::Bool,
                        LangItemKind::I8,
                        LangItemKind::I16,
                        LangItemKind::I32,
                        LangItemKind::I64,
                        LangItemKind::I128,
                        LangItemKind::ISize,
                        LangItemKind::U8,
                        LangItemKind::U16,
                        LangItemKind::U32,
                        LangItemKind::U64,
                        LangItemKind::U128,
                        LangItemKind::USize,
                    ]
                    .into_iter()
                    .any(|kind| name == self.lang_item_name(kind)) =>
            {
                [
                    (LangItemKind::Bool, Ty::Bool),
                    (LangItemKind::I8, Ty::I8),
                    (LangItemKind::I16, Ty::I16),
                    (LangItemKind::I32, Ty::I32),
                    (LangItemKind::I64, Ty::I64),
                    (LangItemKind::I128, Ty::I128),
                    (LangItemKind::ISize, Ty::ISize),
                    (LangItemKind::U8, Ty::U8),
                    (LangItemKind::U16, Ty::U16),
                    (LangItemKind::U32, Ty::U32),
                    (LangItemKind::U64, Ty::U64),
                    (LangItemKind::U128, Ty::U128),
                    (LangItemKind::USize, Ty::USize),
                ]
                .into_iter()
                .find_map(|(kind, ty)| (name == self.lang_item_name(kind)).then_some(ty))
                .expect("primitive lang-item guard matched")
            }
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
            Type::Named(name, arguments)
                if self.is_lang_item_name(name, LangItemKind::PtrTypeForm) =>
            {
                let (access, pointee) = match arguments.as_slice() {
                    [pointee] => (ACCESS_SHARED_MARKER, pointee),
                    [Type::Named(access, access_arguments), pointee]
                        if access_arguments.is_empty() =>
                    {
                        (access.as_str(), pointee)
                    }
                    _ => {
                        self.error(format!(
                            "type `{name}` expects an optional access argument and a type argument"
                        ));
                        return Ty::Error;
                    }
                };
                if !matches!(
                    access,
                    "shared" | "mut" | ACCESS_SHARED_MARKER | ACCESS_MUT_MARKER
                ) {
                    self.error("`Ptr` access must be `shared` or `mut`");
                    return Ty::Error;
                }
                Ty::Pointer {
                    pointee: Box::new(self.lower_source_type(pointee)),
                    mutable: matches!(access, "mut" | ACCESS_MUT_MARKER),
                }
            }
            Type::Named(name, arguments)
                if self.is_lang_item_name(name, LangItemKind::SliceTypeForm) =>
            {
                if let [element] = arguments.as_slice() {
                    Ty::Slice(Box::new(self.lower_source_type(element)))
                } else {
                    self.error(format!("type `{name}` expects exactly one type argument"));
                    Ty::Error
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
                let parameters = match kind {
                    NominalKind::Struct => self.struct_templates[name]
                        .compile_groups
                        .iter()
                        .flatten()
                        .cloned()
                        .collect::<Vec<_>>(),
                    NominalKind::Enum => self.enum_templates[name]
                        .compile_groups
                        .iter()
                        .flatten()
                        .cloned()
                        .collect::<Vec<_>>(),
                };
                let expected = parameters.len();
                if source_arguments.len() != expected {
                    self.error(format!(
                        "type argument count mismatch for `{name}`: expected {expected}, found {}",
                        source_arguments.len()
                    ));
                    return Ty::Error;
                }
                let mut arguments = Vec::new();
                for (parameter, source) in parameters.iter().zip(source_arguments) {
                    let argument = match parameter.kind {
                        CompileParamKind::Type => self.lower_source_type(source),
                        _ => self
                            .probe_compile_argument_ty(parameter, source)
                            .unwrap_or(Ty::Error),
                    };
                    if argument == Ty::Error {
                        self.error(format!(
                            "invalid {} argument `{}` for generic type `{name}`",
                            describe_compile_param_kind(parameter.kind.clone()),
                            parameter.name
                        ));
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
            Ty::I8 => Some(Type::I8),
            Ty::I16 => Some(Type::I16),
            Ty::I32 => Some(Type::I32),
            Ty::I64 => Some(Type::I64),
            Ty::I128 => Some(Type::I128),
            Ty::ISize => Some(Type::ISize),
            Ty::U8 => Some(Type::U8),
            Ty::U16 => Some(Type::U16),
            Ty::U32 => Some(Type::U32),
            Ty::U64 => Some(Type::U64),
            Ty::U128 => Some(Type::U128),
            Ty::USize => Some(Type::USize),
            Ty::Bool => Some(Type::Bool),
            Ty::Unit => Some(Type::Unit),
            Ty::Tuple(fields) => Some(Type::Tuple(
                fields
                    .iter()
                    .map(|field| self.source_type_for_ty(field))
                    .collect::<Option<Vec<_>>>()?,
            )),
            Ty::Array(element, length) => Some(Type::ArrayApplication {
                constructor: self.lang_item_name(LangItemKind::ArrayTypeForm).to_owned(),
                element: Box::new(self.source_type_for_ty(element)?),
                length: USizeConst::Literal(*length),
            }),
            Ty::Pointer { pointee, mutable } => Some(Type::Named(
                self.lang_item_name(LangItemKind::PtrTypeForm).to_owned(),
                vec![
                    Type::Named(
                        if *mutable {
                            ACCESS_MUT_MARKER
                        } else {
                            ACCESS_SHARED_MARKER
                        }
                        .to_owned(),
                        Vec::new(),
                    ),
                    self.source_type_for_ty(pointee)?,
                ],
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
            Ty::Slice(element) => Some(Type::Named(
                self.lang_item_name(LangItemKind::SliceTypeForm).to_owned(),
                vec![self.source_type_for_ty(element)?],
            )),
            Ty::Struct(name) | Ty::Enum(name) => {
                if let Some(value) = usize_value_from_marker(name) {
                    return Some(Type::CompileUSize(value));
                }
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
            Ty::I8 => "i8".to_owned(),
            Ty::I16 => "i16".to_owned(),
            Ty::I32 => "i32".to_owned(),
            Ty::I64 => "i64".to_owned(),
            Ty::I128 => "i128".to_owned(),
            Ty::ISize => "isize".to_owned(),
            Ty::U8 => "u8".to_owned(),
            Ty::U16 => "u16".to_owned(),
            Ty::U32 => "u32".to_owned(),
            Ty::U64 => "u64".to_owned(),
            Ty::U128 => "u128".to_owned(),
            Ty::USize => "usize".to_owned(),
            Ty::Bool => "bool".to_owned(),
            Ty::Unit => "()".to_owned(),
            Ty::Tuple(fields) => {
                let mut rendered = fields
                    .iter()
                    .map(|field| self.diagnostic_type_name(field))
                    .collect::<Vec<_>>()
                    .join(", ");
                if fields.len() == 1 {
                    rendered.push(',');
                }
                format!("({rendered})")
            }
            Ty::Array(element, length) => {
                format!("Array({})({length})", self.diagnostic_type_name(element))
            }
            Ty::Slice(element) => format!("Slice({})", self.diagnostic_type_name(element)),
            Ty::Pointer { pointee, mutable } => format!(
                "{}({})",
                if *mutable { "Ptr(mut)" } else { "Ptr" },
                self.diagnostic_type_name(pointee)
            ),
            Ty::Reference {
                pointee,
                mutable,
                region,
            } => {
                let mode = if *mutable { "borrow(mut)" } else { "borrow" };
                let region = region.as_ref().map_or_else(String::new, |region| {
                    format!("({})", display_region_argument(region))
                });
                format!("{mode}{region} {}", self.diagnostic_type_name(pointee))
            }
            Ty::Struct(name) | Ty::Enum(name) => {
                if let Some(value) = usize_value_from_marker(name) {
                    return value.to_string();
                }
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
                    "i8" => Type::I8,
                    "i16" => Type::I16,
                    "i32" => Type::I32,
                    "i64" => Type::I64,
                    "i128" => Type::I128,
                    "isize" => Type::ISize,
                    "u8" => Type::U8,
                    "u16" => Type::U16,
                    "u32" => Type::U32,
                    "u64" => Type::U64,
                    "u128" => Type::U128,
                    "usize" => Type::USize,
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
                if self.is_lang_item_name(name, LangItemKind::ArrayTypeForm) {
                    if groups.len() != 2 || groups[0].len() != 1 || groups[1].len() != 1 {
                        self.error("`Array` type arguments require `Array(Element)(Length)`");
                        return None;
                    }
                    let element =
                        self.type_argument_from_expr(&groups[0][0].value, substitutions)?;
                    let length = match &groups[1][0].value {
                        Expr::Integer(length) => {
                            let Ok(length) = u64::try_from(*length) else {
                                self.error("array type argument length must fit in `u64`");
                                return None;
                            };
                            USizeConst::Literal(length)
                        }
                        Expr::Name(name) => match substitutions.get(name) {
                            Some(Type::CompileUSize(value)) => USizeConst::Literal(*value),
                            _ => USizeConst::Parameter(name.clone()),
                        },
                        _ => {
                            self.error(
                                "array type argument length must be a non-negative integer or `usize` parameter",
                            );
                            return None;
                        }
                    };
                    Some(Type::ArrayApplication {
                        constructor: name.clone(),
                        element: Box::new(element),
                        length,
                    })
                } else {
                    let mut arguments = Vec::new();
                    let compile_groups = self
                        .struct_templates
                        .get(name)
                        .map(|template| template.compile_groups.clone())
                        .or_else(|| {
                            self.enum_templates
                                .get(name)
                                .map(|template| template.compile_groups.clone())
                        });
                    if let Some(compile_groups) = compile_groups {
                        let parameters = compile_groups.iter().flatten().collect::<Vec<_>>();
                        let supplied = groups
                            .iter()
                            .flat_map(|group| group.iter())
                            .collect::<Vec<_>>();
                        if parameters.len() == supplied.len() {
                            for (parameter, argument) in parameters.into_iter().zip(supplied) {
                                let Some(source) = self.probe_compile_argument_source(
                                    parameter,
                                    &argument.value,
                                    substitutions,
                                ) else {
                                    self.error(format!(
                                        "invalid {} argument `{}` for generic type `{name}`",
                                        describe_compile_param_kind(parameter.kind.clone()),
                                        parameter.name
                                    ));
                                    return None;
                                };
                                arguments.push(source);
                            }
                        } else {
                            for argument in groups.iter().flat_map(|group| group.iter()) {
                                arguments.push(
                                    self.type_argument_from_expr(&argument.value, substitutions)?,
                                );
                            }
                        }
                    } else {
                        for argument in groups.iter().flat_map(|group| group.iter()) {
                            arguments.push(
                                self.type_argument_from_expr(&argument.value, substitutions)?,
                            );
                        }
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
                "compile-time argument `{parameter}` in `{owner}` expects kind {}, found a non-constructor expression",
                describe_compile_param_kind(CompileParamKind::TypeConstructor { parameter_count })
            ));
            return None;
        };
        let Some(target) =
            self.type_constructor_impl_target(&Type::Named(name.clone(), Vec::new()))
        else {
            self.error(format!(
                "compile-time argument `{parameter}` in `{owner}` expects kind {}, but `{name}` is not a generic type constructor",
                describe_compile_param_kind(CompileParamKind::TypeConstructor { parameter_count })
            ));
            return None;
        };
        if target.parameter_count != parameter_count {
            self.error(format!(
                "compile-time argument `{parameter}` in `{owner}` expects kind {}, but constructor `{name}` has {} type parameter{}",
                describe_compile_param_kind(CompileParamKind::TypeConstructor { parameter_count }),
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
                    "i8" => Type::I8,
                    "i16" => Type::I16,
                    "i32" => Type::I32,
                    "i64" => Type::I64,
                    "i128" => Type::I128,
                    "isize" => Type::ISize,
                    "u8" => Type::U8,
                    "u16" => Type::U16,
                    "u32" => Type::U32,
                    "u64" => Type::U64,
                    "u128" => Type::U128,
                    "usize" => Type::USize,
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
                if self.is_lang_item_name(name, LangItemKind::ArrayTypeForm) {
                    if groups.len() != 2 || groups[0].len() != 1 || groups[1].len() != 1 {
                        return None;
                    }
                    let element =
                        self.probe_type_argument_source(&groups[0][0].value, substitutions)?;
                    let length = match &groups[1][0].value {
                        Expr::Integer(length) => USizeConst::Literal(u64::try_from(*length).ok()?),
                        Expr::Name(name) => match substitutions.get(name) {
                            Some(Type::CompileUSize(value)) => USizeConst::Literal(*value),
                            _ => USizeConst::Parameter(name.clone()),
                        },
                        _ => return None,
                    };
                    Some(Type::ArrayApplication {
                        constructor: name.clone(),
                        element: Box::new(element),
                        length,
                    })
                } else {
                    let compile_groups = self
                        .struct_templates
                        .get(name)
                        .map(|template| &template.compile_groups)
                        .or_else(|| {
                            self.enum_templates
                                .get(name)
                                .map(|template| &template.compile_groups)
                        });
                    let arguments = if let Some(compile_groups) = compile_groups {
                        let parameters = compile_groups.iter().flatten().collect::<Vec<_>>();
                        let supplied = groups
                            .iter()
                            .flat_map(|group| group.iter())
                            .collect::<Vec<_>>();
                        if parameters.len() == supplied.len() {
                            let mut arguments = Vec::new();
                            for (parameter, argument) in parameters.into_iter().zip(supplied) {
                                arguments.push(self.probe_compile_argument_source(
                                    parameter,
                                    &argument.value,
                                    substitutions,
                                )?);
                            }
                            arguments
                        } else {
                            groups
                                .iter()
                                .flat_map(|group| group.iter())
                                .map(|argument| {
                                    self.probe_type_argument_source(&argument.value, substitutions)
                                })
                                .collect::<Option<Vec<_>>>()?
                        }
                    } else {
                        groups
                            .iter()
                            .flat_map(|group| group.iter())
                            .map(|argument| {
                                self.probe_type_argument_source(&argument.value, substitutions)
                            })
                            .collect::<Option<Vec<_>>>()?
                    };
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
        match parameter.kind.clone() {
            CompileParamKind::Type => self.probe_type_argument_source(expression, substitutions),
            CompileParamKind::USize => match expression {
                Expr::Integer(value) => Some(Type::CompileUSize(u64::try_from(*value).ok()?)),
                Expr::Name(name) => substitutions.get(name).and_then(|value| {
                    matches!(value, Type::CompileUSize(_)).then(|| value.clone())
                }),
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
            CompileParamKind::ParameterModifier => {
                self.probe_parameter_modifier_source(expression, substitutions)
            }
            CompileParamKind::Region
            | CompileParamKind::Parameters
            | CompileParamKind::ParameterPack
            | CompileParamKind::EffectConstructor { .. } => None,
            CompileParamKind::Named(compile_type) => match expression {
                Expr::Bool(value)
                    if self
                        .closed_type_values
                        .get(&compile_type)
                        .is_some_and(|members| {
                            members.contains(&if *value {
                                "true".to_owned()
                            } else {
                                "false".to_owned()
                            })
                        }) =>
                {
                    Some(Type::Named(
                        closed_value_marker(&compile_type, if *value { "true" } else { "false" }),
                        Vec::new(),
                    ))
                }
                Expr::Name(name)
                    if self
                        .closed_type_values
                        .get(&compile_type)
                        .is_some_and(|members| members.contains(name)) =>
                {
                    Some(Type::Named(
                        closed_value_marker(&compile_type, name),
                        Vec::new(),
                    ))
                }
                Expr::Name(name)
                    if closed_value_from_marker(name)
                        .is_some_and(|(owner, _)| owner == compile_type) =>
                {
                    Some(Type::Named(name.clone(), Vec::new()))
                }
                Expr::Name(name) => substitutions.get(name).and_then(|value| {
                    let Type::Named(marker, arguments) = value else {
                        return None;
                    };
                    (arguments.is_empty()
                        && closed_value_from_marker(marker)
                            .is_some_and(|(owner, _)| owner == compile_type))
                    .then(|| value.clone())
                }),
                _ => None,
            },
        }
    }

    pub(super) fn probe_compile_argument_ty(
        &self,
        parameter: &CompileParam,
        source: &Type,
    ) -> Option<Ty> {
        match parameter.kind.clone() {
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
            CompileParamKind::Type | CompileParamKind::Effect | CompileParamKind::Named(_) => {
                self.probe_source_ty(source)
            }
            CompileParamKind::USize => match source {
                Type::CompileUSize(value) => Some(Ty::Struct(usize_value_marker(*value))),
                _ => None,
            },
            CompileParamKind::Region
            | CompileParamKind::Parameters
            | CompileParamKind::ParameterPack
            | CompileParamKind::ParameterModifier
            | CompileParamKind::EffectConstructor { .. } => None,
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
            .all(|parameter| parameter.kind == CompileParamKind::USize)
        {
            return arguments.iter().all(|argument| {
                matches!(argument.value, Expr::Integer(_))
                    || matches!(&argument.value, Expr::Name(name)
                    if context.type_substitutions.get(name).is_some_and(
                        |value| matches!(value, Type::CompileUSize(_))
                    ))
            });
        }
        if parameters
            .iter()
            .all(|parameter| parameter.kind.is_access())
        {
            return parameters
                .iter()
                .zip(arguments)
                .all(|(parameter, argument)| {
                    self.expression_is_explicit_compile_argument(
                        parameter,
                        &argument.value,
                        context,
                        unit_is_type,
                    )
                });
        }
        if parameters
            .iter()
            .all(|parameter| parameter.kind.is_parameter_modifier())
        {
            return arguments.iter().all(|argument| {
                matches!(&argument.value, Expr::Name(name)
                    if matches!(name.rsplit("::").next().unwrap_or(name), "copy" | "move"))
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
        match parameter.kind.clone() {
            CompileParamKind::Type => {
                (unit_is_type && matches!(expression, Expr::Unit))
                    || self.expression_is_explicit_type_argument(expression, context)
            }
            CompileParamKind::USize => {
                matches!(expression, Expr::Integer(_))
                    || matches!(expression, Expr::Name(name)
                    if context.type_substitutions.get(name).is_some_and(
                        |value| matches!(value, Type::CompileUSize(_))
                    ))
            }
            CompileParamKind::Effect => self.expression_is_explicit_effect_argument(expression),
            CompileParamKind::TypeConstructor { parameter_count } => {
                self.expression_is_explicit_type_constructor_argument(expression, parameter_count)
            }
            CompileParamKind::Region => false,
            CompileParamKind::Parameters => false,
            CompileParamKind::ParameterPack => false,
            CompileParamKind::ParameterModifier => self
                .probe_parameter_modifier_source(expression, &context.type_substitutions)
                .is_some(),
            CompileParamKind::EffectConstructor { .. } => false,
            CompileParamKind::Named(compile_type) => match expression {
                Expr::Bool(value) => {
                    self.closed_type_values
                        .get(&compile_type)
                        .is_some_and(|members| {
                            members.contains(&if *value {
                                "true".to_owned()
                            } else {
                                "false".to_owned()
                            })
                        })
                }
                Expr::Name(name) => {
                    self.closed_type_values
                        .get(&compile_type)
                        .is_some_and(|members| members.contains(name))
                        || closed_value_from_marker(name)
                            .is_some_and(|(owner, _)| owner == compile_type)
                        || context.type_substitutions.get(name).is_some_and(|value| {
                            matches!(
                                value,
                                Type::Named(marker, arguments)
                                    if arguments.is_empty()
                                        && closed_value_from_marker(marker)
                                            .is_some_and(|(owner, _)| owner == compile_type)
                            )
                        })
                }
                _ => false,
            },
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

    pub(super) fn probe_parameter_modifier_source(
        &self,
        expression: &Expr,
        substitutions: &HashMap<String, Type>,
    ) -> Option<Type> {
        match expression {
            Expr::Name(name) => {
                if let Some(source) = substitutions.get(name) {
                    return Some(source.clone());
                }
                match name.rsplit("::").next().unwrap_or(name) {
                    "copy" => Some(Type::Named(
                        PARAMETER_MODIFIER_COPY_MARKER.to_owned(),
                        Vec::new(),
                    )),
                    "move" => Some(Type::Named(
                        PARAMETER_MODIFIER_MOVE_MARKER.to_owned(),
                        Vec::new(),
                    )),
                    _ => None,
                }
            }
            Expr::Call(callee, arguments)
                if arguments.len() == 1
                    && arguments[0].label.is_none()
                    && matches!(
                        callee.as_ref(),
                        Expr::Name(name)
                            if self.transparent_parameter_modifiers.contains(name)
                    ) =>
            {
                self.probe_parameter_modifier_source(&arguments[0].value, substitutions)
            }
            _ => None,
        }
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
                        "i8" | "i16"
                            | "i32"
                            | "i64"
                            | "i128"
                            | "isize"
                            | "u8"
                            | "u16"
                            | "u32"
                            | "u64"
                            | "u128"
                            | "usize"
                            | "bool"
                            | "Never"
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
                if self.is_lang_item_name(name, LangItemKind::ArrayTypeForm) {
                    return groups.len() == 2
                        && groups[0].len() == 1
                        && groups[1].len() == 1
                        && self.expression_is_explicit_type_argument(&groups[0][0].value, context)
                        && matches!(groups[1][0].value, Expr::Integer(_));
                }
                self.struct_templates.contains_key(name) || self.enum_templates.contains_key(name)
            }
            _ => false,
        }
    }

    pub(super) fn probe_source_ty(&self, source: &Type) -> Option<Ty> {
        match source {
            Type::I8 => Some(Ty::I8),
            Type::I16 => Some(Ty::I16),
            Type::I32 => Some(Ty::I32),
            Type::I64 => Some(Ty::I64),
            Type::I128 => Some(Ty::I128),
            Type::ISize => Some(Ty::ISize),
            Type::U8 => Some(Ty::U8),
            Type::U16 => Some(Ty::U16),
            Type::U32 => Some(Ty::U32),
            Type::U64 => Some(Ty::U64),
            Type::U128 => Some(Ty::U128),
            Type::USize => Some(Ty::USize),
            Type::Bool => Some(Ty::Bool),
            Type::Unit => Some(Ty::Unit),
            Type::Tuple(fields) => Some(Ty::Tuple(
                fields
                    .iter()
                    .map(|field| self.probe_source_ty(field))
                    .collect::<Option<Vec<_>>>()?,
            )),
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
            Type::ArrayApplication {
                constructor,
                element,
                length: USizeConst::Literal(length),
            } if self.is_lang_item_name(constructor, LangItemKind::ArrayTypeForm) => {
                Some(Ty::Array(Box::new(self.probe_source_ty(element)?), *length))
            }
            Type::ArrayApplication { .. } | Type::CompileUSize(_) => None,
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
                if self.is_lang_item_name(name, LangItemKind::PtrTypeForm) =>
            {
                let (access, pointee) = match arguments.as_slice() {
                    [pointee] => (ACCESS_SHARED_MARKER, pointee),
                    [Type::Named(access, access_arguments), pointee]
                        if access_arguments.is_empty() =>
                    {
                        (access.as_str(), pointee)
                    }
                    _ => return None,
                };
                if !matches!(
                    access,
                    "shared" | "mut" | ACCESS_SHARED_MARKER | ACCESS_MUT_MARKER
                ) {
                    return None;
                }
                Some(Ty::Pointer {
                    pointee: Box::new(self.probe_source_ty(pointee)?),
                    mutable: matches!(access, "mut" | ACCESS_MUT_MARKER),
                })
            }
            Type::Named(name, arguments)
                if self.is_lang_item_name(name, LangItemKind::SliceTypeForm) =>
            {
                let [element] = arguments.as_slice() else {
                    return None;
                };
                Some(Ty::Slice(Box::new(self.probe_source_ty(element)?)))
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

use std::collections::{HashMap, HashSet};

use crate::ast::{CallArg, CompileParam, CompileParamKind, Expr, Type};
use crate::core::LangItemKind;

use super::compile_time::{
    effect_row_from_marker, effect_row_source, source_effect_identity, type_constructor_marker,
    ACCESS_MUT_MARKER, ACCESS_SHARED_MARKER, PASSING_AUTO_MARKER, PASSING_COPY_MARKER,
    PASSING_MOVE_MARKER,
};
use super::flow::LowerCtx;
use super::hir::{FunctionTy, Ty};
use super::lower::{InferredTypeArgument, TypeProbe};
use super::names::nominal_instance_name;
use super::registry::{NominalInstanceKey, NominalKind};
use super::Analyzer;

impl Analyzer {
    pub(super) fn unify_template_ty(
        &self,
        template: &Type,
        actual: &Ty,
        actual_source: Option<&Type>,
        compile_parameters: &HashSet<String>,
        inferred: &mut HashMap<String, InferredTypeArgument>,
        origin: &str,
    ) -> Result<bool, String> {
        let mismatch = || {
            format!("type inference constraint from {origin} does not match actual type `{actual}`")
        };
        if let Type::Named(name, arguments) = template {
            if arguments.is_empty() && compile_parameters.contains(name) {
                if let Some(previous) = inferred.get_mut(name) {
                    if previous.ty == *actual {
                        if previous.source.is_none() {
                            previous.source = actual_source
                                .cloned()
                                .or_else(|| self.source_type_for_ty(actual));
                        }
                        return Ok(false);
                    }
                    return Err(format!(
                        "conflicting inference for type parameter `{name}`: `{}` from {} conflicts with `{actual}` from {origin}",
                        previous.ty, previous.origin
                    ));
                }
                if *actual == Ty::Error || self.is_uninhabited_type(actual) {
                    return Err(format!(
                        "cannot infer type parameter `{name}` from `{actual}` in {origin}"
                    ));
                }
                inferred.insert(
                    name.clone(),
                    InferredTypeArgument {
                        ty: actual.clone(),
                        source: actual_source
                            .cloned()
                            .or_else(|| self.source_type_for_ty(actual)),
                        origin: origin.to_owned(),
                    },
                );
                return Ok(true);
            }
            if !arguments.is_empty() && compile_parameters.contains(name) {
                let (actual_template, actual_sources, actual_types) = match actual_source {
                    Some(Type::Named(actual_template, actual_sources))
                        if !actual_sources.is_empty() =>
                    {
                        let actual_types = actual_sources
                            .iter()
                            .map(|source| self.probe_source_ty(source))
                            .collect::<Option<Vec<_>>>()
                            .ok_or_else(mismatch)?;
                        (
                            actual_template.clone(),
                            actual_sources.clone(),
                            actual_types,
                        )
                    }
                    _ => {
                        let actual_name = match actual {
                            Ty::Struct(name) | Ty::Enum(name) => name,
                            _ => return Err(mismatch()),
                        };
                        let Some(instance) = self.nominal_instances.get(actual_name) else {
                            return Err(mismatch());
                        };
                        let actual_sources = instance
                            .key
                            .arguments
                            .iter()
                            .map(|argument| self.source_type_for_ty(argument))
                            .collect::<Option<Vec<_>>>()
                            .ok_or_else(mismatch)?;
                        (
                            instance.key.template.clone(),
                            actual_sources,
                            instance.key.arguments.clone(),
                        )
                    }
                };
                if actual_sources.len() != arguments.len() {
                    return Err(mismatch());
                }
                let selected = InferredTypeArgument {
                    ty: Ty::Struct(type_constructor_marker(&actual_template)),
                    source: Some(Type::Named(actual_template.clone(), Vec::new())),
                    origin: origin.to_owned(),
                };
                match inferred.get(name) {
                    Some(previous) if previous.ty != selected.ty => {
                        return Err(format!(
                            "conflicting inference for type-constructor parameter `{name}` from {} and {origin}",
                            previous.origin
                        ));
                    }
                    Some(_) => {}
                    None => {
                        inferred.insert(name.clone(), selected);
                    }
                }
                let mut changed = false;
                for ((template_argument, actual_ty), actual_source) in
                    arguments.iter().zip(&actual_types).zip(&actual_sources)
                {
                    changed |= self.unify_template_ty(
                        template_argument,
                        actual_ty,
                        Some(actual_source),
                        compile_parameters,
                        inferred,
                        origin,
                    )?;
                }
                return Ok(changed);
            }
        }

        match template {
            Type::I32 => (*actual == Ty::I32).then_some(false).ok_or_else(mismatch),
            Type::I64 => (*actual == Ty::I64).then_some(false).ok_or_else(mismatch),
            Type::U32 => (*actual == Ty::U32).then_some(false).ok_or_else(mismatch),
            Type::U64 => (*actual == Ty::U64).then_some(false).ok_or_else(mismatch),
            Type::Bool => (*actual == Ty::Bool).then_some(false).ok_or_else(mismatch),
            Type::Unit => (*actual == Ty::Unit).then_some(false).ok_or_else(mismatch),
            Type::Borrow {
                mutable,
                access,
                region,
                pointee,
            } => {
                let Ty::Reference {
                    pointee: actual_pointee,
                    mutable: actual_mutable,
                    region: actual_region,
                } = actual
                else {
                    return Err(mismatch());
                };
                let mut changed = false;
                if let Some(access) = access {
                    let marker = if *actual_mutable {
                        ACCESS_MUT_MARKER
                    } else {
                        ACCESS_SHARED_MARKER
                    };
                    let selected = InferredTypeArgument {
                        ty: Ty::Struct(marker.to_owned()),
                        source: Some(Type::Named(marker.to_owned(), Vec::new())),
                        origin: origin.to_owned(),
                    };
                    match inferred.get(access) {
                        Some(previous)
                            if previous.origin != "default shared access"
                                && previous.ty != selected.ty =>
                        {
                            return Err(format!(
                                "conflicting inference for access parameter `{access}`: `{}` from {} conflicts with `{}` from {origin}",
                                if previous.ty == Ty::Struct(ACCESS_MUT_MARKER.to_owned()) {
                                    "mut"
                                } else {
                                    "shared"
                                },
                                previous.origin,
                                if *actual_mutable { "mut" } else { "shared" }
                            ));
                        }
                        Some(previous) if previous.ty == selected.ty => {}
                        _ => {
                            inferred.insert(access.clone(), selected);
                            changed = true;
                        }
                    }
                } else if mutable != actual_mutable {
                    return Err(mismatch());
                }
                if region != actual_region {
                    return Err(mismatch());
                }
                self.unify_template_ty(
                    pointee,
                    actual_pointee,
                    match actual_source {
                        Some(Type::Borrow { pointee, .. }) => Some(pointee),
                        _ => None,
                    },
                    compile_parameters,
                    inferred,
                    origin,
                )
                .map(|pointee_changed| changed || pointee_changed)
            }
            Type::Array(element, length) => {
                let Ty::Array(actual_element, actual_length) = actual else {
                    return Err(mismatch());
                };
                if length != actual_length {
                    return Err(mismatch());
                }
                self.unify_template_ty(
                    element,
                    actual_element,
                    match actual_source {
                        Some(Type::Array(element, _)) => Some(element),
                        _ => None,
                    },
                    compile_parameters,
                    inferred,
                    origin,
                )
            }
            Type::Function {
                groups,
                effects,
                result,
            } => {
                let actual_function = match actual {
                    Ty::Function(function) => function,
                    Ty::Callable(callable) => &callable.signature,
                    _ => return Err(mismatch()),
                };
                if groups.len() != actual_function.groups.len()
                    || groups
                        .iter()
                        .zip(&actual_function.groups)
                        .any(|(left, right)| left.len() != right.len())
                {
                    return Err(mismatch());
                }
                let (throws_changed, selected_throws) = match (
                    effects.throws.as_deref(),
                    actual_function.throws_error.as_deref(),
                ) {
                    (None, None) => (false, None),
                    (None, Some(actual_error)) if !effects.parameters.is_empty() => {
                        (false, Some(actual_error.clone()))
                    }
                    (Some(template_error), Some(actual_error)) => (
                        self.unify_template_ty(
                            template_error,
                            actual_error,
                            None,
                            compile_parameters,
                            inferred,
                            origin,
                        )?,
                        None,
                    ),
                    _ => return Err(mismatch()),
                };
                let template_unsafe = self.function_effects_unsafe(effects);
                let fixed_custom = self.function_effects_custom_identities(effects);
                if effects.parameters.is_empty()
                    && ((actual_function.unsafe_effect && !template_unsafe)
                        || actual_function
                            .custom_effects
                            .iter()
                            .any(|effect| !fixed_custom.contains(effect)))
                {
                    return Err(mismatch());
                }
                let mut changed = throws_changed;
                let selected_unsafe = actual_function.unsafe_effect && !template_unsafe;
                let selected_custom = actual_function
                    .custom_effects
                    .iter()
                    .filter(|effect| !fixed_custom.contains(*effect))
                    .cloned()
                    .collect::<Vec<_>>();
                for parameter in &effects.parameters {
                    let source_error = selected_throws
                        .as_ref()
                        .and_then(|error| self.source_type_for_ty(error));
                    if selected_throws.is_some() && source_error.is_none() {
                        return Err(format!(
                            "cannot preserve the thrown error type while inferring effect parameter `{parameter}` from {origin}"
                        ));
                    }
                    let source = effect_row_source(selected_unsafe, source_error, &selected_custom);
                    let selected = InferredTypeArgument {
                        ty: Ty::EffectRow {
                            unsafe_effect: selected_unsafe,
                            throws_error: selected_throws.clone().map(Box::new),
                            custom_effects: selected_custom.clone(),
                        },
                        source: Some(source),
                        origin: origin.to_owned(),
                    };
                    match inferred.get(parameter) {
                        Some(previous)
                            if previous.origin != "default pure effect"
                                && previous.ty != selected.ty =>
                        {
                            return Err(format!(
                                "conflicting inference for effect parameter `{parameter}` from {} and {origin}",
                                previous.origin
                            ));
                        }
                        Some(previous) if previous.ty == selected.ty => {}
                        _ => {
                            inferred.insert(parameter.clone(), selected);
                            changed = true;
                        }
                    }
                }
                let actual_source_function = match actual_source {
                    Some(Type::Function { groups, result, .. }) => Some((groups, result.as_ref())),
                    _ => None,
                };
                for (group_index, (templates, actuals)) in
                    groups.iter().zip(&actual_function.groups).enumerate()
                {
                    for (parameter_index, (template, actual)) in
                        templates.iter().zip(actuals).enumerate()
                    {
                        let source = actual_source_function
                            .and_then(|(groups, _)| groups.get(group_index))
                            .and_then(|group| group.get(parameter_index));
                        changed |= self.unify_template_ty(
                            template,
                            actual,
                            source,
                            compile_parameters,
                            inferred,
                            origin,
                        )?;
                    }
                }
                let actual_logical_result = if actual_function.throws_error.is_some() {
                    self.standard_fallible_info_for_ty(&actual_function.result)
                        .map(|info| info.payload)
                        .ok_or_else(mismatch)?
                } else {
                    (*actual_function.result).clone()
                };
                let actual_logical_source = if actual_function.throws_error.is_some() {
                    actual_source_function.and_then(|(_, result)| match result {
                        Type::Named(_, arguments) if arguments.len() == 2 => arguments.first(),
                        _ => None,
                    })
                } else {
                    actual_source_function.map(|(_, result)| result)
                };
                changed |= self.unify_template_ty(
                    result,
                    &actual_logical_result,
                    actual_logical_source,
                    compile_parameters,
                    inferred,
                    origin,
                )?;
                Ok(changed)
            }
            Type::Named(name, arguments) if name == "()" && arguments.is_empty() => {
                if *actual == Ty::Unit {
                    Ok(false)
                } else {
                    Err(mismatch())
                }
            }
            Type::Named(name, arguments)
                if matches!(name.as_str(), "Ptr" | "MutPtr") && arguments.len() == 1 =>
            {
                let Ty::Pointer { pointee, mutable } = actual else {
                    return Err(mismatch());
                };
                if *mutable != (name == "MutPtr") {
                    return Err(mismatch());
                }
                self.unify_template_ty(
                    &arguments[0],
                    pointee,
                    match actual_source {
                        Some(Type::Named(actual_name, actual_arguments))
                            if actual_name == name && actual_arguments.len() == 1 =>
                        {
                            Some(&actual_arguments[0])
                        }
                        _ => None,
                    },
                    compile_parameters,
                    inferred,
                    origin,
                )
            }
            Type::Named(name, arguments) => {
                let (actual_kind, actual_name) = match actual {
                    Ty::Struct(name) => (NominalKind::Struct, name),
                    Ty::Enum(name) => (NominalKind::Enum, name),
                    _ => return Err(mismatch()),
                };
                if arguments.is_empty() && name == actual_name {
                    return Ok(false);
                }
                if let Some(instance) = self.nominal_instances.get(actual_name) {
                    if instance.key.kind != actual_kind
                        || instance.key.template != *name
                        || instance.key.arguments.len() != arguments.len()
                    {
                        return Err(mismatch());
                    }
                    let actual_arguments = instance.key.arguments.clone();
                    let mut changed = false;
                    for (template, actual) in arguments.iter().zip(&actual_arguments) {
                        changed |= self.unify_template_ty(
                            template,
                            actual,
                            None,
                            compile_parameters,
                            inferred,
                            origin,
                        )?;
                    }
                    Ok(changed)
                } else if let Some(Type::Named(actual_template, source_arguments)) = actual_source {
                    if actual_template != name || source_arguments.len() != arguments.len() {
                        return Err(mismatch());
                    }
                    let mut changed = false;
                    for (template, source) in arguments.iter().zip(source_arguments) {
                        let Some(actual) = self.probe_source_ty(source) else {
                            return Err(mismatch());
                        };
                        changed |= self.unify_template_ty(
                            template,
                            &actual,
                            Some(source),
                            compile_parameters,
                            inferred,
                            origin,
                        )?;
                    }
                    Ok(changed)
                } else {
                    Err(mismatch())
                }
            }
            Type::NamedArgs(name, _) => Err(format!(
                "internal error: labeled type arguments for `{name}` were not normalized before type inference"
            )),
        }
    }

    pub(super) fn unify_source_template(
        &self,
        template: &Type,
        actual: &Type,
        compile_parameters: &HashSet<String>,
        inferred: &mut HashMap<String, InferredTypeArgument>,
        origin: &str,
    ) -> Result<bool, String> {
        if let Some(actual_ty) = self.probe_source_ty(actual) {
            return self.unify_template_ty(
                template,
                &actual_ty,
                Some(actual),
                compile_parameters,
                inferred,
                origin,
            );
        }
        let mismatch = || {
            format!(
                "source type inference constraint from {origin} does not match `{}`",
                source_effect_identity(actual)
            )
        };
        match (template, actual) {
            (
                Type::Named(template_name, template_arguments),
                Type::Named(actual_name, actual_arguments),
            ) if template_name == actual_name
                && template_arguments.len() == actual_arguments.len() =>
            {
                let mut changed = false;
                for (template_argument, actual_argument) in
                    template_arguments.iter().zip(actual_arguments)
                {
                    changed |= self.unify_source_template(
                        template_argument,
                        actual_argument,
                        compile_parameters,
                        inferred,
                        origin,
                    )?;
                }
                Ok(changed)
            }
            _ => Err(mismatch()),
        }
    }

    pub(super) fn resolved_template_ty(
        &self,
        template: &Type,
        compile_parameters: &HashSet<String>,
        inferred: &HashMap<String, InferredTypeArgument>,
    ) -> Option<Ty> {
        match template {
            Type::I32 => Some(Ty::I32),
            Type::I64 => Some(Ty::I64),
            Type::U32 => Some(Ty::U32),
            Type::U64 => Some(Ty::U64),
            Type::Bool => Some(Ty::Bool),
            Type::Unit => Some(Ty::Unit),
            Type::Borrow {
                mutable,
                region,
                pointee,
                ..
            } => Some(Ty::Reference {
                pointee: Box::new(self.resolved_template_ty(
                    pointee,
                    compile_parameters,
                    inferred,
                )?),
                mutable: *mutable,
                region: region.clone(),
            }),
            Type::Array(element, length) => Some(Ty::Array(
                Box::new(self.resolved_template_ty(element, compile_parameters, inferred)?),
                *length,
            )),
            Type::Function {
                groups,
                effects,
                result,
            } => {
                let mut unsafe_effect = self.function_effects_unsafe(effects);
                let mut throws_error = match effects.throws.as_deref() {
                    Some(error) => Some(Box::new(self.resolved_template_ty(
                        error,
                        compile_parameters,
                        inferred,
                    )?)),
                    None => None,
                };
                let mut custom_effects = self.function_effects_custom_identities(effects);
                for parameter in &effects.parameters {
                    let Ty::EffectRow {
                        unsafe_effect: selected_unsafe,
                        throws_error: selected_throws,
                        custom_effects: selected_custom,
                    } = &inferred.get(parameter)?.ty
                    else {
                        return None;
                    };
                    if let Some(selected_throws) = selected_throws {
                        if throws_error
                            .as_ref()
                            .is_some_and(|fixed| **fixed != **selected_throws)
                        {
                            return None;
                        }
                        throws_error = Some(selected_throws.clone());
                    }
                    if custom_effects
                        .iter()
                        .any(|effect| selected_custom.contains(effect))
                    {
                        // Duplicate row members are normalized below.
                    }
                    if selected_custom.iter().any(|effect| effect.is_empty()) {
                        return None;
                    }
                    unsafe_effect |= *selected_unsafe;
                    custom_effects.extend(selected_custom.clone());
                }
                custom_effects.sort();
                custom_effects.dedup();
                Some(Ty::Function(FunctionTy {
                    groups: groups
                        .iter()
                        .map(|group| {
                            group
                                .iter()
                                .map(|ty| {
                                    self.resolved_template_ty(ty, compile_parameters, inferred)
                                })
                                .collect::<Option<Vec<_>>>()
                        })
                        .collect::<Option<Vec<_>>>()?,
                    unsafe_effect,
                    throws_error,
                    custom_effects,
                    result: Box::new(self.resolved_template_ty(
                        result,
                        compile_parameters,
                        inferred,
                    )?),
                }))
            }
            Type::Named(name, arguments)
                if arguments.is_empty() && compile_parameters.contains(name) =>
            {
                inferred.get(name).map(|argument| argument.ty.clone())
            }
            Type::Named(name, arguments) if name == "()" && arguments.is_empty() => Some(Ty::Unit),
            Type::Named(name, arguments) => {
                let arguments = arguments
                    .iter()
                    .map(|argument| {
                        self.resolved_template_ty(argument, compile_parameters, inferred)
                    })
                    .collect::<Option<Vec<_>>>()?;
                if self.struct_templates.contains_key(name) {
                    let key = NominalInstanceKey {
                        kind: NominalKind::Struct,
                        template: name.clone(),
                        arguments,
                    };
                    Some(Ty::Struct(
                        self.nominal_instance_names
                            .get(&key)
                            .cloned()
                            .unwrap_or_else(|| nominal_instance_name(&key)),
                    ))
                } else if self.enum_templates.contains_key(name) {
                    let key = NominalInstanceKey {
                        kind: NominalKind::Enum,
                        template: name.clone(),
                        arguments,
                    };
                    Some(Ty::Enum(
                        self.nominal_instance_names
                            .get(&key)
                            .cloned()
                            .unwrap_or_else(|| nominal_instance_name(&key)),
                    ))
                } else if arguments.is_empty() && self.struct_defs.contains_key(name) {
                    Some(Ty::Struct(name.clone()))
                } else if arguments.is_empty() && self.enum_defs.contains_key(name) {
                    Some(Ty::Enum(name.clone()))
                } else if arguments.is_empty() && self.abstract_type_parameters.contains_key(name) {
                    Some(Ty::Struct(name.clone()))
                } else {
                    None
                }
            }
            Type::NamedArgs(_, _) => None,
        }
    }

    pub(super) fn seed_type_argument_inference(
        &mut self,
        owner: &str,
        compile_groups: &[Vec<CompileParam>],
        groups: &[&[CallArg]],
        context: &LowerCtx,
        unit_is_type: bool,
    ) -> Option<(
        HashSet<String>,
        HashMap<String, InferredTypeArgument>,
        usize,
    )> {
        let compile_parameters: HashSet<_> = compile_groups
            .iter()
            .flatten()
            .filter(|parameter| {
                matches!(
                    parameter.kind,
                    CompileParamKind::Type | CompileParamKind::TypeConstructor { .. }
                )
            })
            .map(|parameter| parameter.name.clone())
            .collect();
        let mut inferred = HashMap::new();
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
                    unit_is_type,
                )
            {
                Some(compile_index)
            } else {
                None
            };
            let Some(target) = target else {
                break;
            };
            let parameters = &compile_groups[target];
            if !labeled && arguments.len() != parameters.len() {
                self.error(format!(
                    "type argument count mismatch in group {} of `{owner}`: expected {}, found {}",
                    target + 1,
                    parameters.len(),
                    arguments.len()
                ));
                return None;
            }
            let mut seen = HashSet::new();
            for (position, argument) in arguments.iter().enumerate() {
                let parameter = if let Some(label) = argument.label.as_deref() {
                    if !seen.insert(label) {
                        self.error(format!(
                            "duplicate compile-time argument `{label}` in `{owner}`"
                        ));
                        return None;
                    }
                    parameters
                        .iter()
                        .find(|parameter| parameter.name == label)
                        .expect("target compile group contains every argument label")
                } else {
                    &parameters[position]
                };
                let source = match parameter.kind {
                    CompileParamKind::Type => {
                        self.type_argument_from_expr(&argument.value, &context.type_substitutions)?
                    }
                    CompileParamKind::Access => match &argument.value {
                        Expr::Name(name) if name == "shared" => {
                            Type::Named(ACCESS_SHARED_MARKER.to_owned(), Vec::new())
                        }
                        Expr::Name(name) if name == "mut" => {
                            Type::Named(ACCESS_MUT_MARKER.to_owned(), Vec::new())
                        }
                        _ => {
                            self.error(format!(
                                "invalid access argument for `{}` in `{owner}`; expected `shared` or `mut`",
                                parameter.name
                            ));
                            return None;
                        }
                    },
                    CompileParamKind::Passing => match &argument.value {
                        Expr::Name(name) if name == "auto" => {
                            Type::Named(PASSING_AUTO_MARKER.to_owned(), Vec::new())
                        }
                        Expr::Name(name) if name == "copy" => {
                            Type::Named(PASSING_COPY_MARKER.to_owned(), Vec::new())
                        }
                        Expr::Name(name) if name == "move" => {
                            Type::Named(PASSING_MOVE_MARKER.to_owned(), Vec::new())
                        }
                        _ => {
                            self.error(format!(
                                "invalid passing argument for `{}` in `{owner}`; expected `auto`, `copy`, or `move`",
                                parameter.name
                            ));
                            return None;
                        }
                    },
                    CompileParamKind::Effect => match &argument.value {
                        Expr::Name(name) if name == "pure" => effect_row_source(false, None, &[]),
                        Expr::Name(name)
                            if name == self.lang_item_name(LangItemKind::UnsafeEffect) =>
                        {
                            effect_row_source(true, None, &[])
                        }
                        Expr::Name(name) if self.effects.contains(name) => {
                            effect_row_source(false, None, std::slice::from_ref(name))
                        }
                        Expr::Name(name) if effect_row_from_marker(name).is_some() => {
                            Type::Named(name.clone(), Vec::new())
                        }
                        Expr::Call(callee, arguments)
                            if matches!(
                                callee.as_ref(),
                                Expr::Name(name)
                                    if name == self.lang_item_name(LangItemKind::UnsafeEffect)
                                        && arguments.is_empty()
                            ) =>
                        {
                            effect_row_source(true, None, &[])
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
                                    self.error(format!(
                                        "effect argument `{}` in `{owner}` does not support labeled constructor arguments yet",
                                        parameter.name
                                    ));
                                    return None;
                                }
                                source_arguments.push(self.type_argument_from_expr(
                                    &argument.value,
                                    &context.type_substitutions,
                                )?);
                            }
                            let effect = Type::Named(name.clone(), source_arguments);
                            if self.is_standard_unsafe_effect_source(&effect) {
                                effect_row_source(true, None, &[])
                            } else {
                                effect_row_source(false, None, &[source_effect_identity(&effect)])
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
                                Some(argument) => Some(self.type_argument_from_expr(
                                    &argument.value,
                                    &context.type_substitutions,
                                )?),
                                None => None,
                            };
                            Type::Named(marker.clone(), error.into_iter().collect())
                        }
                        _ => {
                            self.error(format!(
                                "invalid effect argument for `{}` in `{owner}`; expected `pure`, `Unsafe`, `Throws(Error)`, or a declared custom effect",
                                parameter.name
                            ));
                            return None;
                        }
                    },
                    CompileParamKind::Region => {
                        self.error("region arguments are erased before semantic analysis");
                        return None;
                    }
                    CompileParamKind::TypeConstructor { parameter_count } => {
                        let constructor = self.type_constructor_argument_from_expr(
                            &argument.value,
                            parameter_count,
                            owner,
                            &parameter.name,
                        )?;
                        Type::Named(constructor, Vec::new())
                    }
                    CompileParamKind::EffectConstructor { .. } => {
                        self.error(format!(
                            "constructor compile-time argument `{}` in `{owner}` is parsed but not supported by semantic analysis yet",
                            parameter.name
                        ));
                        return None;
                    }
                };
                let ty = if matches!(parameter.kind, CompileParamKind::TypeConstructor { .. }) {
                    let Type::Named(name, arguments) = &source else {
                        unreachable!("type constructor argument helper returns a named source")
                    };
                    debug_assert!(arguments.is_empty());
                    Ty::Struct(type_constructor_marker(name))
                } else {
                    let Some(ty) = self.probe_source_ty(&source) else {
                        self.error(format!(
                            "invalid explicit type argument for `{}` in `{owner}`",
                            parameter.name
                        ));
                        return None;
                    };
                    ty
                };
                inferred.insert(
                    parameter.name.clone(),
                    InferredTypeArgument {
                        ty,
                        source: Some(source),
                        origin: "explicit type argument".to_owned(),
                    },
                );
            }
            source_index += 1;
            compile_index = target + 1;
        }
        for parameter in compile_groups.iter().flatten() {
            if parameter.kind == CompileParamKind::Access {
                inferred
                    .entry(parameter.name.clone())
                    .or_insert_with(|| InferredTypeArgument {
                        ty: Ty::Struct(ACCESS_SHARED_MARKER.to_owned()),
                        source: Some(Type::Named(ACCESS_SHARED_MARKER.to_owned(), Vec::new())),
                        origin: "default shared access".to_owned(),
                    });
            } else if parameter.kind == CompileParamKind::Passing {
                inferred
                    .entry(parameter.name.clone())
                    .or_insert_with(|| InferredTypeArgument {
                        ty: Ty::Struct(PASSING_AUTO_MARKER.to_owned()),
                        source: Some(Type::Named(PASSING_AUTO_MARKER.to_owned(), Vec::new())),
                        origin: "default automatic passing".to_owned(),
                    });
            } else if parameter.kind == CompileParamKind::Effect {
                inferred
                    .entry(parameter.name.clone())
                    .or_insert_with(|| InferredTypeArgument {
                        ty: Ty::EffectRow {
                            unsafe_effect: false,
                            throws_error: None,
                            custom_effects: Vec::new(),
                        },
                        source: Some(effect_row_source(false, None, &[])),
                        origin: "default pure effect".to_owned(),
                    });
            }
        }
        Some((compile_parameters, inferred, source_index))
    }

    pub(super) fn probe_type_argument_inference_seed(
        &self,
        compile_groups: &[Vec<CompileParam>],
        groups: &[&[CallArg]],
        context: &LowerCtx,
        unit_is_type: bool,
    ) -> Option<(
        HashSet<String>,
        HashMap<String, InferredTypeArgument>,
        usize,
    )> {
        let compile_parameters: HashSet<_> = compile_groups
            .iter()
            .flatten()
            .filter(|parameter| {
                matches!(
                    parameter.kind,
                    CompileParamKind::Type | CompileParamKind::TypeConstructor { .. }
                )
            })
            .map(|parameter| parameter.name.clone())
            .collect();
        let mut inferred = HashMap::new();
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
                    unit_is_type,
                )
            {
                Some(compile_index)
            } else {
                None
            };
            let Some(target) = target else {
                break;
            };
            let parameters = &compile_groups[target];
            let sources = self.probe_compile_group_sources(
                parameters,
                arguments,
                &context.type_substitutions,
            )?;
            for (parameter, source) in parameters.iter().zip(sources) {
                let ty = self.probe_compile_argument_ty(parameter, &source)?;
                inferred.insert(
                    parameter.name.clone(),
                    InferredTypeArgument {
                        ty,
                        source: Some(source),
                        origin: "explicit type argument".to_owned(),
                    },
                );
            }
            source_index += 1;
            compile_index = target + 1;
        }
        for parameter in compile_groups.iter().flatten() {
            if parameter.kind == CompileParamKind::Access {
                inferred
                    .entry(parameter.name.clone())
                    .or_insert_with(|| InferredTypeArgument {
                        ty: Ty::Struct(ACCESS_SHARED_MARKER.to_owned()),
                        source: Some(Type::Named(ACCESS_SHARED_MARKER.to_owned(), Vec::new())),
                        origin: "default shared access".to_owned(),
                    });
            } else if parameter.kind == CompileParamKind::Passing {
                inferred
                    .entry(parameter.name.clone())
                    .or_insert_with(|| InferredTypeArgument {
                        ty: Ty::Struct(PASSING_AUTO_MARKER.to_owned()),
                        source: Some(Type::Named(PASSING_AUTO_MARKER.to_owned(), Vec::new())),
                        origin: "default automatic passing".to_owned(),
                    });
            } else if parameter.kind == CompileParamKind::Effect {
                inferred
                    .entry(parameter.name.clone())
                    .or_insert_with(|| InferredTypeArgument {
                        ty: Ty::EffectRow {
                            unsafe_effect: false,
                            throws_error: None,
                            custom_effects: Vec::new(),
                        },
                        source: Some(effect_row_source(false, None, &[])),
                        origin: "default pure effect".to_owned(),
                    });
            }
        }
        Some((compile_parameters, inferred, source_index))
    }

    pub(super) fn finish_type_argument_inference(
        &mut self,
        owner: &str,
        ordered_parameters: &[CompileParam],
        inferred: &HashMap<String, InferredTypeArgument>,
        unsupported_argument: bool,
    ) -> Option<(Vec<Type>, Vec<Ty>)> {
        let unresolved: Vec<_> = ordered_parameters
            .iter()
            .filter(|parameter| !inferred.contains_key(&parameter.name))
            .map(|parameter| parameter.name.clone())
            .collect();
        if !unresolved.is_empty() {
            if unsupported_argument {
                self.error(format!(
                    "cannot infer type argument{} {} for `{owner}` from this argument expression; write explicit type arguments",
                    if unresolved.len() == 1 { "" } else { "s" },
                    unresolved
                        .iter()
                        .map(|name| format!("`{name}`"))
                        .collect::<Vec<_>>()
                        .join(", ")
                ));
            } else {
                self.error(format!(
                    "cannot infer type argument{} {} for `{owner}`; write explicit type arguments",
                    if unresolved.len() == 1 { "" } else { "s" },
                    unresolved
                        .iter()
                        .map(|name| format!("`{name}`"))
                        .collect::<Vec<_>>()
                        .join(", ")
                ));
            }
            return None;
        }
        let mut source_arguments = Vec::new();
        let mut arguments = Vec::new();
        for parameter in ordered_parameters {
            let inferred = &inferred[&parameter.name];
            let Some(source) = inferred
                .source
                .clone()
                .or_else(|| self.source_type_for_ty(&inferred.ty))
            else {
                self.error(format!(
                    "cannot use inferred type `{}` for type parameter `{}` in `{owner}`",
                    inferred.ty, parameter.name
                ));
                return None;
            };
            source_arguments.push(source);
            arguments.push(inferred.ty.clone());
        }
        Some((source_arguments, arguments))
    }

    pub(super) fn infer_from_expression_constraints(
        &mut self,
        constraints: &[(Type, Expr, String)],
        compile_parameters: &HashSet<String>,
        inferred: &mut HashMap<String, InferredTypeArgument>,
        context: &LowerCtx,
    ) -> Option<bool> {
        let mut pending: Vec<_> = (0..constraints.len()).collect();
        let unsupported = loop {
            let mut progress = false;
            let mut next = Vec::new();
            let mut defaultable = Vec::new();
            for index in pending {
                let (template, expression, origin) = &constraints[index];
                let hint = self.resolved_template_ty(template, compile_parameters, inferred);
                match self.probe_expr_ty(expression, hint.as_ref(), context) {
                    TypeProbe::Known(actual) => {
                        match self.unify_template_ty(
                            template,
                            &actual,
                            None,
                            compile_parameters,
                            inferred,
                            origin,
                        ) {
                            Ok(changed) => progress |= changed,
                            Err(message) => {
                                self.error(message);
                                return None;
                            }
                        }
                    }
                    TypeProbe::KnownSource(actual, source) => {
                        match self.unify_template_ty(
                            template,
                            &actual,
                            Some(&source),
                            compile_parameters,
                            inferred,
                            origin,
                        ) {
                            Ok(changed) => progress |= changed,
                            Err(message) => {
                                self.error(message);
                                return None;
                            }
                        }
                    }
                    TypeProbe::Defaultable(actual) => defaultable.push((index, actual)),
                    TypeProbe::Unsupported => next.push(index),
                }
            }
            if progress {
                next.extend(defaultable.into_iter().map(|(index, _)| index));
                pending = next;
                continue;
            }
            let mut default_progress = false;
            for (index, actual) in defaultable {
                let (template, _, origin) = &constraints[index];
                match self.unify_template_ty(
                    template,
                    &actual,
                    None,
                    compile_parameters,
                    inferred,
                    origin,
                ) {
                    Ok(changed) => default_progress |= changed,
                    Err(message) => {
                        self.error(message);
                        return None;
                    }
                }
            }
            if next.is_empty() {
                break false;
            }
            if !default_progress {
                break true;
            }
            pending = next;
        };
        Some(unsupported)
    }
}

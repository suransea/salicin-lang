use std::collections::{HashMap, HashSet};

use crate::ast::{
    CallArg, CompileParam, CompileParamDefault, Expr, Sort, Type, USizeConst, WherePredicate,
};
use crate::core::LangItemKind;
use crate::static_semantics::StaticValue;

use super::compile_time::{
    access_mutability, closed_value_from_marker, closed_value_marker, closed_value_member,
    describe_compile_sort, effect_row_from_marker, effect_row_source, is_access_sort_name,
    source_effect_identity, source_from_static_value, static_value_from_source,
    type_constructor_marker, usize_value_marker, ACCESS_MUT_MARKER, ACCESS_SHARED_MARKER,
};
use super::effects::source_type_is_never;
use super::flow::LowerCtx;
use super::hir::{FunctionTy, Ty};
use super::lower::{InferredTypeArgument, TypeProbe};
use super::names::nominal_instance_name;
use super::registry::{NominalInstanceKey, NominalKind};
use super::Analyzer;

impl Analyzer {
    pub(super) fn resolve_inferred_generic_function_instance(
        &mut self,
        name: &str,
        groups: &[&[CallArg]],
        expected: Option<&Ty>,
        context: &LowerCtx,
    ) -> Option<(String, usize)> {
        let template = self
            .function_templates
            .get(name)
            .unwrap_or_else(|| panic!("missing generic function template `{name}`"))
            .clone();
        let (compile_parameters, mut inferred, runtime_start) = self.seed_type_argument_inference(
            name,
            &template.compile_groups,
            groups,
            context,
            false,
        )?;
        let runtime_groups = &groups[runtime_start..];
        if runtime_groups.len() > template.groups.len() {
            self.error(format!(
                "too many parameter groups in call to `{name}`: expected at most {}, found {}",
                template.groups.len(),
                runtime_groups.len()
            ));
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
            ordered_runtime_groups.push(self.ordered_call_arguments(
                name,
                group_index + 1,
                arguments,
                &parameter_names,
            )?);
        }

        if runtime_groups.len() == template.groups.len() {
            if let (Some(expected), Some(result)) = (expected, template.return_type.as_ref()) {
                if *expected != Ty::Error {
                    let logical_result = if template.effects.throws.is_some() {
                        match result {
                            Type::Named(_, arguments) if arguments.len() == 2 => &arguments[0],
                            _ => result,
                        }
                    } else {
                        result
                    };
                    if !source_type_is_never(logical_result) {
                        if let Err(message) = self.unify_template_ty(
                            logical_result,
                            expected,
                            None,
                            &compile_parameters,
                            &mut inferred,
                            "expected result type",
                        ) {
                            self.error(message);
                            return None;
                        }
                    }
                }
            }
        }

        let constraints: Vec<_> = ordered_runtime_groups
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
            .collect();
        let unsupported_argument = self.infer_from_expression_constraints(
            &constraints,
            &compile_parameters,
            &mut inferred,
            context,
        )?;
        self.infer_from_concrete_trait_predicates(
            &template.where_predicates,
            &compile_parameters,
            &mut inferred,
        )?;
        let ordered_parameters = template
            .compile_groups
            .iter()
            .flatten()
            .cloned()
            .collect::<Vec<_>>();
        let (source_arguments, arguments) = self.finish_type_argument_inference(
            name,
            &ordered_parameters,
            &inferred,
            unsupported_argument,
        )?;
        let canonical = self.ensure_function_instance(name, source_arguments, arguments)?;
        Some((canonical, runtime_start))
    }

    pub(super) fn infer_from_concrete_trait_predicates(
        &mut self,
        predicates: &[WherePredicate],
        compile_parameters: &HashSet<String>,
        inferred: &mut HashMap<String, InferredTypeArgument>,
    ) -> Option<()> {
        let default_effect_parameters = inferred
            .iter()
            .filter_map(|(name, argument)| {
                (argument.origin == "default pure effect").then_some(name.clone())
            })
            .collect::<HashSet<_>>();
        if default_effect_parameters.is_empty() {
            return Some(());
        }
        let mut predicate_parameters = compile_parameters.clone();
        predicate_parameters.extend(default_effect_parameters.iter().cloned());
        for predicate in predicates {
            let Type::Named(trait_name, trait_arguments) = &predicate.trait_ref else {
                continue;
            };
            let Some(schema) = self.traits.get(trait_name) else {
                continue;
            };
            if !trait_arguments.iter().zip(&schema.compile_parameters).any(
                |(argument, parameter)| {
                    parameter.kind == Sort::Effects
                        && matches!(argument, Type::Named(name, arguments)
                            if arguments.is_empty() && default_effect_parameters.contains(name))
                },
            ) {
                continue;
            }
            let Some(subject) =
                self.resolved_template_ty(&predicate.subject, compile_parameters, inferred)
            else {
                continue;
            };
            let candidates = self
                .trait_impls
                .values()
                .filter(|implementation| {
                    implementation.key.self_ty == subject
                        && implementation.key.trait_ref.name == *trait_name
                        && implementation.key.trait_ref.arguments.len() == trait_arguments.len()
                })
                .filter(|implementation| {
                    trait_arguments
                        .iter()
                        .zip(&implementation.key.trait_ref.arguments)
                        .all(|(template, actual)| {
                            let Type::Named(parameter, arguments) = template else {
                                return self
                                    .resolved_template_ty(template, &predicate_parameters, inferred)
                                    .is_none_or(|resolved| resolved == *actual);
                            };
                            if !arguments.is_empty() || !predicate_parameters.contains(parameter) {
                                return self
                                    .resolved_template_ty(template, &predicate_parameters, inferred)
                                    .is_none_or(|resolved| resolved == *actual);
                            }
                            inferred.get(parameter).is_none_or(|selected| {
                                selected.origin.starts_with("default ") || selected.ty == *actual
                            })
                        })
                })
                .cloned()
                .collect::<Vec<_>>();
            let [implementation] = candidates.as_slice() else {
                continue;
            };
            let mut constraints = trait_arguments
                .iter()
                .cloned()
                .zip(implementation.key.trait_ref.arguments.iter().cloned())
                .map(|(template, actual)| {
                    (
                        template,
                        actual,
                        None,
                        format!("where predicate `{trait_name}` argument"),
                    )
                })
                .collect::<Vec<_>>();
            for binding in &predicate.associated_types {
                let Some(actual) = implementation.associated_types.get(&binding.name) else {
                    continue;
                };
                constraints.push((
                    binding.ty.clone(),
                    actual.clone(),
                    implementation
                        .associated_type_sources
                        .get(&binding.name)
                        .cloned(),
                    format!(
                        "associated type `{trait_name}.{}` in where predicate",
                        binding.name
                    ),
                ));
            }
            let mut candidate = inferred.clone();
            for (template, actual, actual_source, origin) in constraints {
                if let Type::Named(parameter, arguments) = &template {
                    if arguments.is_empty()
                        && predicate_parameters.contains(parameter)
                        && candidate
                            .get(parameter)
                            .is_some_and(|selected| selected.origin.starts_with("default "))
                    {
                        candidate.remove(parameter);
                    }
                }
                if let Err(message) = self.unify_template_ty(
                    &template,
                    &actual,
                    actual_source.as_ref(),
                    &predicate_parameters,
                    &mut candidate,
                    &origin,
                ) {
                    self.error(message);
                    return None;
                }
            }
            *inferred = candidate;
        }
        Some(())
    }

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
            Type::I8 => (*actual == Ty::I8).then_some(false).ok_or_else(mismatch),
            Type::I16 => (*actual == Ty::I16).then_some(false).ok_or_else(mismatch),
            Type::I32 => (*actual == Ty::I32).then_some(false).ok_or_else(mismatch),
            Type::I64 => (*actual == Ty::I64).then_some(false).ok_or_else(mismatch),
            Type::I128 => (*actual == Ty::I128).then_some(false).ok_or_else(mismatch),
            Type::ISize => (*actual == Ty::ISize).then_some(false).ok_or_else(mismatch),
            Type::U8 => (*actual == Ty::U8).then_some(false).ok_or_else(mismatch),
            Type::U16 => (*actual == Ty::U16).then_some(false).ok_or_else(mismatch),
            Type::U32 => (*actual == Ty::U32).then_some(false).ok_or_else(mismatch),
            Type::U64 => (*actual == Ty::U64).then_some(false).ok_or_else(mismatch),
            Type::U128 => (*actual == Ty::U128).then_some(false).ok_or_else(mismatch),
            Type::USize => (*actual == Ty::USize).then_some(false).ok_or_else(mismatch),
            Type::Bool => (*actual == Ty::Bool).then_some(false).ok_or_else(mismatch),
            Type::Unit => (*actual == Ty::Unit).then_some(false).ok_or_else(mismatch),
            Type::Tuple(fields) => {
                let Ty::Tuple(actual_fields) = actual else {
                    return Err(mismatch());
                };
                if fields.len() != actual_fields.len() {
                    return Err(mismatch());
                }
                let source_fields = match actual_source {
                    Some(Type::Tuple(fields)) if fields.len() == actual_fields.len() => {
                        Some(fields.as_slice())
                    }
                    _ => None,
                };
                let mut changed = false;
                for (index, (field, actual_field)) in
                    fields.iter().zip(actual_fields).enumerate()
                {
                    changed |= self.unify_template_ty(
                        field,
                        actual_field,
                        source_fields.and_then(|fields| fields.get(index)),
                        compile_parameters,
                        inferred,
                        origin,
                    )?;
                }
                Ok(changed)
            }
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
                                && previous.ty == Ty::Struct(ACCESS_SHARED_MARKER.to_owned())
                                && selected.ty == Ty::Struct(ACCESS_MUT_MARKER.to_owned()) =>
                        {
                            // A mutable reference can be reborrowed as shared when an
                            // explicit access argument has already selected `shared`.
                        }
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
                if region.is_some() && region != actual_region {
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
            Type::ArrayApplication {
                constructor,
                element,
                length,
            } => {
                if !self.is_lang_item_name(constructor, LangItemKind::ArrayTypeForm) {
                    return Err(mismatch());
                }
                let Ty::Array(actual_element, actual_length) = actual else {
                    return Err(mismatch());
                };
                let mut changed = false;
                match length {
                    USizeConst::Literal(length) if length != actual_length => {
                        return Err(mismatch());
                    }
                    USizeConst::Literal(_) => {}
                    USizeConst::Parameter(name) => {
                        let selected = InferredTypeArgument {
                            ty: Ty::Struct(usize_value_marker(*actual_length)),
                            source: Some(Type::CompileUSize(*actual_length)),
                            origin: origin.to_owned(),
                        };
                        match inferred.get(name) {
                            Some(previous) if previous.ty != selected.ty => {
                                return Err(format!(
                                    "conflicting inference for `usize` parameter `{name}`: `{}` from {} conflicts with `{actual_length}` from {origin}",
                                    match &previous.source {
                                        Some(Type::CompileUSize(value)) => value.to_string(),
                                        _ => previous.ty.to_string(),
                                    },
                                    previous.origin,
                                ));
                            }
                            Some(_) => {}
                            None => {
                                inferred.insert(name.clone(), selected);
                                changed = true;
                            }
                        }
                    }
                    USizeConst::Expression(_) => {
                        // Static expressions are normalized after their free
                        // `usize` parameters have been inferred or supplied.
                    }
                }
                self.unify_template_ty(
                    element,
                    actual_element,
                    match actual_source {
                        Some(Type::ArrayApplication { element, .. })
                        | Some(Type::Array(element, _)) => Some(element),
                        _ => None,
                    },
                    compile_parameters,
                    inferred,
                    origin,
                )
                .map(|element_changed| changed || element_changed)
            }
            Type::CompileUSize(_) => Err(mismatch()),
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
                if self.is_lang_item_name(name, LangItemKind::PtrTypeForm)
                    && matches!(arguments.len(), 1 | 2) =>
            {
                let Ty::Pointer { pointee, mutable } = actual else {
                    return Err(mismatch());
                };
                let actual_access = Ty::Struct(
                    if *mutable {
                        ACCESS_MUT_MARKER
                    } else {
                        ACCESS_SHARED_MARKER
                    }
                    .to_owned(),
                );
                let actual_source_arguments = match actual_source {
                    Some(Type::Named(actual_name, actual_arguments))
                        if actual_name == name
                            && matches!(actual_arguments.len(), 1 | 2) =>
                    {
                        Some(actual_arguments.as_slice())
                    }
                    _ => None,
                };
                let (access, template_pointee) = match arguments.as_slice() {
                    [pointee] => (None, pointee),
                    [access, pointee] => (Some(access), pointee),
                    _ => unreachable!("pointer arity guard matched"),
                };
                let mut changed = match access {
                    None if *mutable => return Err(mismatch()),
                    None => false,
                    Some(access) => match access {
                    Type::Named(access, access_arguments)
                        if access_arguments.is_empty() && access_mutability(access).is_some() =>
                    {
                        let expects_mutable =
                            access_mutability(access).expect("access member was checked");
                        if expects_mutable != *mutable {
                            return Err(mismatch());
                        }
                        false
                    }
                    access => self.unify_template_ty(
                        access,
                        &actual_access,
                        actual_source_arguments.and_then(|arguments| {
                            (arguments.len() == 2).then(|| &arguments[0])
                        }),
                        compile_parameters,
                        inferred,
                        origin,
                    )?,
                    },
                };
                changed |= self.unify_template_ty(
                    template_pointee,
                    pointee,
                    actual_source_arguments.map(|arguments| {
                        &arguments[if arguments.len() == 2 { 1 } else { 0 }]
                    }),
                    compile_parameters,
                    inferred,
                    origin,
                )?;
                Ok(changed)
            }
            Type::Named(name, arguments)
                if self.is_lang_item_name(name, LangItemKind::SliceTypeForm)
                    && arguments.len() == 1 =>
            {
                let Ty::Slice(actual_element) = actual else {
                    return Err(mismatch());
                };
                let actual_source_element = match actual_source {
                    Some(Type::Named(actual_name, actual_arguments))
                        if actual_name == name && actual_arguments.len() == 1 =>
                    {
                        actual_arguments.first()
                    }
                    _ => None,
                };
                self.unify_template_ty(
                    &arguments[0],
                    actual_element,
                    actual_source_element,
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
                    .map(|field| self.resolved_template_ty(field, compile_parameters, inferred))
                    .collect::<Option<Vec<_>>>()?,
            )),
            Type::Borrow {
                mutable,
                access,
                region,
                pointee,
            } => Some(Ty::Reference {
                pointee: Box::new(self.resolved_template_ty(
                    pointee,
                    compile_parameters,
                    inferred,
                )?),
                mutable: access
                    .as_deref()
                    .and_then(|name| inferred.get(name))
                    .map_or(*mutable, |argument| {
                        argument.ty == Ty::Struct(ACCESS_MUT_MARKER.to_owned())
                    }),
                region: region.clone(),
            }),
            Type::Array(element, length) => Some(Ty::Array(
                Box::new(self.resolved_template_ty(element, compile_parameters, inferred)?),
                *length,
            )),
            Type::ArrayApplication {
                constructor,
                element,
                length,
            } => {
                if !self.is_lang_item_name(constructor, LangItemKind::ArrayTypeForm) {
                    return None;
                }
                let length = match length {
                    USizeConst::Literal(value) => *value,
                    USizeConst::Parameter(name) => {
                        let Type::CompileUSize(value) = inferred.get(name)?.source.as_ref()? else {
                            return None;
                        };
                        *value
                    }
                    USizeConst::Expression(_) => return None,
                };
                Some(Ty::Array(
                    Box::new(self.resolved_template_ty(element, compile_parameters, inferred)?),
                    length,
                ))
            }
            Type::CompileUSize(_) => None,
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
                if self.is_lang_item_name(name, LangItemKind::PtrTypeForm) =>
            {
                let (access, pointee) = match arguments.as_slice() {
                    [pointee] => (None, pointee),
                    [access, pointee] => (Some(access), pointee),
                    _ => return None,
                };
                let mutable = match access {
                    None => false,
                    Some(access) => {
                        match self.resolved_template_ty(access, compile_parameters, inferred)? {
                            Ty::Struct(name) => access_mutability(&name)?,
                            _ => return None,
                        }
                    }
                };
                Some(Ty::Pointer {
                    pointee: Box::new(self.resolved_template_ty(
                        pointee,
                        compile_parameters,
                        inferred,
                    )?),
                    mutable,
                })
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
                    &parameter.kind,
                    Sort::Type | Sort::USize | Sort::Named(_) | Sort::TypeConstructor { .. }
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
                let schema = parameters
                    .iter()
                    .map(|parameter| {
                        format!(
                            "`{}` of sort {}",
                            parameter.name,
                            describe_compile_sort(parameter.kind.clone())
                        )
                    })
                    .collect::<Vec<_>>()
                    .join(", ");
                self.error(format!(
                    "compile-time argument count mismatch in group {} of `{owner}`: expected {} ({schema}), found {}",
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
                let source = match parameter.kind.clone() {
                    Sort::Type => {
                        self.type_argument_from_expr(&argument.value, &context.type_substitutions)?
                    }
                    Sort::USize => match &argument.value {
                        Expr::Integer(value) => {
                            let Ok(value) = u64::try_from(*value) else {
                                self.error(format!(
                                    "invalid `usize` argument for `{}` in `{owner}`; expected a non-negative integer fitting in `u64`",
                                    parameter.name
                                ));
                                return None;
                            };
                            source_from_static_value(&StaticValue::USize(value))
                                .expect("usize static values have a monomorphization source")
                        }
                        Expr::Name(name) => {
                            let Some(Type::CompileUSize(value)) =
                                context.type_substitutions.get(name)
                            else {
                                self.error(format!(
                                    "invalid `usize` argument for `{}` in `{owner}`",
                                    parameter.name
                                ));
                                return None;
                            };
                            source_from_static_value(&StaticValue::USize(*value))
                                .expect("usize static values have a monomorphization source")
                        }
                        _ => {
                            self.error(format!(
                                "invalid `usize` argument for `{}` in `{owner}`; expected a non-negative integer",
                                parameter.name
                            ));
                            return None;
                        }
                    },
                    Sort::Effect | Sort::Effects => {
                        let source = match &argument.value {
                            Expr::Name(name) if name == "pure" => {
                                effect_row_source(false, None, &[])
                            }
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
                                    effect_row_source(
                                        false,
                                        None,
                                        &[source_effect_identity(&effect)],
                                    )
                                }
                            }
                            Expr::Call(callee, arguments)
                                if matches!(callee.as_ref(), Expr::Name(name) if effect_row_from_marker(name).is_some())
                                    && arguments.len() <= 1
                                    && arguments
                                        .iter()
                                        .all(|argument| argument.label.is_none()) =>
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
                                "compile-time argument `{}` in `{owner}` expects sort {}; write `pure`, `Unsafe`, `Throws(Error)`, or a declared custom effect",
                                parameter.name,
                                describe_compile_sort(parameter.kind.clone()),
                            ));
                                return None;
                            }
                        };
                        if parameter.kind == Sort::Effect
                            && static_value_from_source(&source, &Sort::Effect).is_none()
                        {
                            self.error(format!(
                                "compile-time argument `{}` in `{owner}` expects one `effect` identity, not an `effects` row",
                                parameter.name
                            ));
                            return None;
                        }
                        source
                    }
                    Sort::Region => {
                        self.error("region arguments are erased before semantic analysis");
                        return None;
                    }
                    Sort::String => {
                        self.error(
                            "`String` arguments are currently restricted to compiler-owned syntax",
                        );
                        return None;
                    }
                    Sort::Parameters => {
                        self.error(format!(
                            "explicit parameter-schema argument `{}` in `{owner}` is not supported yet; parameter schemas are currently supplied by compiler-derived associated declarations",
                            parameter.name
                        ));
                        return None;
                    }
                    Sort::ParameterPack => {
                        self.error(format!(
                            "explicit parameter-group pack argument `{}` in `{owner}` is not supported; it is inferred from the trailing case groups",
                            parameter.name
                        ));
                        return None;
                    }
                    Sort::ParameterModifier => {
                        let Some(source) = self.probe_parameter_modifier_source(
                            &argument.value,
                            &context.type_substitutions,
                        ) else {
                            self.error(format!(
                                "invalid parameter modifier argument for `{}` in `{owner}`; expected `copy`, `move`, or a declared modifier parameter",
                                parameter.name
                            ));
                            return None;
                        };
                        source
                    }
                    Sort::TypeConstructor { parameter_groups } => {
                        let constructor = self.type_constructor_argument_from_expr(
                            &argument.value,
                            &parameter_groups,
                            owner,
                            &parameter.name,
                        )?;
                        Type::Named(constructor, Vec::new())
                    }
                    Sort::EffectConstructor { .. } => {
                        self.error(format!(
                            "constructor compile-time argument `{}` in `{owner}` is parsed but not supported by semantic analysis yet",
                            parameter.name
                        ));
                        return None;
                    }
                    Sort::Named(ref compile_type) => {
                        let member = match &argument.value {
                            Expr::Bool(value) => if *value { "true" } else { "false" }.to_owned(),
                            Expr::Name(name) => {
                                if let Some(Type::Named(marker, arguments)) =
                                    context.type_substitutions.get(name)
                                {
                                    if arguments.is_empty()
                                        && closed_value_from_marker(marker)
                                            .is_some_and(|(owner, _)| owner == compile_type)
                                    {
                                        marker.clone()
                                    } else {
                                        name.clone()
                                    }
                                } else {
                                    name.clone()
                                }
                            }
                            Expr::Member(owner, member)
                                if matches!(
                                    owner.unlocated(),
                                    Expr::Name(owner)
                                        if owner == compile_type
                                            || (compile_type == "access"
                                                && is_access_sort_name(owner))
                                ) =>
                            {
                                member.clone()
                            }
                            _ => {
                                self.error(format!(
                                    "invalid `{compile_type}` argument for `{}` in `{owner}`; expected a closed value member",
                                    parameter.name
                                ));
                                return None;
                            }
                        };
                        if closed_value_from_marker(&member).is_some() {
                            let source = Type::Named(member, Vec::new());
                            static_value_from_source(&source, &parameter.kind)
                                .and_then(|value| source_from_static_value(&value))
                                .expect("validated finite static value must round-trip")
                        } else {
                            let normalized = if compile_type == "access" {
                                access_mutability(&member).map(|mutable| {
                                    if mutable { "mut" } else { "shared" }.to_owned()
                                })
                            } else {
                                self.closed_type_values
                                    .get(compile_type)
                                    .and_then(|members| {
                                        closed_value_member(compile_type, &member, members)
                                    })
                                    .map(str::to_owned)
                            };
                            let Some(member) = normalized else {
                                let description = if compile_type == "access" {
                                    "invalid access argument".to_owned()
                                } else {
                                    format!("invalid `{compile_type}` argument")
                                };
                                self.error(format!(
                                    "{description} `{member}` for `{}` in `{owner}`",
                                    parameter.name
                                ));
                                return None;
                            };
                            source_from_static_value(&StaticValue::Finite {
                                sort: compile_type.clone(),
                                member,
                            })
                            .expect("finite static values have a monomorphization source")
                        }
                    }
                };
                let ty = if matches!(parameter.kind, Sort::TypeConstructor { .. }) {
                    let Type::Named(name, arguments) = &source else {
                        unreachable!("type constructor argument helper returns a named source")
                    };
                    debug_assert!(arguments.is_empty());
                    Ty::Struct(type_constructor_marker(name))
                } else if let Type::CompileUSize(value) = &source {
                    Ty::Struct(usize_value_marker(*value))
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
            if parameter.kind.is_access() {
                inferred
                    .entry(parameter.name.clone())
                    .or_insert_with(|| InferredTypeArgument {
                        ty: Ty::Struct(ACCESS_SHARED_MARKER.to_owned()),
                        source: Some(Type::Named(ACCESS_SHARED_MARKER.to_owned(), Vec::new())),
                        origin: "default shared access".to_owned(),
                    });
            } else if parameter.kind == Sort::Effects {
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
            } else if let (Sort::Named(compile_type), Some(CompileParamDefault::Name(member))) =
                (&parameter.kind, &parameter.default)
            {
                if self
                    .closed_type_values
                    .get(compile_type)
                    .is_some_and(|members| members.contains(member))
                {
                    let marker = closed_value_marker(compile_type, member);
                    inferred.entry(parameter.name.clone()).or_insert_with(|| {
                        InferredTypeArgument {
                            ty: Ty::Struct(marker.clone()),
                            source: Some(Type::Named(marker, Vec::new())),
                            origin: format!("default `{member}` value"),
                        }
                    });
                }
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
                    &parameter.kind,
                    Sort::Type | Sort::USize | Sort::Named(_) | Sort::TypeConstructor { .. }
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
            if parameter.kind.is_access() {
                inferred
                    .entry(parameter.name.clone())
                    .or_insert_with(|| InferredTypeArgument {
                        ty: Ty::Struct(ACCESS_SHARED_MARKER.to_owned()),
                        source: Some(Type::Named(ACCESS_SHARED_MARKER.to_owned(), Vec::new())),
                        origin: "default shared access".to_owned(),
                    });
            } else if parameter.kind == Sort::Effects {
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
            } else if let (Sort::Named(compile_type), Some(CompileParamDefault::Name(member))) =
                (&parameter.kind, &parameter.default)
            {
                if self
                    .closed_type_values
                    .get(compile_type)
                    .is_some_and(|members| members.contains(member))
                {
                    let marker = closed_value_marker(compile_type, member);
                    inferred.entry(parameter.name.clone()).or_insert_with(|| {
                        InferredTypeArgument {
                            ty: Ty::Struct(marker.clone()),
                            source: Some(Type::Named(marker, Vec::new())),
                            origin: format!("default `{member}` value"),
                        }
                    });
                }
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
            .cloned()
            .collect();
        if !unresolved.is_empty() {
            let unresolved_count = unresolved.len();
            let unresolved = unresolved
                .iter()
                .map(|parameter| {
                    format!(
                        "`{}` of sort {}",
                        parameter.name,
                        describe_compile_sort(parameter.kind.clone())
                    )
                })
                .collect::<Vec<_>>()
                .join(", ");
            if unsupported_argument {
                self.error(format!(
                    "cannot infer compile-time argument{} {unresolved} for `{owner}` from this argument expression; write the required compile-time argument group explicitly",
                    if unresolved_count == 1 { "" } else { "s" },
                ));
            } else {
                self.error(format!(
                    "cannot infer compile-time argument{} {unresolved} for `{owner}`; write the required compile-time argument group explicitly",
                    if unresolved_count == 1 { "" } else { "s" },
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
                let probe = self
                    .probe_borrow_template_argument_ty(template, expression, inferred, context)
                    .unwrap_or_else(|| {
                        hint.as_ref()
                            .map(|hint| self.probe_expr_ty(expression, Some(hint), context))
                            .unwrap_or_else(|| self.probe_expr_ty(expression, None, context))
                    });
                match probe {
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

    fn probe_borrow_template_argument_ty(
        &self,
        template: &Type,
        expression: &Expr,
        inferred: &HashMap<String, InferredTypeArgument>,
        context: &LowerCtx,
    ) -> Option<TypeProbe> {
        let Type::Borrow {
            mutable,
            access,
            region,
            ..
        } = template
        else {
            return None;
        };
        let (pointee, pointee_source) = match self.probe_expr_ty(expression, None, context) {
            TypeProbe::Known(ty @ Ty::Reference { .. }) => {
                let source = self.source_type_for_ty(&ty);
                return Some(match source {
                    Some(source) => TypeProbe::KnownSource(ty, source),
                    None => TypeProbe::Known(ty),
                });
            }
            TypeProbe::Known(ty) => {
                let source = self.source_type_for_ty(&ty);
                (ty, source)
            }
            TypeProbe::KnownSource(ty @ Ty::Reference { .. }, source) => {
                return Some(TypeProbe::KnownSource(ty, source));
            }
            TypeProbe::KnownSource(ty, source) => (ty, Some(source)),
            TypeProbe::Defaultable(_) | TypeProbe::Unsupported => return None,
        };
        let actual_mutable = access
            .as_ref()
            .and_then(|access| inferred.get(access))
            .is_some_and(|selected| selected.ty == Ty::Struct(ACCESS_MUT_MARKER.to_owned()))
            || (access.is_none() && *mutable);
        let source = pointee_source.map(|pointee| Type::Borrow {
            mutable: actual_mutable,
            access: None,
            region: region.clone(),
            pointee: Box::new(pointee),
        });
        let actual = Ty::Reference {
            pointee: Box::new(pointee),
            mutable: actual_mutable,
            region: region.clone(),
        };
        Some(match source {
            Some(source) => TypeProbe::KnownSource(actual, source),
            None => TypeProbe::Known(actual),
        })
    }
}

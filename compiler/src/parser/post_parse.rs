use std::collections::{HashMap, HashSet};

use crate::ast::{
    AssociatedTypeBinding, Binding, CompileParam, Expr, ExtendMember, Function, FunctionEffects,
    Item, Sort, StaticFragmentKind, Stmt, TraitMember, Type, USizeConst, VariantFields,
};

pub(crate) fn infer_extend_parameters(items: &mut [Item]) -> Result<(), String> {
    let mut constructors = HashMap::<String, Vec<CompileParam>>::new();
    let mut concrete_types = HashSet::<String>::new();
    let mut closed_values = HashMap::<String, HashSet<String>>::new();
    for item in items.iter() {
        let (name, groups) = match item {
            Item::Struct(definition) => (&definition.name, Some(&definition.compile_groups)),
            Item::Enum(definition) => (&definition.name, Some(&definition.compile_groups)),
            Item::TypeForm(definition) => (&definition.name, Some(&definition.compile_groups)),
            Item::TypeAlias(definition) => (&definition.name, Some(&definition.compile_groups)),
            Item::Trait(definition) => {
                concrete_types.insert(definition.name.clone());
                continue;
            }
            Item::Sort(definition) => {
                if let Some(members) = &definition.members {
                    closed_values
                        .insert(definition.name.clone(), members.iter().cloned().collect());
                }
                continue;
            }
            _ => continue,
        };
        concrete_types.insert(name.clone());
        if let Some(groups) = groups {
            constructors.insert(name.clone(), groups.iter().flatten().cloned().collect());
        }
    }

    let schema = ExtendPatternSchema {
        constructors: &constructors,
        concrete_types: &concrete_types,
        closed_values: &closed_values,
    };
    for item in items {
        let Item::Extend(extension) = item else {
            continue;
        };
        let mut inferred = Vec::<CompileParam>::new();
        let mut positions = HashMap::<String, usize>::new();
        infer_extend_pattern(
            &extension.target,
            Sort::Type,
            true,
            &schema,
            &mut inferred,
            &mut positions,
        )?;
        extension.compile_groups = if inferred.is_empty() {
            Vec::new()
        } else {
            vec![inferred]
        };
    }
    Ok(())
}

struct ExtendPatternSchema<'a> {
    constructors: &'a HashMap<String, Vec<CompileParam>>,
    concrete_types: &'a HashSet<String>,
    closed_values: &'a HashMap<String, HashSet<String>>,
}

fn infer_extend_pattern(
    pattern: &Type,
    expected: Sort,
    root: bool,
    schema: &ExtendPatternSchema<'_>,
    inferred: &mut Vec<CompileParam>,
    positions: &mut HashMap<String, usize>,
) -> Result<(), String> {
    let bind = |name: &str,
                kind: Sort,
                inferred: &mut Vec<CompileParam>,
                positions: &mut HashMap<String, usize>|
     -> Result<(), String> {
        if let Some(index) = positions.get(name).copied() {
            if inferred[index].kind != kind {
                return Err(format!(
                    "extend pattern `{name}` is inferred as both `{:?}` and `{:?}`",
                    inferred[index].kind, kind
                ));
            }
            return Ok(());
        }
        positions.insert(name.to_owned(), inferred.len());
        inferred.push(CompileParam {
            name: name.to_owned(),
            kind,
            default: None,
        });
        Ok(())
    };

    match pattern {
        Type::Named(name, arguments) => {
            if arguments.is_empty() {
                let is_concrete = schema.concrete_types.contains(name)
                    || (root && (name.contains("::") || name.contains('.')))
                    || (expected.is_access()
                        && (matches!(
                            name.as_str(),
                            "mut" | "shared" | "$access$mut" | "$access$shared"
                        ) || name.ends_with("::mut")
                            || name.ends_with("::shared")
                            || name.ends_with(".mut")
                            || name.ends_with(".shared")))
                    || match &expected {
                        Sort::Named(sort) => schema
                            .closed_values
                            .get(sort)
                            .is_some_and(|members| members.contains(name)),
                        _ => false,
                    }
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
                    );
                if !is_concrete {
                    bind(name, expected, inferred, positions)?;
                }
                return Ok(());
            }
            let parameters = schema.constructors.get(name);
            for (index, argument) in arguments.iter().enumerate() {
                let kind = parameters
                    .and_then(|parameters| parameters.get(index))
                    .map_or(Sort::Type, |parameter| parameter.kind.clone());
                infer_extend_pattern(argument, kind, false, schema, inferred, positions)?;
            }
        }
        Type::NamedArgs(name, arguments) => {
            let parameters = schema.constructors.get(name);
            for (index, argument) in arguments.iter().enumerate() {
                let parameter = argument
                    .label
                    .as_ref()
                    .and_then(|label| {
                        parameters.and_then(|parameters| {
                            parameters.iter().find(|parameter| &parameter.name == label)
                        })
                    })
                    .or_else(|| parameters.and_then(|parameters| parameters.get(index)));
                infer_extend_pattern(
                    &argument.ty,
                    parameter.map_or(Sort::Type, |parameter| parameter.kind.clone()),
                    false,
                    schema,
                    inferred,
                    positions,
                )?;
            }
        }
        Type::ArrayApplication {
            element, length, ..
        } => {
            infer_extend_pattern(element, Sort::Type, false, schema, inferred, positions)?;
            if let USizeConst::Parameter(name) = length {
                bind(name, Sort::USize, inferred, positions)?;
            }
        }
        Type::Borrow {
            access,
            region,
            pointee,
            ..
        } => {
            if let Some(name) = access {
                bind(name, Sort::Named("access".to_owned()), inferred, positions)?;
            }
            if let Some(name) = region {
                bind(name, Sort::Region, inferred, positions)?;
            }
            infer_extend_pattern(pointee, Sort::Type, false, schema, inferred, positions)?;
        }
        Type::Tuple(fields) => {
            for field in fields {
                infer_extend_pattern(field, Sort::Type, false, schema, inferred, positions)?;
            }
        }
        Type::Array(element, _) => {
            infer_extend_pattern(element, Sort::Type, false, schema, inferred, positions)?
        }
        Type::Function { .. }
        | Type::I8
        | Type::I16
        | Type::I32
        | Type::I64
        | Type::I128
        | Type::ISize
        | Type::U8
        | Type::U16
        | Type::U32
        | Type::U64
        | Type::U128
        | Type::USize
        | Type::Bool
        | Type::Unit
        | Type::CompileUSize(_) => {
            if !root && expected != Sort::Type {
                return Err(format!(
                    "extend pattern value has sort `type`, expected `{:?}`",
                    expected
                ));
            }
        }
    }
    Ok(())
}

pub(crate) fn normalize_and_validate_scopes(items: &mut [Item]) -> Result<(), String> {
    let empty = HashSet::new();
    for item in items {
        match item {
            Item::Function(function) => validate_function_scopes(function, &empty, &empty, &empty)?,
            Item::Global(binding) => validate_binding_scopes(binding, &empty, &empty)?,
            Item::TypeAlias(definition) => {
                let mut names = HashSet::new();
                for parameter in definition.compile_groups.iter().flatten() {
                    if !names.insert(parameter.name.clone()) {
                        return Err(format!(
                            "duplicate compile-time parameter `{}`",
                            parameter.name
                        ));
                    }
                }
                let regions = declared_regions(&definition.compile_groups, &empty)?;
                let accesses = declared_accesses(&definition.compile_groups, &empty)?;
                normalize_type_region_qualifiers(&mut definition.target, &regions, &accesses)?;
                validate_type_regions(&definition.target, &regions)?;
                validate_type_accesses(&definition.target, &accesses)?;
            }
            Item::Effect(definition) => {
                reject_parameter_modifier_parameters(
                    &definition.compile_groups,
                    &format!("effect `{}`", definition.name),
                )?;
                reject_effect_parameters(
                    &definition.compile_groups,
                    &format!("effect `{}`", definition.name),
                )?;
                let regions = declared_regions(&definition.compile_groups, &empty)?;
                let accesses = declared_accesses(&definition.compile_groups, &empty)?;
                for operation in &mut definition.operations {
                    validate_function_scopes(operation, &regions, &accesses, &empty)?;
                }
            }
            Item::Sort(_) => {}
            Item::TypeForm(definition) => {
                let mut names = HashSet::new();
                for parameter in definition.compile_groups.iter().flatten() {
                    if !names.insert(parameter.name.clone()) {
                        return Err(format!(
                            "duplicate compile-time parameter `{}`",
                            parameter.name
                        ));
                    }
                }
                let _regions = declared_regions(&definition.compile_groups, &empty)?;
                let _accesses = declared_accesses(&definition.compile_groups, &empty)?;
            }
            Item::Struct(definition) => {
                reject_parameter_modifier_parameters(
                    &definition.compile_groups,
                    &format!("struct `{}`", definition.name),
                )?;
                reject_effect_parameters(
                    &definition.compile_groups,
                    &format!("struct `{}`", definition.name),
                )?;
                let regions = declared_regions(&definition.compile_groups, &empty)?;
                let accesses = declared_accesses(&definition.compile_groups, &empty)?;
                for field in &mut definition.fields {
                    normalize_type_region_qualifiers(&mut field.ty, &regions, &accesses)?;
                    validate_type_regions(&field.ty, &regions)?;
                    validate_type_accesses(&field.ty, &accesses)?;
                }
            }
            Item::Enum(definition) => {
                reject_parameter_modifier_parameters(
                    &definition.compile_groups,
                    &format!("enum `{}`", definition.name),
                )?;
                reject_effect_parameters(
                    &definition.compile_groups,
                    &format!("enum `{}`", definition.name),
                )?;
                let regions = declared_regions(&definition.compile_groups, &empty)?;
                let accesses = declared_accesses(&definition.compile_groups, &empty)?;
                for variant in &mut definition.variants {
                    match &mut variant.fields {
                        VariantFields::Unit => {}
                        VariantFields::Positional(types) => {
                            for ty in types {
                                normalize_type_region_qualifiers(ty, &regions, &accesses)?;
                                validate_type_regions(ty, &regions)?;
                                validate_type_accesses(ty, &accesses)?;
                            }
                        }
                        VariantFields::Named(fields) => {
                            for field in fields {
                                normalize_type_region_qualifiers(
                                    &mut field.ty,
                                    &regions,
                                    &accesses,
                                )?;
                                validate_type_regions(&field.ty, &regions)?;
                                validate_type_accesses(&field.ty, &accesses)?;
                            }
                        }
                    }
                }
            }
            Item::Trait(definition) => {
                reject_parameter_modifier_parameters(
                    &definition.compile_groups,
                    &format!("trait `{}`", definition.name),
                )?;
                let regions = declared_regions(&definition.compile_groups, &empty)?;
                let accesses = declared_accesses(&definition.compile_groups, &empty)?;
                let mut effects = definition
                    .compile_groups
                    .iter()
                    .flatten()
                    .filter(|parameter| parameter.kind.is_effect_classifier())
                    .map(|parameter| parameter.name.clone())
                    .collect::<HashSet<_>>();
                if definition.self_parameter.kind.is_effect_classifier() {
                    effects.insert(definition.self_parameter.name.clone());
                }
                for member in &mut definition.members {
                    match member {
                        TraitMember::Function(function) => {
                            validate_function_scopes(function, &regions, &accesses, &effects)?
                        }
                        TraitMember::AssociatedType {
                            name,
                            compile_groups,
                            default,
                            ..
                        } => {
                            reject_parameter_modifier_parameters(
                                compile_groups,
                                &format!("associated type `{}`", name),
                            )?;
                            reject_effect_parameters(
                                compile_groups,
                                &format!("associated type `{}`", name),
                            )?;
                            let member_regions = declared_regions(compile_groups, &regions)?;
                            let member_accesses = declared_accesses(compile_groups, &accesses)?;
                            if let Some(default) = default {
                                normalize_type_region_qualifiers(
                                    default,
                                    &member_regions,
                                    &member_accesses,
                                )?;
                                validate_type_regions(default, &member_regions)?;
                                validate_type_accesses(default, &member_accesses)?;
                            }
                        }
                    }
                }
            }
            Item::Extend(extension) => {
                reject_parameter_modifier_parameters(&extension.compile_groups, "extend header")?;
                reject_effect_parameters(&extension.compile_groups, "extend header")?;
                let regions = declared_regions(&extension.compile_groups, &empty)?;
                let accesses = declared_accesses(&extension.compile_groups, &empty)?;
                normalize_type_region_qualifiers(&mut extension.target, &regions, &accesses)?;
                validate_type_regions(&extension.target, &regions)?;
                validate_type_accesses(&extension.target, &accesses)?;
                if let Some(trait_ref) = &mut extension.trait_ref {
                    normalize_type_region_qualifiers(trait_ref, &regions, &accesses)?;
                    validate_type_regions(trait_ref, &regions)?;
                    validate_type_accesses(trait_ref, &accesses)?;
                }
                for predicate in &mut extension.where_predicates {
                    normalize_type_region_qualifiers(&mut predicate.subject, &regions, &accesses)?;
                    normalize_type_region_qualifiers(
                        &mut predicate.trait_ref,
                        &regions,
                        &accesses,
                    )?;
                    validate_type_regions(&predicate.subject, &regions)?;
                    validate_type_regions(&predicate.trait_ref, &regions)?;
                    for binding in &mut predicate.associated_types {
                        validate_associated_binding_scopes(binding, &regions, &accesses, &empty)?;
                    }
                }
                for member in &mut extension.members {
                    match member {
                        ExtendMember::Function(function) => {
                            validate_function_scopes(function, &regions, &accesses, &empty)?
                        }
                        ExtendMember::Const(binding) => {
                            validate_binding_scopes(binding, &regions, &accesses)?
                        }
                    }
                }
            }
        }
    }
    Ok(())
}

fn reject_parameter_modifier_parameters(
    groups: &[Vec<CompileParam>],
    owner: &str,
) -> Result<(), String> {
    if groups
        .iter()
        .flatten()
        .any(|parameter| parameter.kind.is_parameter_modifier())
    {
        Err(format!(
            "{owner} cannot declare a parameter modifier function; modifier parameters belong to functions"
        ))
    } else {
        Ok(())
    }
}

fn reject_effect_parameters(groups: &[Vec<CompileParam>], owner: &str) -> Result<(), String> {
    if groups
        .iter()
        .flatten()
        .any(|parameter| parameter.kind.is_effect_classifier())
    {
        Err(format!(
            "{owner} cannot declare an `effect` parameter; effect parameters belong to functions"
        ))
    } else {
        Ok(())
    }
}

fn declared_accesses(
    groups: &[Vec<CompileParam>],
    outer: &HashSet<String>,
) -> Result<HashSet<String>, String> {
    let mut accesses = outer.clone();
    for parameter in groups.iter().flatten() {
        if parameter.kind.is_access() && !accesses.insert(parameter.name.clone()) {
            return Err(format!("duplicate access parameter `{}`", parameter.name));
        }
    }
    Ok(accesses)
}

fn declared_regions(
    groups: &[Vec<CompileParam>],
    outer: &HashSet<String>,
) -> Result<HashSet<String>, String> {
    let mut regions = outer.clone();
    for parameter in groups.iter().flatten() {
        if parameter.kind != Sort::Region {
            continue;
        }
        if parameter.name == "static" {
            return Err(
                "region entity `'static` is predefined and cannot be redeclared".to_owned(),
            );
        }
        if !regions.insert(parameter.name.clone()) {
            return Err(format!("duplicate region parameter `{}`", parameter.name));
        }
    }
    Ok(regions)
}

fn validate_function_scopes(
    function: &mut Function,
    outer_regions: &HashSet<String>,
    outer_accesses: &HashSet<String>,
    outer_effects: &HashSet<String>,
) -> Result<(), String> {
    let regions = declared_regions(&function.compile_groups, outer_regions)?;
    let accesses = declared_accesses(&function.compile_groups, outer_accesses)?;
    let mut effects = outer_effects.clone();
    for parameter in function
        .compile_groups
        .iter()
        .flatten()
        .filter(|parameter| parameter.kind.is_effect_classifier())
    {
        if !effects.insert(parameter.name.clone()) {
            return Err(format!("duplicate effect parameter `{}`", parameter.name));
        }
    }
    let mut compile_names = HashSet::new();
    let fragments = function
        .compile_groups
        .iter()
        .flatten()
        .filter_map(|parameter| match parameter.kind {
            Sort::Fragment(kind) => Some((parameter.name.clone(), kind)),
            _ => None,
        })
        .collect::<HashMap<_, _>>();
    for parameter in function.compile_groups.iter().flatten() {
        if !compile_names.insert(parameter.name.clone()) {
            return Err(format!(
                "duplicate compile-time parameter `{}`",
                parameter.name
            ));
        }
    }
    for parameter in function.groups.iter_mut().flatten() {
        for modifier in &parameter.modifiers {
            if !compile_names.contains(modifier) && !matches!(modifier.as_str(), "copy" | "move") {
                return Err(format!("use of undeclared parameter modifier `{modifier}`"));
            }
        }
        if let Some(access) = &parameter.access {
            validate_access_name(access, &accesses)?;
        }
        if let Some(region) = &parameter.region {
            validate_region_name(region, &regions)?;
        }
        normalize_type_region_qualifiers(&mut parameter.ty, &regions, &accesses)?;
        validate_type_regions(&parameter.ty, &regions)?;
        validate_type_accesses(&parameter.ty, &accesses)?;
        validate_type_effects(&parameter.ty, &effects)?;
        validate_type_static_fragments(&parameter.ty, &fragments)?;
    }
    if let Some(return_type) = &mut function.return_type {
        normalize_type_region_qualifiers(return_type, &regions, &accesses)?;
        validate_type_regions(return_type, &regions)?;
        validate_type_accesses(return_type, &accesses)?;
        validate_type_effects(return_type, &effects)?;
        validate_type_static_fragments(return_type, &fragments)?;
    }
    for parameter in &function.effects.parameters {
        if !effects.contains(parameter) {
            return Err(format!("use of undeclared effect parameter `{parameter}`"));
        }
    }
    normalize_function_effect_region_qualifiers(&mut function.effects, &regions, &accesses)?;
    for effect in &function.effects.custom {
        validate_type_regions(effect, &regions)?;
        validate_type_accesses(effect, &accesses)?;
        validate_type_effects(effect, &effects)?;
        validate_type_static_fragments(effect, &fragments)?;
    }
    for predicate in &mut function.where_predicates {
        normalize_type_region_qualifiers(&mut predicate.subject, &regions, &accesses)?;
        normalize_type_region_qualifiers(&mut predicate.trait_ref, &regions, &accesses)?;
        validate_type_regions(&predicate.subject, &regions)?;
        validate_type_regions(&predicate.trait_ref, &regions)?;
        validate_type_effects(&predicate.subject, &effects)?;
        validate_trait_ref_effects(&predicate.trait_ref, &effects)?;
        validate_type_static_fragments(&predicate.subject, &fragments)?;
        validate_type_static_fragments(&predicate.trait_ref, &fragments)?;
        for binding in &mut predicate.associated_types {
            validate_associated_binding_scopes(binding, &regions, &accesses, &effects)?;
        }
    }
    if let Some(body) = &mut function.body {
        normalize_expr_region_qualifiers(body, &regions, &accesses)?;
        validate_expr_regions(body, &regions)?;
        validate_expr_accesses(body, &accesses)?;
    }
    Ok(())
}

fn validate_associated_binding_scopes(
    binding: &mut AssociatedTypeBinding,
    outer_regions: &HashSet<String>,
    outer_accesses: &HashSet<String>,
    effects: &HashSet<String>,
) -> Result<(), String> {
    reject_parameter_modifier_parameters(
        &binding.compile_groups,
        &format!("associated type equality `{}`", binding.name),
    )?;
    reject_effect_parameters(
        &binding.compile_groups,
        &format!("associated type equality `{}`", binding.name),
    )?;
    let regions = declared_regions(&binding.compile_groups, outer_regions)?;
    let accesses = declared_accesses(&binding.compile_groups, outer_accesses)?;
    normalize_type_region_qualifiers(&mut binding.ty, &regions, &accesses)?;
    validate_type_regions(&binding.ty, &regions)?;
    validate_type_accesses(&binding.ty, &accesses)?;
    validate_type_effects(&binding.ty, effects)
}

fn normalize_borrow_region_qualifier(
    access: &mut Option<String>,
    region: &mut Option<String>,
    regions: &HashSet<String>,
    accesses: &HashSet<String>,
) -> Result<(), String> {
    let Some(name) = access.as_ref() else {
        return Ok(());
    };
    if region.is_none() && regions.contains(name) {
        if accesses.contains(name) {
            return Err(format!(
                "borrow qualifier `{name}` is ambiguous between access and region parameters"
            ));
        }
        *region = access.take();
    }
    Ok(())
}

fn normalize_type_region_qualifiers(
    ty: &mut Type,
    regions: &HashSet<String>,
    accesses: &HashSet<String>,
) -> Result<(), String> {
    match ty {
        Type::Borrow {
            access,
            region,
            pointee,
            ..
        } => {
            normalize_borrow_region_qualifier(access, region, regions, accesses)?;
            normalize_type_region_qualifiers(pointee, regions, accesses)
        }
        Type::Array(element, _) | Type::ArrayApplication { element, .. } => {
            normalize_type_region_qualifiers(element, regions, accesses)
        }
        Type::Tuple(fields) => {
            for field in fields {
                normalize_type_region_qualifiers(field, regions, accesses)?;
            }
            Ok(())
        }
        Type::Function {
            groups,
            effects,
            result,
        } => {
            for ty in groups.iter_mut().flatten() {
                normalize_type_region_qualifiers(ty, regions, accesses)?;
            }
            normalize_function_effect_region_qualifiers(effects, regions, accesses)?;
            normalize_type_region_qualifiers(result, regions, accesses)
        }
        Type::Named(_, arguments) => {
            for argument in arguments {
                normalize_type_region_qualifiers(argument, regions, accesses)?;
            }
            Ok(())
        }
        Type::NamedArgs(_, arguments) => {
            for argument in arguments {
                normalize_type_region_qualifiers(&mut argument.ty, regions, accesses)?;
            }
            Ok(())
        }
        Type::I8
        | Type::I16
        | Type::I32
        | Type::I64
        | Type::I128
        | Type::ISize
        | Type::U8
        | Type::U16
        | Type::U32
        | Type::U64
        | Type::U128
        | Type::USize
        | Type::Bool
        | Type::Unit
        | Type::CompileUSize(_) => Ok(()),
    }
}

fn normalize_function_effect_region_qualifiers(
    effects: &mut FunctionEffects,
    regions: &HashSet<String>,
    accesses: &HashSet<String>,
) -> Result<(), String> {
    if let Some(error) = &mut effects.failure {
        normalize_type_region_qualifiers(error, regions, accesses)?;
    }
    for effect in &mut effects.custom {
        normalize_type_region_qualifiers(effect, regions, accesses)?;
    }
    Ok(())
}

fn normalize_expr_region_qualifiers(
    expression: &mut Expr,
    regions: &HashSet<String>,
    accesses: &HashSet<String>,
) -> Result<(), String> {
    match expression {
        Expr::Located { value, .. } => normalize_expr_region_qualifiers(value, regions, accesses),
        Expr::Type(ty) => normalize_type_region_qualifiers(ty, regions, accesses),
        Expr::Borrow { access, value, .. } => {
            if let Some(access) = access {
                if regions.contains(access) && !accesses.contains(access) {
                    return Err(format!(
                        "region parameter `{access}` cannot be used as a borrow expression access"
                    ));
                }
            }
            normalize_expr_region_qualifiers(value, regions, accesses)
        }
        Expr::Unary(_, value) | Expr::Try(value) | Expr::Throw(value) | Expr::Unsafe(value) => {
            normalize_expr_region_qualifiers(value, regions, accesses)
        }
        Expr::DoBlock { body } | Expr::Async { body } | Expr::Await(body) => {
            normalize_expr_region_qualifiers(body, regions, accesses)
        }
        Expr::Binary(left, _, right)
        | Expr::Coalesce(left, right)
        | Expr::Assign(left, right)
        | Expr::CompoundAssign(left, _, right) => {
            normalize_expr_region_qualifiers(left, regions, accesses)?;
            normalize_expr_region_qualifiers(right, regions, accesses)
        }
        Expr::HandlerCoalesce {
            scrutinee,
            success,
            fallback,
            ..
        } => {
            normalize_expr_region_qualifiers(scrutinee, regions, accesses)?;
            normalize_expr_region_qualifiers(success, regions, accesses)?;
            normalize_expr_region_qualifiers(fallback, regions, accesses)
        }
        Expr::HandlerChainCall(chain) => {
            normalize_expr_region_qualifiers(&mut chain.scrutinee, regions, accesses)?;
            for argument in chain.groups.iter_mut().flatten() {
                normalize_expr_region_qualifiers(&mut argument.value, regions, accesses)?;
            }
            normalize_expr_region_qualifiers(&mut chain.success, regions, accesses)?;
            normalize_expr_region_qualifiers(&mut chain.residual, regions, accesses)
        }
        Expr::Call(callee, arguments) => {
            normalize_expr_region_qualifiers(callee, regions, accesses)?;
            for argument in arguments {
                normalize_expr_region_qualifiers(&mut argument.value, regions, accesses)?;
            }
            Ok(())
        }
        Expr::StructLiteral {
            constructor,
            fields,
        } => {
            normalize_expr_region_qualifiers(constructor, regions, accesses)?;
            for field in fields {
                normalize_expr_region_qualifiers(&mut field.value, regions, accesses)?;
            }
            Ok(())
        }
        Expr::Member(base, _) | Expr::ChainMember(base, _) => {
            normalize_expr_region_qualifiers(base, regions, accesses)
        }
        Expr::Array(elements) | Expr::Tuple(elements) => {
            for element in elements {
                normalize_expr_region_qualifiers(element, regions, accesses)?;
            }
            Ok(())
        }
        Expr::Index { base, index } => {
            normalize_expr_region_qualifiers(base, regions, accesses)?;
            normalize_expr_region_qualifiers(index, regions, accesses)
        }
        Expr::Block(statements, tail) => {
            for statement in statements {
                match statement {
                    Stmt::Let(binding) => {
                        if let Some(annotation) = &mut binding.annotation {
                            normalize_type_region_qualifiers(annotation, regions, accesses)?;
                        }
                        normalize_expr_region_qualifiers(&mut binding.value, regions, accesses)?;
                    }
                    Stmt::Expr(expression) => {
                        normalize_expr_region_qualifiers(expression, regions, accesses)?
                    }
                }
            }
            if let Some(tail) = tail {
                normalize_expr_region_qualifiers(tail, regions, accesses)?;
            }
            Ok(())
        }
        Expr::Closure(parameters, body) => {
            for parameter in parameters {
                normalize_type_region_qualifiers(&mut parameter.ty, regions, accesses)?;
            }
            normalize_expr_region_qualifiers(body, regions, accesses)
        }
        Expr::PatternClosure { guard, body, .. } => {
            if let Some(guard) = guard {
                normalize_expr_region_qualifiers(guard, regions, accesses)?;
            }
            normalize_expr_region_qualifiers(body, regions, accesses)
        }
        Expr::If {
            condition,
            then_branch,
            else_branch,
        } => {
            normalize_expr_region_qualifiers(condition, regions, accesses)?;
            normalize_expr_region_qualifiers(then_branch, regions, accesses)?;
            if let Some(else_branch) = else_branch {
                normalize_expr_region_qualifiers(else_branch, regions, accesses)?;
            }
            Ok(())
        }
        Expr::Return(value) | Expr::Break(value) => {
            if let Some(value) = value {
                normalize_expr_region_qualifiers(value, regions, accesses)?;
            }
            Ok(())
        }
        Expr::While {
            condition, body, ..
        } => {
            normalize_expr_region_qualifiers(condition, regions, accesses)?;
            normalize_expr_region_qualifiers(body, regions, accesses)
        }
        Expr::Loop { body } => normalize_expr_region_qualifiers(body, regions, accesses),
        Expr::Match { scrutinee, arms } => {
            normalize_expr_region_qualifiers(scrutinee, regions, accesses)?;
            for arm in arms {
                if let Some(guard) = &mut arm.guard {
                    normalize_expr_region_qualifiers(guard, regions, accesses)?;
                }
                normalize_expr_region_qualifiers(&mut arm.body, regions, accesses)?;
            }
            Ok(())
        }
        Expr::Unit
        | Expr::Integer(_)
        | Expr::Bool(_)
        | Expr::String(_)
        | Expr::Name(_)
        | Expr::Continue => Ok(()),
    }
}

fn validate_type_effects(ty: &Type, effects: &HashSet<String>) -> Result<(), String> {
    match ty {
        Type::Named(name, _) if effects.contains(name) => Err(format!(
            "effect parameter `{name}` cannot be used as a runtime type"
        )),
        Type::NamedArgs(name, _) if effects.contains(name) => Err(format!(
            "effect parameter `{name}` cannot be used as a runtime type"
        )),
        Type::Borrow { pointee, .. }
        | Type::Array(pointee, _)
        | Type::ArrayApplication {
            element: pointee, ..
        } => validate_type_effects(pointee, effects),
        Type::Tuple(fields) => {
            for field in fields {
                validate_type_effects(field, effects)?;
            }
            Ok(())
        }
        Type::Function {
            groups,
            effects: function_effects,
            result,
        } => {
            for parameter in &function_effects.parameters {
                if !effects.contains(parameter) {
                    return Err(format!("use of undeclared effect parameter `{parameter}`"));
                }
            }
            for ty in groups.iter().flatten() {
                validate_type_effects(ty, effects)?;
            }
            if let Some(error) = &function_effects.failure {
                validate_type_effects(error, effects)?;
            }
            validate_type_effects(result, effects)
        }
        Type::Named(_, arguments) => {
            for argument in arguments {
                validate_type_effects(argument, effects)?;
            }
            Ok(())
        }
        Type::NamedArgs(_, arguments) => {
            for argument in arguments {
                validate_type_effects(&argument.ty, effects)?;
            }
            Ok(())
        }
        Type::I8
        | Type::I16
        | Type::I32
        | Type::I64
        | Type::I128
        | Type::ISize
        | Type::U8
        | Type::U16
        | Type::U32
        | Type::U64
        | Type::U128
        | Type::USize
        | Type::Bool
        | Type::Unit
        | Type::CompileUSize(_) => Ok(()),
    }
}

fn validate_type_static_fragments(
    ty: &Type,
    fragments: &HashMap<String, StaticFragmentKind>,
) -> Result<(), String> {
    match ty {
        Type::Named(name, arguments) => {
            if arguments.is_empty() {
                if let Some(kind) = fragments.get(name) {
                    let sort = match kind {
                        StaticFragmentKind::Constraint => "constraint",
                        StaticFragmentKind::Declaration => "declaration",
                    };
                    return Err(format!(
                        "{sort} fragment parameter `{name}` cannot be used as a runtime type"
                    ));
                }
            }
            for argument in arguments {
                validate_type_static_fragments(argument, fragments)?;
            }
        }
        Type::NamedArgs(name, arguments) => {
            if arguments.is_empty() {
                if let Some(kind) = fragments.get(name) {
                    let sort = match kind {
                        StaticFragmentKind::Constraint => "constraint",
                        StaticFragmentKind::Declaration => "declaration",
                    };
                    return Err(format!(
                        "{sort} fragment parameter `{name}` cannot be used as a runtime type"
                    ));
                }
            }
            for argument in arguments {
                validate_type_static_fragments(&argument.ty, fragments)?;
            }
        }
        Type::Borrow { pointee, .. }
        | Type::Array(pointee, _)
        | Type::ArrayApplication {
            element: pointee, ..
        } => validate_type_static_fragments(pointee, fragments)?,
        Type::Tuple(fields) => {
            for field in fields {
                validate_type_static_fragments(field, fragments)?;
            }
        }
        Type::Function {
            groups,
            effects,
            result,
        } => {
            for ty in groups.iter().flatten() {
                validate_type_static_fragments(ty, fragments)?;
            }
            if let Some(error) = &effects.failure {
                validate_type_static_fragments(error, fragments)?;
            }
            for effect in &effects.custom {
                validate_type_static_fragments(effect, fragments)?;
            }
            validate_type_static_fragments(result, fragments)?;
        }
        Type::I8
        | Type::I16
        | Type::I32
        | Type::I64
        | Type::I128
        | Type::ISize
        | Type::U8
        | Type::U16
        | Type::U32
        | Type::U64
        | Type::U128
        | Type::USize
        | Type::Bool
        | Type::Unit
        | Type::CompileUSize(_) => {}
    }
    Ok(())
}

fn validate_trait_ref_effects(trait_ref: &Type, effects: &HashSet<String>) -> Result<(), String> {
    let arguments = match trait_ref {
        Type::Named(_, arguments) => arguments.iter().collect::<Vec<_>>(),
        Type::NamedArgs(_, arguments) => arguments.iter().map(|argument| &argument.ty).collect(),
        _ => return validate_type_effects(trait_ref, effects),
    };
    for argument in arguments {
        if matches!(argument, Type::Named(name, nested)
            if nested.is_empty() && effects.contains(name))
        {
            continue;
        }
        validate_type_effects(argument, effects)?;
    }
    Ok(())
}

fn validate_access_name(access: &str, accesses: &HashSet<String>) -> Result<(), String> {
    if accesses.contains(access) {
        Ok(())
    } else {
        Err(format!(
            "use of undeclared access or region parameter `{access}`"
        ))
    }
}

fn validate_type_accesses(ty: &Type, accesses: &HashSet<String>) -> Result<(), String> {
    match ty {
        Type::Borrow {
            access, pointee, ..
        } => {
            if let Some(access) = access {
                validate_access_name(access, accesses)?;
            }
            validate_type_accesses(pointee, accesses)
        }
        Type::Array(element, _) | Type::ArrayApplication { element, .. } => {
            validate_type_accesses(element, accesses)
        }
        Type::Tuple(fields) => {
            for field in fields {
                validate_type_accesses(field, accesses)?;
            }
            Ok(())
        }
        Type::Function {
            groups,
            effects,
            result,
        } => {
            for ty in groups.iter().flatten() {
                validate_type_accesses(ty, accesses)?;
            }
            if let Some(error) = &effects.failure {
                validate_type_accesses(error, accesses)?;
            }
            validate_type_accesses(result, accesses)
        }
        Type::Named(_, arguments) => {
            for argument in arguments {
                validate_type_accesses(argument, accesses)?;
            }
            Ok(())
        }
        Type::NamedArgs(_, arguments) => {
            for argument in arguments {
                validate_type_accesses(&argument.ty, accesses)?;
            }
            Ok(())
        }
        Type::I8
        | Type::I16
        | Type::I32
        | Type::I64
        | Type::I128
        | Type::ISize
        | Type::U8
        | Type::U16
        | Type::U32
        | Type::U64
        | Type::U128
        | Type::USize
        | Type::Bool
        | Type::Unit
        | Type::CompileUSize(_) => Ok(()),
    }
}

fn validate_expr_accesses(expression: &Expr, accesses: &HashSet<String>) -> Result<(), String> {
    match expression {
        Expr::Located { value, .. } => validate_expr_accesses(value, accesses),
        Expr::Borrow { access, value, .. } => {
            if let Some(access) = access {
                validate_access_name(access, accesses)?;
            }
            validate_expr_accesses(value, accesses)
        }
        Expr::Unary(_, value) | Expr::Try(value) | Expr::Throw(value) | Expr::Unsafe(value) => {
            validate_expr_accesses(value, accesses)
        }
        Expr::DoBlock { body } | Expr::Async { body } | Expr::Await(body) => {
            validate_expr_accesses(body, accesses)
        }
        Expr::Binary(left, _, right)
        | Expr::Coalesce(left, right)
        | Expr::Assign(left, right)
        | Expr::CompoundAssign(left, _, right) => {
            validate_expr_accesses(left, accesses)?;
            validate_expr_accesses(right, accesses)
        }
        Expr::HandlerCoalesce {
            scrutinee,
            success,
            fallback,
            ..
        } => {
            validate_expr_accesses(scrutinee, accesses)?;
            validate_expr_accesses(success, accesses)?;
            validate_expr_accesses(fallback, accesses)
        }
        Expr::HandlerChainCall(chain) => {
            validate_expr_accesses(&chain.scrutinee, accesses)?;
            for argument in chain.groups.iter().flatten() {
                validate_expr_accesses(&argument.value, accesses)?;
            }
            validate_expr_accesses(&chain.success, accesses)?;
            validate_expr_accesses(&chain.residual, accesses)
        }
        Expr::Call(callee, arguments) => {
            validate_expr_accesses(callee, accesses)?;
            for argument in arguments {
                validate_expr_accesses(&argument.value, accesses)?;
            }
            Ok(())
        }
        Expr::StructLiteral {
            constructor,
            fields,
        } => {
            validate_expr_accesses(constructor, accesses)?;
            for field in fields {
                validate_expr_accesses(&field.value, accesses)?;
            }
            Ok(())
        }
        Expr::Member(base, _) | Expr::ChainMember(base, _) => {
            validate_expr_accesses(base, accesses)
        }
        Expr::Array(elements) | Expr::Tuple(elements) => {
            for element in elements {
                validate_expr_accesses(element, accesses)?;
            }
            Ok(())
        }
        Expr::Index { base, index } => {
            validate_expr_accesses(base, accesses)?;
            validate_expr_accesses(index, accesses)
        }
        Expr::Block(statements, tail) => {
            for statement in statements {
                match statement {
                    Stmt::Let(binding) => {
                        if let Some(annotation) = &binding.annotation {
                            validate_type_accesses(annotation, accesses)?;
                        }
                        validate_expr_accesses(&binding.value, accesses)?;
                    }
                    Stmt::Expr(expression) => validate_expr_accesses(expression, accesses)?,
                }
            }
            if let Some(tail) = tail {
                validate_expr_accesses(tail, accesses)?;
            }
            Ok(())
        }
        Expr::Closure(parameters, body) => {
            for parameter in parameters {
                if let Some(access) = &parameter.access {
                    validate_access_name(access, accesses)?;
                }
                validate_type_accesses(&parameter.ty, accesses)?;
            }
            validate_expr_accesses(body, accesses)
        }
        Expr::PatternClosure { guard, body, .. } => {
            if let Some(guard) = guard {
                validate_expr_accesses(guard, accesses)?;
            }
            validate_expr_accesses(body, accesses)
        }
        Expr::If {
            condition,
            then_branch,
            else_branch,
        } => {
            validate_expr_accesses(condition, accesses)?;
            validate_expr_accesses(then_branch, accesses)?;
            if let Some(else_branch) = else_branch {
                validate_expr_accesses(else_branch, accesses)?;
            }
            Ok(())
        }
        Expr::Return(value) | Expr::Break(value) => {
            if let Some(value) = value {
                validate_expr_accesses(value, accesses)?;
            }
            Ok(())
        }
        Expr::While {
            condition, body, ..
        } => {
            validate_expr_accesses(condition, accesses)?;
            validate_expr_accesses(body, accesses)
        }
        Expr::Loop { body } => validate_expr_accesses(body, accesses),
        Expr::Match { scrutinee, arms } => {
            validate_expr_accesses(scrutinee, accesses)?;
            for arm in arms {
                if let Some(guard) = &arm.guard {
                    validate_expr_accesses(guard, accesses)?;
                }
                validate_expr_accesses(&arm.body, accesses)?;
            }
            Ok(())
        }
        Expr::Type(_)
        | Expr::Unit
        | Expr::Integer(_)
        | Expr::Bool(_)
        | Expr::String(_)
        | Expr::Name(_)
        | Expr::Continue => Ok(()),
    }
}

fn validate_binding_scopes(
    binding: &mut Binding,
    regions: &HashSet<String>,
    accesses: &HashSet<String>,
) -> Result<(), String> {
    if let Some(annotation) = &mut binding.annotation {
        normalize_type_region_qualifiers(annotation, regions, accesses)?;
        validate_type_regions(annotation, regions)?;
        validate_type_accesses(annotation, accesses)?;
    }
    normalize_expr_region_qualifiers(&mut binding.value, regions, accesses)?;
    validate_expr_regions(&binding.value, regions)?;
    validate_expr_accesses(&binding.value, accesses)
}

fn validate_type_regions(ty: &Type, regions: &HashSet<String>) -> Result<(), String> {
    match ty {
        Type::Borrow {
            region, pointee, ..
        } => {
            if let Some(region) = region {
                validate_region_name(region, regions)?;
            }
            validate_type_regions(pointee, regions)
        }
        Type::Array(element, _) | Type::ArrayApplication { element, .. } => {
            validate_type_regions(element, regions)
        }
        Type::Tuple(fields) => {
            for field in fields {
                validate_type_regions(field, regions)?;
            }
            Ok(())
        }
        Type::Function {
            groups,
            effects,
            result,
        } => {
            for ty in groups.iter().flatten() {
                validate_type_regions(ty, regions)?;
            }
            if let Some(error) = &effects.failure {
                validate_type_regions(error, regions)?;
            }
            validate_type_regions(result, regions)
        }
        Type::Named(_, arguments) => {
            for argument in arguments {
                validate_type_regions(argument, regions)?;
            }
            Ok(())
        }
        Type::NamedArgs(_, arguments) => {
            for argument in arguments {
                validate_type_regions(&argument.ty, regions)?;
            }
            Ok(())
        }
        Type::I8
        | Type::I16
        | Type::I32
        | Type::I64
        | Type::I128
        | Type::ISize
        | Type::U8
        | Type::U16
        | Type::U32
        | Type::U64
        | Type::U128
        | Type::USize
        | Type::Bool
        | Type::Unit
        | Type::CompileUSize(_) => Ok(()),
    }
}

fn validate_region_name(region: &str, regions: &HashSet<String>) -> Result<(), String> {
    if region == "static" || regions.contains(region) {
        Ok(())
    } else {
        Err(format!(
            "use of undeclared region {}",
            display_region_name(region)
        ))
    }
}

fn display_region_name(region: &str) -> String {
    if region.chars().next().is_some_and(char::is_uppercase) {
        format!("`{region}`")
    } else {
        format!("`'{region}`")
    }
}

fn validate_expr_regions(expression: &Expr, regions: &HashSet<String>) -> Result<(), String> {
    match expression {
        Expr::Located { value, .. } => validate_expr_regions(value, regions),
        Expr::Type(_)
        | Expr::Unit
        | Expr::Integer(_)
        | Expr::Bool(_)
        | Expr::String(_)
        | Expr::Name(_)
        | Expr::Continue => Ok(()),
        Expr::Unary(_, value)
        | Expr::Try(value)
        | Expr::Throw(value)
        | Expr::Unsafe(value)
        | Expr::Borrow { value, .. } => validate_expr_regions(value, regions),
        Expr::DoBlock { body } | Expr::Async { body } | Expr::Await(body) => {
            validate_expr_regions(body, regions)
        }
        Expr::Binary(left, _, right)
        | Expr::Coalesce(left, right)
        | Expr::Assign(left, right)
        | Expr::CompoundAssign(left, _, right) => {
            validate_expr_regions(left, regions)?;
            validate_expr_regions(right, regions)
        }
        Expr::HandlerCoalesce {
            scrutinee,
            success,
            fallback,
            ..
        } => {
            validate_expr_regions(scrutinee, regions)?;
            validate_expr_regions(success, regions)?;
            validate_expr_regions(fallback, regions)
        }
        Expr::HandlerChainCall(chain) => {
            validate_expr_regions(&chain.scrutinee, regions)?;
            for argument in chain.groups.iter().flatten() {
                validate_expr_regions(&argument.value, regions)?;
            }
            validate_expr_regions(&chain.success, regions)?;
            validate_expr_regions(&chain.residual, regions)
        }
        Expr::Call(callee, arguments) => {
            validate_expr_regions(callee, regions)?;
            for argument in arguments {
                validate_expr_regions(&argument.value, regions)?;
            }
            Ok(())
        }
        Expr::StructLiteral {
            constructor,
            fields,
        } => {
            validate_expr_regions(constructor, regions)?;
            for field in fields {
                validate_expr_regions(&field.value, regions)?;
            }
            Ok(())
        }
        Expr::Member(base, _) | Expr::ChainMember(base, _) => validate_expr_regions(base, regions),
        Expr::Array(elements) | Expr::Tuple(elements) => {
            for element in elements {
                validate_expr_regions(element, regions)?;
            }
            Ok(())
        }
        Expr::Index { base, index } => {
            validate_expr_regions(base, regions)?;
            validate_expr_regions(index, regions)
        }
        Expr::Block(statements, tail) => {
            for statement in statements {
                match statement {
                    Stmt::Let(binding) => {
                        if let Some(annotation) = &binding.annotation {
                            validate_type_regions(annotation, regions)?;
                        }
                        validate_expr_regions(&binding.value, regions)?;
                    }
                    Stmt::Expr(expression) => validate_expr_regions(expression, regions)?,
                }
            }
            if let Some(tail) = tail {
                validate_expr_regions(tail, regions)?;
            }
            Ok(())
        }
        Expr::Closure(parameters, body) => {
            for parameter in parameters {
                if let Some(region) = &parameter.region {
                    validate_region_name(region, regions)?;
                }
                validate_type_regions(&parameter.ty, regions)?;
            }
            validate_expr_regions(body, regions)
        }
        Expr::PatternClosure { guard, body, .. } => {
            if let Some(guard) = guard {
                validate_expr_regions(guard, regions)?;
            }
            validate_expr_regions(body, regions)
        }
        Expr::If {
            condition,
            then_branch,
            else_branch,
        } => {
            validate_expr_regions(condition, regions)?;
            validate_expr_regions(then_branch, regions)?;
            if let Some(else_branch) = else_branch {
                validate_expr_regions(else_branch, regions)?;
            }
            Ok(())
        }
        Expr::Return(value) | Expr::Break(value) => {
            if let Some(value) = value {
                validate_expr_regions(value, regions)?;
            }
            Ok(())
        }
        Expr::While {
            condition, body, ..
        } => {
            validate_expr_regions(condition, regions)?;
            validate_expr_regions(body, regions)
        }
        Expr::Loop { body } => validate_expr_regions(body, regions),
        Expr::Match { scrutinee, arms } => {
            validate_expr_regions(scrutinee, regions)?;
            for arm in arms {
                if let Some(guard) = &arm.guard {
                    validate_expr_regions(guard, regions)?;
                }
                validate_expr_regions(&arm.body, regions)?;
            }
            Ok(())
        }
    }
}

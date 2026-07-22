use std::collections::{HashMap, HashSet};

use crate::ast::{
    Binding, CallArg, CompileParam, CompileParamKind, EnumDef, Expr, ExtendMember, Function, Item,
    Param, PassMode, Pattern, PatternFields, Program, Stmt, StructDef, TraitMember, Type,
    VariantFields,
};

use super::compile_time::{
    effect_identity_sources, effect_row_from_source, ACCESS_MUT_MARKER, ACCESS_SHARED_MARKER,
    EFFECT_PURE_MARKER, EFFECT_UNSAFE_MARKER, PASSING_AUTO_MARKER, PASSING_COPY_MARKER,
    PASSING_MOVE_MARKER,
};

pub(super) fn normalize_labeled_type_arguments<const N: usize>(
    programs: [&mut Program; N],
) -> Vec<String> {
    let constructor_parameters = programs
        .iter()
        .flat_map(|program| program.items.iter())
        .filter_map(|item| {
            let (name, groups) = match item {
                Item::Struct(definition) => (&definition.name, &definition.compile_groups),
                Item::Enum(definition) => (&definition.name, &definition.compile_groups),
                Item::Effect(definition) => (&definition.name, &definition.compile_groups),
                Item::Trait(definition) => (&definition.name, &definition.compile_groups),
                Item::TypeAlias(definition) => (&definition.name, &definition.compile_groups),
                Item::Function(_) | Item::Global(_) | Item::Domain(_) | Item::Extend(_) => {
                    return None;
                }
            };
            Some((
                name.clone(),
                groups.iter().flatten().cloned().collect::<Vec<_>>(),
            ))
        })
        .collect::<HashMap<_, _>>();
    let mut diagnostics = Vec::new();

    for program in programs {
        for item in &mut program.items {
            normalize_item_labeled_type_arguments(item, &constructor_parameters, &mut diagnostics);
        }
    }
    diagnostics.sort();
    diagnostics.dedup();
    diagnostics
}

fn normalize_item_labeled_type_arguments(
    item: &mut Item,
    constructor_parameters: &HashMap<String, Vec<CompileParam>>,
    diagnostics: &mut Vec<String>,
) {
    match item {
        Item::Function(function) => {
            normalize_function_labeled_type_arguments(function, constructor_parameters, diagnostics)
        }
        Item::Global(binding) => {
            if let Some(annotation) = &mut binding.annotation {
                normalize_type_labeled_arguments(annotation, constructor_parameters, diagnostics);
            }
            normalize_expr_labeled_type_arguments(
                &mut binding.value,
                constructor_parameters,
                diagnostics,
            );
        }
        Item::Struct(definition) => {
            for field in &mut definition.fields {
                normalize_type_labeled_arguments(
                    &mut field.ty,
                    constructor_parameters,
                    diagnostics,
                );
            }
        }
        Item::Enum(definition) => {
            for variant in &mut definition.variants {
                match &mut variant.fields {
                    VariantFields::Unit => {}
                    VariantFields::Positional(types) => {
                        for ty in types {
                            normalize_type_labeled_arguments(
                                ty,
                                constructor_parameters,
                                diagnostics,
                            );
                        }
                    }
                    VariantFields::Named(fields) => {
                        for field in fields {
                            normalize_type_labeled_arguments(
                                &mut field.ty,
                                constructor_parameters,
                                diagnostics,
                            );
                        }
                    }
                }
            }
        }
        Item::Effect(definition) => {
            for operation in &mut definition.operations {
                normalize_function_labeled_type_arguments(
                    operation,
                    constructor_parameters,
                    diagnostics,
                );
            }
        }
        Item::Trait(definition) => {
            for predicate in &mut definition.where_predicates {
                normalize_type_labeled_arguments(
                    &mut predicate.subject,
                    constructor_parameters,
                    diagnostics,
                );
                normalize_type_labeled_arguments(
                    &mut predicate.trait_ref,
                    constructor_parameters,
                    diagnostics,
                );
                for binding in &mut predicate.associated_types {
                    normalize_type_labeled_arguments(
                        &mut binding.ty,
                        constructor_parameters,
                        diagnostics,
                    );
                }
            }
            for member in &mut definition.members {
                match member {
                    TraitMember::Function(function) => normalize_function_labeled_type_arguments(
                        function,
                        constructor_parameters,
                        diagnostics,
                    ),
                    TraitMember::AssociatedType { default, .. } => {
                        if let Some(default) = default {
                            normalize_type_labeled_arguments(
                                default,
                                constructor_parameters,
                                diagnostics,
                            );
                        }
                    }
                }
            }
        }
        Item::Extend(extension) => {
            normalize_type_labeled_arguments(
                &mut extension.target,
                constructor_parameters,
                diagnostics,
            );
            if let Some(trait_ref) = &mut extension.trait_ref {
                normalize_type_labeled_arguments(trait_ref, constructor_parameters, diagnostics);
            }
            for predicate in &mut extension.where_predicates {
                normalize_type_labeled_arguments(
                    &mut predicate.subject,
                    constructor_parameters,
                    diagnostics,
                );
                normalize_type_labeled_arguments(
                    &mut predicate.trait_ref,
                    constructor_parameters,
                    diagnostics,
                );
                for binding in &mut predicate.associated_types {
                    normalize_type_labeled_arguments(
                        &mut binding.ty,
                        constructor_parameters,
                        diagnostics,
                    );
                }
            }
            for member in &mut extension.members {
                match member {
                    ExtendMember::Function(function) => normalize_function_labeled_type_arguments(
                        function,
                        constructor_parameters,
                        diagnostics,
                    ),
                    ExtendMember::Const(binding) => {
                        if let Some(annotation) = &mut binding.annotation {
                            normalize_type_labeled_arguments(
                                annotation,
                                constructor_parameters,
                                diagnostics,
                            );
                        }
                        normalize_expr_labeled_type_arguments(
                            &mut binding.value,
                            constructor_parameters,
                            diagnostics,
                        );
                    }
                }
            }
        }
        Item::TypeAlias(definition) => normalize_type_labeled_arguments(
            &mut definition.target,
            constructor_parameters,
            diagnostics,
        ),
        Item::Domain(_) => {}
    }
}

fn normalize_function_labeled_type_arguments(
    function: &mut Function,
    constructor_parameters: &HashMap<String, Vec<CompileParam>>,
    diagnostics: &mut Vec<String>,
) {
    for parameter in function.groups.iter_mut().flatten() {
        normalize_type_labeled_arguments(&mut parameter.ty, constructor_parameters, diagnostics);
    }
    if let Some(result) = &mut function.return_type {
        normalize_type_labeled_arguments(result, constructor_parameters, diagnostics);
    }
    if let Some(error) = &mut function.effects.throws {
        normalize_type_labeled_arguments(error, constructor_parameters, diagnostics);
    }
    for effect in &mut function.effects.custom {
        normalize_type_labeled_arguments(effect, constructor_parameters, diagnostics);
    }
    for predicate in &mut function.where_predicates {
        normalize_type_labeled_arguments(
            &mut predicate.subject,
            constructor_parameters,
            diagnostics,
        );
        normalize_type_labeled_arguments(
            &mut predicate.trait_ref,
            constructor_parameters,
            diagnostics,
        );
        for binding in &mut predicate.associated_types {
            normalize_type_labeled_arguments(&mut binding.ty, constructor_parameters, diagnostics);
        }
    }
    if let Some(body) = &mut function.body {
        normalize_expr_labeled_type_arguments(body, constructor_parameters, diagnostics);
    }
}

fn normalize_type_labeled_arguments(
    ty: &mut Type,
    constructor_parameters: &HashMap<String, Vec<CompileParam>>,
    diagnostics: &mut Vec<String>,
) {
    match ty {
        Type::Borrow { pointee, .. } => {
            normalize_type_labeled_arguments(pointee, constructor_parameters, diagnostics)
        }
        Type::Array(element, _) => {
            normalize_type_labeled_arguments(element, constructor_parameters, diagnostics)
        }
        Type::Function {
            groups,
            effects,
            result,
        } => {
            for ty in groups.iter_mut().flatten() {
                normalize_type_labeled_arguments(ty, constructor_parameters, diagnostics);
            }
            if let Some(error) = &mut effects.throws {
                normalize_type_labeled_arguments(error, constructor_parameters, diagnostics);
            }
            for effect in &mut effects.custom {
                normalize_type_labeled_arguments(effect, constructor_parameters, diagnostics);
            }
            normalize_type_labeled_arguments(result, constructor_parameters, diagnostics);
        }
        Type::Named(_, arguments) => {
            for argument in arguments {
                normalize_type_labeled_arguments(argument, constructor_parameters, diagnostics);
            }
        }
        Type::NamedArgs(name, arguments) => {
            for argument in &mut *arguments {
                normalize_type_labeled_arguments(
                    &mut argument.ty,
                    constructor_parameters,
                    diagnostics,
                );
            }
            let written = arguments.clone();
            let positional = written
                .iter()
                .map(|argument| argument.ty.clone())
                .collect::<Vec<_>>();
            let Some(parameters) = constructor_parameters.get(name) else {
                diagnostics.push(format!(
                    "labeled type arguments require a known type constructor `{name}`"
                ));
                *ty = Type::Named(name.clone(), positional);
                return;
            };
            if parameters
                .iter()
                .any(|parameter| parameter.kind != CompileParamKind::Type)
            {
                diagnostics.push(format!(
                    "type constructor `{name}` has non-type compile-time parameters and cannot be applied in a type position with labels"
                ));
                *ty = Type::Named(name.clone(), positional);
                return;
            }
            if written.len() != parameters.len() {
                diagnostics.push(format!(
                    "type argument count mismatch for `{name}`: expected {}, found {}",
                    parameters.len(),
                    written.len()
                ));
                *ty = Type::Named(name.clone(), positional);
                return;
            }
            let mut ordered = Vec::with_capacity(parameters.len());
            let mut seen = HashSet::new();
            let mut valid = true;
            for parameter in parameters {
                let mut matches = written
                    .iter()
                    .filter(|argument| argument.label.as_deref() == Some(parameter.name.as_str()));
                match (matches.next(), matches.next()) {
                    (Some(argument), None) => {
                        seen.insert(parameter.name.clone());
                        ordered.push(argument.ty.clone());
                    }
                    (Some(_), Some(_)) => {
                        diagnostics.push(format!(
                            "duplicate type argument `{}` for `{name}`",
                            parameter.name
                        ));
                        valid = false;
                    }
                    (None, _) => {
                        diagnostics.push(format!(
                            "missing type argument `{}` for `{name}`",
                            parameter.name
                        ));
                        valid = false;
                    }
                }
            }
            for argument in &written {
                if let Some(label) = &argument.label {
                    if !seen.contains(label) {
                        diagnostics.push(format!("unknown type argument `{label}` for `{name}`"));
                        valid = false;
                    }
                }
            }
            *ty = if valid {
                Type::Named(name.clone(), ordered)
            } else {
                Type::Named(name.clone(), positional)
            };
        }
        Type::I32 | Type::I64 | Type::U32 | Type::U64 | Type::Bool | Type::Unit => {}
    }
}

fn normalize_expr_labeled_type_arguments(
    expression: &mut Expr,
    constructor_parameters: &HashMap<String, Vec<CompileParam>>,
    diagnostics: &mut Vec<String>,
) {
    match expression {
        Expr::Type(_)
        | Expr::Unit
        | Expr::Integer(_)
        | Expr::Bool(_)
        | Expr::Name(_)
        | Expr::Continue => {}
        Expr::Unary(_, operand)
        | Expr::Try(operand)
        | Expr::Throw(operand)
        | Expr::Unsafe(operand) => {
            normalize_expr_labeled_type_arguments(operand, constructor_parameters, diagnostics)
        }
        Expr::DoBlock { body } => {
            normalize_expr_labeled_type_arguments(body, constructor_parameters, diagnostics)
        }
        Expr::Borrow { value, .. } => {
            normalize_expr_labeled_type_arguments(value, constructor_parameters, diagnostics)
        }
        Expr::Binary(left, _, right)
        | Expr::Coalesce(left, right)
        | Expr::Assign(left, right)
        | Expr::CompoundAssign(left, _, right) => {
            normalize_expr_labeled_type_arguments(left, constructor_parameters, diagnostics);
            normalize_expr_labeled_type_arguments(right, constructor_parameters, diagnostics);
        }
        Expr::HandlerCoalesce {
            scrutinee,
            success,
            fallback,
            ..
        } => {
            normalize_expr_labeled_type_arguments(scrutinee, constructor_parameters, diagnostics);
            normalize_expr_labeled_type_arguments(success, constructor_parameters, diagnostics);
            normalize_expr_labeled_type_arguments(fallback, constructor_parameters, diagnostics);
        }
        Expr::HandlerChainCall(chain) => {
            normalize_expr_labeled_type_arguments(
                &mut chain.scrutinee,
                constructor_parameters,
                diagnostics,
            );
            for argument in chain.groups.iter_mut().flatten() {
                normalize_expr_labeled_type_arguments(
                    &mut argument.value,
                    constructor_parameters,
                    diagnostics,
                );
            }
            normalize_expr_labeled_type_arguments(
                &mut chain.success,
                constructor_parameters,
                diagnostics,
            );
            normalize_expr_labeled_type_arguments(
                &mut chain.residual,
                constructor_parameters,
                diagnostics,
            );
        }
        Expr::Call(callee, arguments) => {
            normalize_expr_labeled_type_arguments(callee, constructor_parameters, diagnostics);
            for argument in arguments {
                normalize_expr_labeled_type_arguments(
                    &mut argument.value,
                    constructor_parameters,
                    diagnostics,
                );
            }
        }
        Expr::StructLiteral {
            constructor,
            fields,
        } => {
            normalize_expr_labeled_type_arguments(constructor, constructor_parameters, diagnostics);
            for field in fields {
                normalize_expr_labeled_type_arguments(
                    &mut field.value,
                    constructor_parameters,
                    diagnostics,
                );
            }
        }
        Expr::Member(base, _) | Expr::ChainMember(base, _) => {
            normalize_expr_labeled_type_arguments(base, constructor_parameters, diagnostics)
        }
        Expr::Array(elements) => {
            for element in elements {
                normalize_expr_labeled_type_arguments(element, constructor_parameters, diagnostics);
            }
        }
        Expr::Index { base, index } => {
            normalize_expr_labeled_type_arguments(base, constructor_parameters, diagnostics);
            normalize_expr_labeled_type_arguments(index, constructor_parameters, diagnostics);
        }
        Expr::Block(statements, tail) => {
            for statement in statements {
                match statement {
                    Stmt::Let(binding) => {
                        if let Some(annotation) = &mut binding.annotation {
                            normalize_type_labeled_arguments(
                                annotation,
                                constructor_parameters,
                                diagnostics,
                            );
                        }
                        normalize_expr_labeled_type_arguments(
                            &mut binding.value,
                            constructor_parameters,
                            diagnostics,
                        );
                    }
                    Stmt::Expr(expression) => normalize_expr_labeled_type_arguments(
                        expression,
                        constructor_parameters,
                        diagnostics,
                    ),
                }
            }
            if let Some(tail) = tail {
                normalize_expr_labeled_type_arguments(tail, constructor_parameters, diagnostics);
            }
        }
        Expr::Closure(parameters, body) => {
            for parameter in parameters {
                normalize_type_labeled_arguments(
                    &mut parameter.ty,
                    constructor_parameters,
                    diagnostics,
                );
            }
            normalize_expr_labeled_type_arguments(body, constructor_parameters, diagnostics);
        }
        Expr::If {
            condition,
            then_branch,
            else_branch,
        } => {
            normalize_expr_labeled_type_arguments(condition, constructor_parameters, diagnostics);
            normalize_expr_labeled_type_arguments(then_branch, constructor_parameters, diagnostics);
            if let Some(else_branch) = else_branch {
                normalize_expr_labeled_type_arguments(
                    else_branch,
                    constructor_parameters,
                    diagnostics,
                );
            }
        }
        Expr::Return(value) | Expr::Break(value) => {
            if let Some(value) = value {
                normalize_expr_labeled_type_arguments(value, constructor_parameters, diagnostics);
            }
        }
        Expr::While { condition, body } => {
            normalize_expr_labeled_type_arguments(condition, constructor_parameters, diagnostics);
            normalize_expr_labeled_type_arguments(body, constructor_parameters, diagnostics);
        }
        Expr::Loop { body } => {
            normalize_expr_labeled_type_arguments(body, constructor_parameters, diagnostics)
        }
        Expr::Match { scrutinee, arms } => {
            normalize_expr_labeled_type_arguments(scrutinee, constructor_parameters, diagnostics);
            for arm in arms {
                if let Some(guard) = &mut arm.guard {
                    normalize_expr_labeled_type_arguments(
                        guard,
                        constructor_parameters,
                        diagnostics,
                    );
                }
                normalize_expr_labeled_type_arguments(
                    &mut arm.body,
                    constructor_parameters,
                    diagnostics,
                );
            }
        }
    }
}

pub(super) fn expand_type_aliases<const N: usize>(mut programs: [&mut Program; N]) -> Vec<String> {
    let aliases = programs
        .iter()
        .flat_map(|program| program.items.iter())
        .filter_map(|item| match item {
            Item::TypeAlias(definition) => Some((definition.name.clone(), definition.clone())),
            _ => None,
        })
        .collect::<HashMap<_, _>>();
    let mut diagnostics = Vec::new();

    for program in &mut programs {
        let items = std::mem::take(&mut program.items);
        let visibilities = std::mem::take(&mut program.item_visibilities);
        let origins = std::mem::take(&mut program.item_origins);
        for ((mut item, visibility), origin) in items.into_iter().zip(visibilities).zip(origins) {
            if matches!(item, Item::TypeAlias(_)) {
                continue;
            }
            expand_item_aliases(&mut item, &aliases, &mut diagnostics);
            program.items.push(item);
            program.item_visibilities.push(visibility);
            program.item_origins.push(origin);
        }
    }
    diagnostics.sort();
    diagnostics.dedup();
    diagnostics
}

pub(super) fn collect_type_aliases<const N: usize>(
    programs: [&Program; N],
) -> HashMap<String, crate::ast::TypeAliasDef> {
    programs
        .iter()
        .flat_map(|program| program.items.iter())
        .filter_map(|item| match item {
            Item::TypeAlias(definition) => Some((definition.name.clone(), definition.clone())),
            _ => None,
        })
        .collect()
}

pub(super) fn promote_inferred_type_aliases<const N: usize>(programs: [&mut Program; N]) {
    let mut known_types = HashMap::from([
        ("i32".to_owned(), 0_usize),
        ("i64".to_owned(), 0),
        ("u32".to_owned(), 0),
        ("u64".to_owned(), 0),
        ("bool".to_owned(), 0),
    ]);
    for item in programs.iter().flat_map(|program| program.items.iter()) {
        match item {
            Item::Struct(definition) => {
                known_types.insert(
                    definition.name.clone(),
                    definition.compile_groups.iter().flatten().count(),
                );
            }
            Item::Enum(definition) => {
                known_types.insert(
                    definition.name.clone(),
                    definition.compile_groups.iter().flatten().count(),
                );
            }
            Item::TypeAlias(definition) => {
                known_types.insert(
                    definition.name.clone(),
                    definition.compile_groups.iter().flatten().count(),
                );
            }
            _ => {}
        }
    }

    loop {
        let mut promoted = Vec::new();
        for (program_index, program) in programs.iter().enumerate() {
            for (item_index, item) in program.items.iter().enumerate() {
                let Item::Global(binding) = item else {
                    continue;
                };
                if binding.mutable || binding.annotation.is_some() {
                    continue;
                }
                let Some(target) = expression_type_source(&binding.value) else {
                    continue;
                };
                let head = match &target {
                    Type::I32 => "i32",
                    Type::I64 => "i64",
                    Type::U32 => "u32",
                    Type::U64 => "u64",
                    Type::Bool => "bool",
                    Type::Named(name, _) => name,
                    _ => continue,
                };
                let Some(arity) = known_types.get(head).copied() else {
                    continue;
                };
                let source_arity = match &target {
                    Type::Named(_, arguments) => arguments.len(),
                    _ => 0,
                };
                let is_type_binding = match &binding.value {
                    Expr::Name(_) => arity == 0,
                    Expr::Call(_, _) => arity > 0 && source_arity == arity,
                    _ => false,
                };
                if is_type_binding {
                    promoted.push((program_index, item_index, binding.name.clone(), target));
                }
            }
        }
        if promoted.is_empty() {
            break;
        }
        for (program_index, item_index, name, target) in promoted {
            known_types.insert(name.clone(), 0);
            programs[program_index].items[item_index] = Item::TypeAlias(crate::ast::TypeAliasDef {
                name,
                compile_groups: Vec::new(),
                target,
            });
        }
    }
}

fn expand_item_aliases(
    item: &mut Item,
    aliases: &HashMap<String, crate::ast::TypeAliasDef>,
    diagnostics: &mut Vec<String>,
) {
    match item {
        Item::Function(function) => expand_function_aliases(function, aliases, diagnostics),
        Item::Global(binding) => expand_binding_aliases(binding, aliases, diagnostics),
        Item::Struct(definition) => {
            for field in &mut definition.fields {
                expand_alias_type(&mut field.ty, aliases, &mut Vec::new(), diagnostics);
            }
        }
        Item::Enum(definition) => {
            for variant in &mut definition.variants {
                match &mut variant.fields {
                    VariantFields::Unit => {}
                    VariantFields::Positional(types) => {
                        for ty in types {
                            expand_alias_type(ty, aliases, &mut Vec::new(), diagnostics);
                        }
                    }
                    VariantFields::Named(fields) => {
                        for field in fields {
                            expand_alias_type(&mut field.ty, aliases, &mut Vec::new(), diagnostics);
                        }
                    }
                }
            }
        }
        Item::Trait(definition) => {
            for predicate in &mut definition.where_predicates {
                expand_alias_type(
                    &mut predicate.subject,
                    aliases,
                    &mut Vec::new(),
                    diagnostics,
                );
                expand_alias_type(
                    &mut predicate.trait_ref,
                    aliases,
                    &mut Vec::new(),
                    diagnostics,
                );
                for binding in &mut predicate.associated_types {
                    expand_alias_type(&mut binding.ty, aliases, &mut Vec::new(), diagnostics);
                }
            }
            for member in &mut definition.members {
                match member {
                    TraitMember::Function(function) => {
                        expand_function_aliases(function, aliases, diagnostics)
                    }
                    TraitMember::AssociatedType { default, .. } => {
                        if let Some(default) = default {
                            expand_alias_type(default, aliases, &mut Vec::new(), diagnostics);
                        }
                    }
                }
            }
        }
        Item::Extend(extension) => {
            if !is_partial_alias_application(&extension.target, aliases) {
                expand_alias_type(&mut extension.target, aliases, &mut Vec::new(), diagnostics);
            }
            if let Some(trait_ref) = &mut extension.trait_ref {
                expand_alias_type(trait_ref, aliases, &mut Vec::new(), diagnostics);
            }
            for predicate in &mut extension.where_predicates {
                expand_alias_type(
                    &mut predicate.subject,
                    aliases,
                    &mut Vec::new(),
                    diagnostics,
                );
                expand_alias_type(
                    &mut predicate.trait_ref,
                    aliases,
                    &mut Vec::new(),
                    diagnostics,
                );
                for binding in &mut predicate.associated_types {
                    expand_alias_type(&mut binding.ty, aliases, &mut Vec::new(), diagnostics);
                }
            }
            for member in &mut extension.members {
                match member {
                    ExtendMember::Function(function) => {
                        expand_function_aliases(function, aliases, diagnostics)
                    }
                    ExtendMember::Const(binding) => {
                        expand_binding_aliases(binding, aliases, diagnostics)
                    }
                }
            }
        }
        Item::Effect(definition) => {
            for operation in &mut definition.operations {
                expand_function_aliases(operation, aliases, diagnostics);
            }
        }
        Item::Domain(_) => {}
        Item::TypeAlias(_) => unreachable!("aliases are removed before item expansion"),
    }
}

fn is_partial_alias_application(
    source: &Type,
    aliases: &HashMap<String, crate::ast::TypeAliasDef>,
) -> bool {
    let Type::Named(name, arguments) = source else {
        return false;
    };
    let Some(alias) = aliases.get(name) else {
        return false;
    };
    let parameters = alias.compile_groups.iter().flatten().count();
    !arguments.is_empty() && arguments.len() < parameters
}

pub(super) fn expand_function_aliases(
    function: &mut Function,
    aliases: &HashMap<String, crate::ast::TypeAliasDef>,
    diagnostics: &mut Vec<String>,
) {
    for parameter in function.groups.iter_mut().flatten() {
        expand_alias_type(&mut parameter.ty, aliases, &mut Vec::new(), diagnostics);
    }
    if let Some(result) = &mut function.return_type {
        expand_alias_type(result, aliases, &mut Vec::new(), diagnostics);
    }
    if let Some(error) = &mut function.effects.throws {
        expand_alias_type(error, aliases, &mut Vec::new(), diagnostics);
    }
    for effect in &mut function.effects.custom {
        expand_alias_type(effect, aliases, &mut Vec::new(), diagnostics);
    }
    for predicate in &mut function.where_predicates {
        expand_alias_type(
            &mut predicate.subject,
            aliases,
            &mut Vec::new(),
            diagnostics,
        );
        expand_alias_type(
            &mut predicate.trait_ref,
            aliases,
            &mut Vec::new(),
            diagnostics,
        );
        for binding in &mut predicate.associated_types {
            expand_alias_type(&mut binding.ty, aliases, &mut Vec::new(), diagnostics);
        }
    }
    if let Some(body) = &mut function.body {
        expand_expr_aliases(body, aliases, diagnostics);
    }
}

fn expand_binding_aliases(
    binding: &mut Binding,
    aliases: &HashMap<String, crate::ast::TypeAliasDef>,
    diagnostics: &mut Vec<String>,
) {
    if let Some(annotation) = &mut binding.annotation {
        expand_alias_type(annotation, aliases, &mut Vec::new(), diagnostics);
    }
    expand_expr_aliases(&mut binding.value, aliases, diagnostics);
}

pub(super) fn expand_alias_type(
    source: &mut Type,
    aliases: &HashMap<String, crate::ast::TypeAliasDef>,
    stack: &mut Vec<String>,
    diagnostics: &mut Vec<String>,
) {
    match source {
        Type::Borrow { pointee, .. } => expand_alias_type(pointee, aliases, stack, diagnostics),
        Type::Array(element, _) => expand_alias_type(element, aliases, stack, diagnostics),
        Type::Function { groups, result, .. } => {
            for ty in groups.iter_mut().flatten() {
                expand_alias_type(ty, aliases, stack, diagnostics);
            }
            expand_alias_type(result, aliases, stack, diagnostics);
        }
        Type::Named(name, arguments) => {
            for argument in &mut *arguments {
                expand_alias_type(argument, aliases, stack, diagnostics);
            }
            let Some(alias) = aliases.get(name) else {
                return;
            };
            let parameters = alias.compile_groups.iter().flatten().collect::<Vec<_>>();
            if arguments.len() != parameters.len() {
                diagnostics.push(format!(
                    "type-constructor argument count mismatch for alias `{name}`: expected {}, found {}",
                    parameters.len(),
                    arguments.len()
                ));
                return;
            }
            if let Some(start) = stack.iter().position(|candidate| candidate == name) {
                let mut cycle = stack[start..].to_vec();
                cycle.push(name.clone());
                diagnostics.push(format!("cyclic type alias: {}", cycle.join(" -> ")));
                return;
            }
            let substitutions = parameters
                .iter()
                .zip(arguments.iter())
                .map(|(parameter, argument)| (parameter.name.clone(), argument.clone()))
                .collect::<HashMap<_, _>>();
            let mut target = alias.target.clone();
            substitute_type_parameters(&mut target, &substitutions);
            stack.push(name.clone());
            expand_alias_type(&mut target, aliases, stack, diagnostics);
            stack.pop();
            *source = target;
        }
        Type::NamedArgs(name, arguments) => {
            for argument in arguments {
                expand_alias_type(&mut argument.ty, aliases, stack, diagnostics);
            }
            diagnostics.push(format!(
                "internal error: labeled type arguments for `{name}` were not normalized before type alias expansion"
            ));
        }
        Type::I32 | Type::I64 | Type::U32 | Type::U64 | Type::Bool | Type::Unit => {}
    }
}

fn expression_type_source(expression: &Expr) -> Option<Type> {
    match expression {
        Expr::Unit => Some(Type::Unit),
        Expr::Name(name) => Some(match name.as_str() {
            "i32" => Type::I32,
            "i64" => Type::I64,
            "u32" => Type::U32,
            "u64" => Type::U64,
            "bool" => Type::Bool,
            _ => Type::Named(name.clone(), Vec::new()),
        }),
        Expr::Call(callee, arguments)
            if arguments.iter().all(|argument| argument.label.is_none()) =>
        {
            let Expr::Name(name) = callee.as_ref() else {
                return None;
            };
            Some(Type::Named(
                name.clone(),
                arguments
                    .iter()
                    .map(|argument| expression_type_source(&argument.value))
                    .collect::<Option<Vec<_>>>()?,
            ))
        }
        _ => None,
    }
}

fn transparent_alias_constructor(alias: &crate::ast::TypeAliasDef) -> Option<&str> {
    let Type::Named(target, arguments) = &alias.target else {
        return None;
    };
    let parameters = alias.compile_groups.iter().flatten().collect::<Vec<_>>();
    (arguments.len() == parameters.len()
        && arguments
            .iter()
            .zip(parameters)
            .all(|(argument, parameter)| {
                matches!(argument, Type::Named(name, values)
                if values.is_empty() && name == &parameter.name)
            }))
    .then_some(target.as_str())
}

fn expand_expr_aliases(
    expression: &mut Expr,
    aliases: &HashMap<String, crate::ast::TypeAliasDef>,
    diagnostics: &mut Vec<String>,
) {
    if let Expr::Call(callee, arguments) = expression {
        if let Expr::Name(name) = callee.as_ref() {
            if let Some(alias) = aliases.get(name) {
                let expected = alias.compile_groups.iter().flatten().count();
                if arguments.len() == expected
                    && arguments.iter().all(|argument| argument.label.is_none())
                {
                    if let Some(source_arguments) = arguments
                        .iter()
                        .map(|argument| expression_type_source(&argument.value))
                        .collect::<Option<Vec<_>>>()
                    {
                        let mut source = Type::Named(name.clone(), source_arguments);
                        expand_alias_type(&mut source, aliases, &mut Vec::new(), diagnostics);
                        *expression = source_type_expression(&source);
                    }
                }
            }
        }
    }

    match expression {
        Expr::Name(name) => {
            if let Some(alias) = aliases.get(name) {
                if let Some(target) = transparent_alias_constructor(alias) {
                    *name = target.to_owned();
                }
            }
        }
        Expr::Type(_) | Expr::Unit | Expr::Integer(_) | Expr::Bool(_) | Expr::Continue => {}
        Expr::Unary(_, operand)
        | Expr::Try(operand)
        | Expr::Throw(operand)
        | Expr::Unsafe(operand) => expand_expr_aliases(operand, aliases, diagnostics),
        Expr::DoBlock { body } => expand_expr_aliases(body, aliases, diagnostics),
        Expr::Borrow { value, .. } => expand_expr_aliases(value, aliases, diagnostics),
        Expr::Binary(left, _, right)
        | Expr::Coalesce(left, right)
        | Expr::Assign(left, right)
        | Expr::CompoundAssign(left, _, right) => {
            expand_expr_aliases(left, aliases, diagnostics);
            expand_expr_aliases(right, aliases, diagnostics);
        }
        Expr::HandlerCoalesce {
            scrutinee,
            success,
            fallback,
            ..
        } => {
            expand_expr_aliases(scrutinee, aliases, diagnostics);
            expand_expr_aliases(success, aliases, diagnostics);
            expand_expr_aliases(fallback, aliases, diagnostics);
        }
        Expr::HandlerChainCall(chain) => {
            expand_expr_aliases(&mut chain.scrutinee, aliases, diagnostics);
            for argument in chain.groups.iter_mut().flatten() {
                expand_expr_aliases(&mut argument.value, aliases, diagnostics);
            }
            expand_expr_aliases(&mut chain.success, aliases, diagnostics);
            expand_expr_aliases(&mut chain.residual, aliases, diagnostics);
        }
        Expr::Call(callee, arguments) => {
            expand_expr_aliases(callee, aliases, diagnostics);
            for argument in arguments {
                expand_expr_aliases(&mut argument.value, aliases, diagnostics);
            }
        }
        Expr::StructLiteral {
            constructor,
            fields,
        } => {
            expand_expr_aliases(constructor, aliases, diagnostics);
            for field in fields {
                expand_expr_aliases(&mut field.value, aliases, diagnostics);
            }
        }
        Expr::Member(base, _) | Expr::ChainMember(base, _) => {
            expand_expr_aliases(base, aliases, diagnostics)
        }
        Expr::Array(elements) => {
            for element in elements {
                expand_expr_aliases(element, aliases, diagnostics);
            }
        }
        Expr::Index { base, index } => {
            expand_expr_aliases(base, aliases, diagnostics);
            expand_expr_aliases(index, aliases, diagnostics);
        }
        Expr::Block(statements, tail) => {
            for statement in statements {
                match statement {
                    Stmt::Let(binding) => expand_binding_aliases(binding, aliases, diagnostics),
                    Stmt::Expr(expression) => expand_expr_aliases(expression, aliases, diagnostics),
                }
            }
            if let Some(tail) = tail {
                expand_expr_aliases(tail, aliases, diagnostics);
            }
        }
        Expr::Closure(parameters, body) => {
            for parameter in parameters {
                expand_alias_type(&mut parameter.ty, aliases, &mut Vec::new(), diagnostics);
            }
            expand_expr_aliases(body, aliases, diagnostics);
        }
        Expr::If {
            condition,
            then_branch,
            else_branch,
        } => {
            expand_expr_aliases(condition, aliases, diagnostics);
            expand_expr_aliases(then_branch, aliases, diagnostics);
            if let Some(else_branch) = else_branch {
                expand_expr_aliases(else_branch, aliases, diagnostics);
            }
        }
        Expr::Return(value) | Expr::Break(value) => {
            if let Some(value) = value {
                expand_expr_aliases(value, aliases, diagnostics);
            }
        }
        Expr::While { condition, body } => {
            expand_expr_aliases(condition, aliases, diagnostics);
            expand_expr_aliases(body, aliases, diagnostics);
        }
        Expr::Loop { body } => expand_expr_aliases(body, aliases, diagnostics),
        Expr::Match { scrutinee, arms } => {
            expand_expr_aliases(scrutinee, aliases, diagnostics);
            for arm in arms {
                if let Some(guard) = &mut arm.guard {
                    expand_expr_aliases(guard, aliases, diagnostics);
                }
                expand_expr_aliases(&mut arm.body, aliases, diagnostics);
            }
        }
    }
}

pub(super) fn substitute_struct_types(
    definition: &mut StructDef,
    substitutions: &HashMap<String, Type>,
) {
    for field in &mut definition.fields {
        substitute_type_parameters(&mut field.ty, substitutions);
    }
}

pub(super) fn erase_region_parameters(program: &mut Program) {
    fn erase_groups(groups: &mut Vec<Vec<CompileParam>>) {
        for group in &mut *groups {
            group.retain(|parameter| parameter.kind != CompileParamKind::Region);
        }
        groups.retain(|group| !group.is_empty());
    }

    fn erase_function(function: &mut Function) {
        erase_groups(&mut function.compile_groups);
    }

    for item in &mut program.items {
        match item {
            Item::Function(function) => erase_function(function),
            Item::Global(_) => {}
            Item::TypeAlias(definition) => erase_groups(&mut definition.compile_groups),
            Item::Effect(definition) => {
                erase_groups(&mut definition.compile_groups);
                for operation in &mut definition.operations {
                    erase_function(operation);
                }
            }
            Item::Domain(_) => {}
            Item::Struct(definition) => erase_groups(&mut definition.compile_groups),
            Item::Enum(definition) => erase_groups(&mut definition.compile_groups),
            Item::Trait(definition) => {
                erase_groups(&mut definition.compile_groups);
                for member in &mut definition.members {
                    match member {
                        TraitMember::Function(function) => erase_function(function),
                        TraitMember::AssociatedType { compile_groups, .. } => {
                            erase_groups(compile_groups)
                        }
                    }
                }
            }
            Item::Extend(extension) => {
                erase_groups(&mut extension.compile_groups);
                for member in &mut extension.members {
                    if let ExtendMember::Function(function) = member {
                        erase_function(function);
                    }
                }
            }
        }
    }
}

pub(super) fn substitute_enum_types(
    definition: &mut EnumDef,
    substitutions: &HashMap<String, Type>,
) {
    for variant in &mut definition.variants {
        match &mut variant.fields {
            VariantFields::Unit => {}
            VariantFields::Positional(types) => {
                for ty in types {
                    substitute_type_parameters(ty, substitutions);
                }
            }
            VariantFields::Named(fields) => {
                for field in fields {
                    substitute_type_parameters(&mut field.ty, substitutions);
                }
            }
        }
    }
}

pub(super) fn substitute_function_types(
    function: &mut Function,
    substitutions: &HashMap<String, Type>,
) {
    let had_throws = function.effects.throws.is_some();
    for group in &mut function.groups {
        for parameter in group {
            substitute_parameter_types(parameter, substitutions);
        }
    }
    if let Some(result) = &mut function.return_type {
        substitute_type_parameters(result, substitutions);
    }
    if let Some(error) = &mut function.effects.throws {
        substitute_type_parameters(error, substitutions);
    }
    for effect in &mut function.effects.custom {
        substitute_type_parameters(effect, substitutions);
    }
    let mut remaining_effect_parameters = Vec::new();
    for parameter in function.effects.parameters.drain(..) {
        match substituted_effect_row(&parameter, substitutions) {
            Some((unsafe_effect, throws_error, custom))
                if throws_error.as_ref().is_none_or(|selected| {
                    function
                        .effects
                        .throws
                        .as_deref()
                        .is_none_or(|fixed| fixed == selected)
                }) =>
            {
                function.effects.unsafe_effect |= unsafe_effect;
                if function.effects.throws.is_none() {
                    function.effects.throws = throws_error.map(Box::new);
                }
                function
                    .effects
                    .custom
                    .extend(effect_identity_sources(&custom));
            }
            Some(_) | None => remaining_effect_parameters.push(parameter),
        }
    }
    let mut seen_custom = HashSet::new();
    function
        .effects
        .custom
        .retain(|effect| seen_custom.insert(effect.clone()));
    function.effects.parameters = remaining_effect_parameters;
    if !had_throws {
        if let Some(error) = function.effects.throws.as_deref() {
            let Some(result) = function.return_type.take() else {
                return;
            };
            function.return_type = Some(Type::Named(
                "core::Result".to_owned(),
                vec![error.clone(), result],
            ));
        }
    }
    for predicate in &mut function.where_predicates {
        substitute_type_parameters(&mut predicate.subject, substitutions);
        substitute_type_parameters(&mut predicate.trait_ref, substitutions);
        for binding in &mut predicate.associated_types {
            substitute_type_parameters(&mut binding.ty, substitutions);
        }
    }
    if let Some(body) = &mut function.body {
        substitute_expr_types(body, substitutions);
    }
}

pub(super) fn substitute_where_predicate(
    predicate: &mut crate::ast::WherePredicate,
    substitutions: &HashMap<String, Type>,
) {
    substitute_type_parameters(&mut predicate.subject, substitutions);
    substitute_type_parameters(&mut predicate.trait_ref, substitutions);
    for binding in &mut predicate.associated_types {
        substitute_type_parameters(&mut binding.ty, substitutions);
    }
}

fn substitute_parameter_types(parameter: &mut Param, substitutions: &HashMap<String, Type>) {
    if let Some(access) = parameter.access.as_deref() {
        if let Some(mutable) = substituted_access_mutability(access, substitutions) {
            parameter.mode = if mutable {
                PassMode::MutBorrow
            } else {
                PassMode::Borrow
            };
            parameter.access = None;
        }
    }
    if let Some(passing) = parameter.passing.as_deref() {
        if let Some(mode) = substituted_passing_mode(passing, substitutions) {
            parameter.mode = mode;
            parameter.passing = None;
        }
    }
    substitute_type_parameters(&mut parameter.ty, substitutions);
}

pub(super) fn substitute_self_expression_target(expression: &mut Expr, target: &str) {
    match expression {
        Expr::Name(name) if name == "Self" => *name = target.to_owned(),
        Expr::Type(_)
        | Expr::Unit
        | Expr::Integer(_)
        | Expr::Bool(_)
        | Expr::Name(_)
        | Expr::Continue => {}
        Expr::Unary(_, operand)
        | Expr::Try(operand)
        | Expr::Throw(operand)
        | Expr::Unsafe(operand) => substitute_self_expression_target(operand, target),
        Expr::DoBlock { body } => substitute_self_expression_target(body, target),
        Expr::Borrow { value, .. } => substitute_self_expression_target(value, target),
        Expr::Binary(left, _, right)
        | Expr::Coalesce(left, right)
        | Expr::Assign(left, right)
        | Expr::CompoundAssign(left, _, right) => {
            substitute_self_expression_target(left, target);
            substitute_self_expression_target(right, target);
        }
        Expr::HandlerCoalesce {
            scrutinee,
            success,
            fallback,
            ..
        } => {
            substitute_self_expression_target(scrutinee, target);
            substitute_self_expression_target(success, target);
            substitute_self_expression_target(fallback, target);
        }
        Expr::HandlerChainCall(chain) => {
            substitute_self_expression_target(&mut chain.scrutinee, target);
            for argument in chain.groups.iter_mut().flatten() {
                substitute_self_expression_target(&mut argument.value, target);
            }
            substitute_self_expression_target(&mut chain.success, target);
            substitute_self_expression_target(&mut chain.residual, target);
        }
        Expr::Call(callee, arguments) => {
            substitute_self_expression_target(callee, target);
            for argument in arguments {
                substitute_self_expression_target(&mut argument.value, target);
            }
        }
        Expr::StructLiteral {
            constructor,
            fields,
        } => {
            substitute_self_expression_target(constructor, target);
            for field in fields {
                substitute_self_expression_target(&mut field.value, target);
            }
        }
        Expr::Member(base, _) | Expr::ChainMember(base, _) => {
            substitute_self_expression_target(base, target)
        }
        Expr::Array(elements) => {
            for element in elements {
                substitute_self_expression_target(element, target);
            }
        }
        Expr::Index { base, index } => {
            substitute_self_expression_target(base, target);
            substitute_self_expression_target(index, target);
        }
        Expr::Block(statements, tail) => {
            for statement in statements {
                match statement {
                    Stmt::Let(binding) => {
                        substitute_self_expression_target(&mut binding.value, target)
                    }
                    Stmt::Expr(expression) => substitute_self_expression_target(expression, target),
                }
            }
            if let Some(tail) = tail {
                substitute_self_expression_target(tail, target);
            }
        }
        Expr::Closure(_, body) => substitute_self_expression_target(body, target),
        Expr::If {
            condition,
            then_branch,
            else_branch,
        } => {
            substitute_self_expression_target(condition, target);
            substitute_self_expression_target(then_branch, target);
            if let Some(else_branch) = else_branch {
                substitute_self_expression_target(else_branch, target);
            }
        }
        Expr::Return(value) | Expr::Break(value) => {
            if let Some(value) = value {
                substitute_self_expression_target(value, target);
            }
        }
        Expr::While { condition, body } => {
            substitute_self_expression_target(condition, target);
            substitute_self_expression_target(body, target);
        }
        Expr::Loop { body } => substitute_self_expression_target(body, target),
        Expr::Match { scrutinee, arms } => {
            substitute_self_expression_target(scrutinee, target);
            for arm in arms {
                substitute_self_pattern_target(&mut arm.pattern, target);
                if let Some(guard) = &mut arm.guard {
                    substitute_self_expression_target(guard, target);
                }
                substitute_self_expression_target(&mut arm.body, target);
            }
        }
    }
}

fn substitute_self_pattern_target(pattern: &mut Pattern, target: &str) {
    let Pattern::Constructor { path, fields } = pattern else {
        return;
    };
    if path.first().is_some_and(|segment| segment == "Self") {
        path[0] = target.to_owned();
    }
    match fields {
        PatternFields::Unit => {}
        PatternFields::Positional(patterns) => {
            for pattern in patterns {
                substitute_self_pattern_target(pattern, target);
            }
        }
        PatternFields::Named(fields) => {
            for field in fields {
                substitute_self_pattern_target(&mut field.pattern, target);
            }
        }
    }
}

pub(super) fn rewrite_abstract_self_qualified_methods(expression: &mut Expr) {
    match expression {
        Expr::Call(callee, arguments) => {
            rewrite_abstract_self_qualified_methods(callee);
            for argument in &mut *arguments {
                rewrite_abstract_self_qualified_methods(&mut argument.value);
            }
            let replacement = match (callee.as_ref(), arguments.as_slice()) {
                (
                    Expr::Member(base, member),
                    [CallArg {
                        label: Some(label),
                        value,
                    }],
                ) if matches!(base.as_ref(), Expr::Name(name) if name == "Self")
                    && label == "self" =>
                {
                    Some(Expr::Member(Box::new(value.clone()), member.clone()))
                }
                _ => None,
            };
            if let Some(replacement) = replacement {
                *expression = replacement;
            }
        }
        Expr::Type(_)
        | Expr::Unit
        | Expr::Integer(_)
        | Expr::Bool(_)
        | Expr::Name(_)
        | Expr::Continue => {}
        Expr::Unary(_, operand)
        | Expr::Try(operand)
        | Expr::Throw(operand)
        | Expr::Unsafe(operand) => rewrite_abstract_self_qualified_methods(operand),
        Expr::DoBlock { body } => rewrite_abstract_self_qualified_methods(body),
        Expr::Borrow { value, .. } => rewrite_abstract_self_qualified_methods(value),
        Expr::Binary(left, _, right)
        | Expr::Coalesce(left, right)
        | Expr::Assign(left, right)
        | Expr::CompoundAssign(left, _, right) => {
            rewrite_abstract_self_qualified_methods(left);
            rewrite_abstract_self_qualified_methods(right);
        }
        Expr::HandlerCoalesce {
            scrutinee,
            success,
            fallback,
            ..
        } => {
            rewrite_abstract_self_qualified_methods(scrutinee);
            rewrite_abstract_self_qualified_methods(success);
            rewrite_abstract_self_qualified_methods(fallback);
        }
        Expr::HandlerChainCall(chain) => {
            rewrite_abstract_self_qualified_methods(&mut chain.scrutinee);
            for argument in chain.groups.iter_mut().flatten() {
                rewrite_abstract_self_qualified_methods(&mut argument.value);
            }
            rewrite_abstract_self_qualified_methods(&mut chain.success);
            rewrite_abstract_self_qualified_methods(&mut chain.residual);
        }
        Expr::StructLiteral {
            constructor,
            fields,
        } => {
            rewrite_abstract_self_qualified_methods(constructor);
            for field in fields {
                rewrite_abstract_self_qualified_methods(&mut field.value);
            }
        }
        Expr::Member(base, _) | Expr::ChainMember(base, _) => {
            rewrite_abstract_self_qualified_methods(base)
        }
        Expr::Array(elements) => {
            for element in elements {
                rewrite_abstract_self_qualified_methods(element);
            }
        }
        Expr::Index { base, index } => {
            rewrite_abstract_self_qualified_methods(base);
            rewrite_abstract_self_qualified_methods(index);
        }
        Expr::Block(statements, tail) => {
            for statement in statements {
                match statement {
                    Stmt::Let(binding) => {
                        rewrite_abstract_self_qualified_methods(&mut binding.value)
                    }
                    Stmt::Expr(expression) => rewrite_abstract_self_qualified_methods(expression),
                }
            }
            if let Some(tail) = tail {
                rewrite_abstract_self_qualified_methods(tail);
            }
        }
        Expr::Closure(_, body) => rewrite_abstract_self_qualified_methods(body),
        Expr::If {
            condition,
            then_branch,
            else_branch,
        } => {
            rewrite_abstract_self_qualified_methods(condition);
            rewrite_abstract_self_qualified_methods(then_branch);
            if let Some(else_branch) = else_branch {
                rewrite_abstract_self_qualified_methods(else_branch);
            }
        }
        Expr::Return(value) | Expr::Break(value) => {
            if let Some(value) = value {
                rewrite_abstract_self_qualified_methods(value);
            }
        }
        Expr::While { condition, body } => {
            rewrite_abstract_self_qualified_methods(condition);
            rewrite_abstract_self_qualified_methods(body);
        }
        Expr::Loop { body } => rewrite_abstract_self_qualified_methods(body),
        Expr::Match { scrutinee, arms } => {
            rewrite_abstract_self_qualified_methods(scrutinee);
            for arm in arms {
                if let Some(guard) = &mut arm.guard {
                    rewrite_abstract_self_qualified_methods(guard);
                }
                rewrite_abstract_self_qualified_methods(&mut arm.body);
            }
        }
    }
}

pub(super) fn substitute_expr_types(expression: &mut Expr, substitutions: &HashMap<String, Type>) {
    match expression {
        Expr::Name(name) => {
            if let Some(replacement) = substitutions.get(name) {
                if effect_row_from_source(replacement).is_some() {
                    *expression = source_type_expression(replacement);
                    return;
                }
            }
            if let Some(Type::Named(marker, arguments)) = substitutions.get(name) {
                if arguments.is_empty() {
                    match marker.as_str() {
                        ACCESS_SHARED_MARKER => *name = "shared".to_owned(),
                        ACCESS_MUT_MARKER => *name = "mut".to_owned(),
                        PASSING_AUTO_MARKER => *name = "auto".to_owned(),
                        PASSING_COPY_MARKER => *name = "copy".to_owned(),
                        PASSING_MOVE_MARKER => *name = "move".to_owned(),
                        EFFECT_PURE_MARKER => *name = "pure".to_owned(),
                        EFFECT_UNSAFE_MARKER => *name = "unsafe".to_owned(),
                        _ => {}
                    }
                }
            }
        }
        Expr::Type(_) | Expr::Unit | Expr::Integer(_) | Expr::Bool(_) | Expr::Continue => {}
        Expr::Unary(_, operand)
        | Expr::Try(operand)
        | Expr::Throw(operand)
        | Expr::Unsafe(operand) => substitute_expr_types(operand, substitutions),
        Expr::DoBlock { body } => substitute_expr_types(body, substitutions),
        Expr::Borrow {
            mutable,
            access,
            value,
        } => {
            if let Some(name) = access.as_deref() {
                if let Some(selected) = substituted_access_mutability(name, substitutions) {
                    *mutable = selected;
                    *access = None;
                }
            }
            substitute_expr_types(value, substitutions)
        }
        Expr::Binary(left, _, right)
        | Expr::Coalesce(left, right)
        | Expr::Assign(left, right)
        | Expr::CompoundAssign(left, _, right) => {
            substitute_expr_types(left, substitutions);
            substitute_expr_types(right, substitutions);
        }
        Expr::HandlerCoalesce {
            scrutinee,
            success,
            fallback,
            ..
        } => {
            substitute_expr_types(scrutinee, substitutions);
            substitute_expr_types(success, substitutions);
            substitute_expr_types(fallback, substitutions);
        }
        Expr::HandlerChainCall(chain) => {
            substitute_expr_types(&mut chain.scrutinee, substitutions);
            for argument in chain.groups.iter_mut().flatten() {
                substitute_expr_types(&mut argument.value, substitutions);
            }
            substitute_expr_types(&mut chain.success, substitutions);
            substitute_expr_types(&mut chain.residual, substitutions);
        }
        Expr::Call(callee, arguments) => {
            substitute_expr_types(callee, substitutions);
            for argument in arguments {
                substitute_expr_types(&mut argument.value, substitutions);
            }
        }
        Expr::StructLiteral {
            constructor,
            fields,
        } => {
            substitute_type_expression_parameters(constructor, substitutions);
            for field in fields {
                substitute_expr_types(&mut field.value, substitutions);
            }
        }
        Expr::Member(base, _) => {
            if matches!(base.as_ref(), Expr::Call(_, _) | Expr::StructLiteral { .. }) {
                substitute_type_expression_parameters(base, substitutions);
            } else {
                substitute_expr_types(base, substitutions);
            }
        }
        Expr::ChainMember(base, _) => substitute_expr_types(base, substitutions),
        Expr::Array(elements) => {
            for element in elements {
                substitute_expr_types(element, substitutions);
            }
        }
        Expr::Index { base, index } => {
            substitute_expr_types(base, substitutions);
            substitute_expr_types(index, substitutions);
        }
        Expr::Block(statements, tail) => {
            for statement in statements {
                match statement {
                    Stmt::Let(binding) => {
                        if let Some(annotation) = &mut binding.annotation {
                            substitute_type_parameters(annotation, substitutions);
                        }
                        substitute_expr_types(&mut binding.value, substitutions);
                    }
                    Stmt::Expr(expression) => substitute_expr_types(expression, substitutions),
                }
            }
            if let Some(tail) = tail {
                substitute_expr_types(tail, substitutions);
            }
        }
        Expr::Closure(parameters, body) => {
            for parameter in parameters {
                substitute_parameter_types(parameter, substitutions);
            }
            substitute_expr_types(body, substitutions);
        }
        Expr::If {
            condition,
            then_branch,
            else_branch,
        } => {
            substitute_expr_types(condition, substitutions);
            substitute_expr_types(then_branch, substitutions);
            if let Some(else_branch) = else_branch {
                substitute_expr_types(else_branch, substitutions);
            }
        }
        Expr::Return(value) | Expr::Break(value) => {
            if let Some(value) = value {
                substitute_expr_types(value, substitutions);
            }
        }
        Expr::While { condition, body } => {
            substitute_expr_types(condition, substitutions);
            substitute_expr_types(body, substitutions);
        }
        Expr::Loop { body } => substitute_expr_types(body, substitutions),
        Expr::Match { scrutinee, arms } => {
            substitute_expr_types(scrutinee, substitutions);
            for arm in arms {
                if let Some(guard) = &mut arm.guard {
                    substitute_expr_types(guard, substitutions);
                }
                substitute_expr_types(&mut arm.body, substitutions);
            }
        }
    }
}

pub(super) fn substitute_type_expression_parameters(
    expression: &mut Expr,
    substitutions: &HashMap<String, Type>,
) {
    match expression {
        Expr::Name(name) => {
            if let Some(replacement) = substitutions.get(name) {
                *expression = source_type_expression(replacement);
            }
        }
        Expr::Call(callee, arguments) => {
            substitute_type_expression_parameters(callee, substitutions);
            for argument in arguments {
                substitute_type_expression_parameters(&mut argument.value, substitutions);
            }
        }
        Expr::StructLiteral {
            constructor,
            fields,
        } => {
            substitute_type_expression_parameters(constructor, substitutions);
            for field in fields {
                substitute_expr_types(&mut field.value, substitutions);
            }
        }
        Expr::Unit => {}
        _ => substitute_expr_types(expression, substitutions),
    }
}

pub(super) fn source_type_expression(source: &Type) -> Expr {
    match source {
        Type::Unit => Expr::Unit,
        Type::I32 => Expr::Name("i32".to_owned()),
        Type::I64 => Expr::Name("i64".to_owned()),
        Type::U32 => Expr::Name("u32".to_owned()),
        Type::U64 => Expr::Name("u64".to_owned()),
        Type::Bool => Expr::Name("bool".to_owned()),
        Type::Borrow { .. } | Type::Function { .. } => Expr::Type(source.clone()),
        Type::Array(element, length) => Expr::Call(
            Box::new(Expr::Name("Array".to_owned())),
            vec![
                CallArg {
                    label: None,
                    value: source_type_expression(element),
                },
                CallArg {
                    label: None,
                    value: Expr::Integer(i128::from(*length)),
                },
            ],
        ),
        Type::Named(name, arguments) if arguments.is_empty() => Expr::Name(name.clone()),
        Type::Named(name, arguments) => Expr::Call(
            Box::new(Expr::Name(name.clone())),
            arguments
                .iter()
                .map(|argument| CallArg {
                    label: None,
                    value: source_type_expression(argument),
                })
                .collect(),
        ),
        Type::NamedArgs(name, arguments) => Expr::Call(
            Box::new(Expr::Name(name.clone())),
            arguments
                .iter()
                .map(|argument| CallArg {
                    label: argument.label.clone(),
                    value: source_type_expression(&argument.ty),
                })
                .collect(),
        ),
    }
}

pub(super) fn substitute_type_parameters(ty: &mut Type, substitutions: &HashMap<String, Type>) {
    match ty {
        Type::Named(name, arguments) if arguments.is_empty() => {
            if let Some(replacement) = substitutions.get(name) {
                *ty = replacement.clone();
            }
        }
        Type::Named(name, arguments) if substitutions.contains_key(name) => {
            for argument in arguments.iter_mut() {
                substitute_type_parameters(argument, substitutions);
            }
            if let Some(Type::Named(replacement, replacement_arguments)) = substitutions.get(name) {
                let mut applied = replacement_arguments.clone();
                applied.extend(arguments.clone());
                *ty = Type::Named(replacement.clone(), applied);
            }
        }
        Type::Borrow {
            mutable,
            access,
            pointee,
            ..
        } => {
            if let Some(name) = access.as_deref() {
                if let Some(selected) = substituted_access_mutability(name, substitutions) {
                    *mutable = selected;
                    *access = None;
                }
            }
            substitute_type_parameters(pointee, substitutions)
        }
        Type::Array(element, _) => substitute_type_parameters(element, substitutions),
        Type::Function {
            groups,
            effects,
            result,
        } => {
            let had_throws = effects.throws.is_some();
            for ty in groups.iter_mut().flatten() {
                substitute_type_parameters(ty, substitutions);
            }
            if let Some(error) = &mut effects.throws {
                substitute_type_parameters(error, substitutions);
            }
            for effect in &mut effects.custom {
                substitute_type_parameters(effect, substitutions);
            }
            substitute_type_parameters(result, substitutions);
            let mut remaining = Vec::new();
            for parameter in effects.parameters.drain(..) {
                match substituted_effect_row(&parameter, substitutions) {
                    Some((unsafe_effect, throws_error, custom))
                        if throws_error.as_ref().is_none_or(|selected| {
                            effects
                                .throws
                                .as_deref()
                                .is_none_or(|fixed| fixed == selected)
                        }) =>
                    {
                        effects.unsafe_effect |= unsafe_effect;
                        if effects.throws.is_none() {
                            effects.throws = throws_error.map(Box::new);
                        }
                        effects.custom.extend(effect_identity_sources(&custom));
                    }
                    Some(_) | None => remaining.push(parameter),
                }
            }
            effects.parameters = remaining;
            let mut seen_custom = HashSet::new();
            effects
                .custom
                .retain(|effect| seen_custom.insert(effect.clone()));
            if !had_throws {
                if let Some(error) = effects.throws.as_deref() {
                    let logical_result = std::mem::replace(result.as_mut(), Type::Unit);
                    **result = Type::Named(
                        "core::Result".to_owned(),
                        vec![error.clone(), logical_result],
                    );
                }
            }
        }
        Type::Named(_, arguments) => {
            for argument in arguments {
                substitute_type_parameters(argument, substitutions);
            }
        }
        Type::NamedArgs(_, arguments) => {
            for argument in arguments {
                substitute_type_parameters(&mut argument.ty, substitutions);
            }
        }
        Type::I32 | Type::I64 | Type::U32 | Type::U64 | Type::Bool | Type::Unit => {}
    }
}

fn substituted_access_mutability(
    name: &str,
    substitutions: &HashMap<String, Type>,
) -> Option<bool> {
    let Type::Named(marker, arguments) = substitutions.get(name)? else {
        return None;
    };
    if !arguments.is_empty() {
        return None;
    }
    match marker.as_str() {
        ACCESS_SHARED_MARKER => Some(false),
        ACCESS_MUT_MARKER => Some(true),
        _ => None,
    }
}

fn substituted_passing_mode(name: &str, substitutions: &HashMap<String, Type>) -> Option<PassMode> {
    let Type::Named(marker, arguments) = substitutions.get(name)? else {
        return None;
    };
    if !arguments.is_empty() {
        return None;
    }
    match marker.as_str() {
        PASSING_AUTO_MARKER => Some(PassMode::Inferred),
        PASSING_COPY_MARKER => Some(PassMode::Copy),
        PASSING_MOVE_MARKER => Some(PassMode::Move),
        _ => None,
    }
}

fn substituted_effect_row(
    name: &str,
    substitutions: &HashMap<String, Type>,
) -> Option<(bool, Option<Type>, Vec<String>)> {
    effect_row_from_source(substitutions.get(name)?)
}

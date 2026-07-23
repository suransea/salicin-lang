use std::collections::{HashMap, HashSet};

use crate::ast::{CompileParam, CompileParamKind, Type};

pub(super) const ACCESS_SHARED_MARKER: &str = "$access$shared";
pub(super) const ACCESS_MUT_MARKER: &str = "$access$mut";
pub(super) const PASSING_AUTO_MARKER: &str = "$passing$auto";
pub(super) const PASSING_COPY_MARKER: &str = "$passing$copy";
pub(super) const PASSING_MOVE_MARKER: &str = "$passing$move";
pub(super) const EFFECT_PURE_MARKER: &str = "$effect$pure";
pub(super) const EFFECT_UNSAFE_MARKER: &str = "$effect$unsafe";

const EFFECT_ROW_MARKER_PREFIX: &str = "$effect$row$";
const TYPE_CONSTRUCTOR_MARKER_PREFIX: &str = "$type$constructor$";

fn effect_row_marker(unsafe_effect: bool, custom: &[String]) -> String {
    let mut custom = custom.to_vec();
    custom.sort();
    custom.dedup();
    format!(
        "{EFFECT_ROW_MARKER_PREFIX}{}|{}",
        if unsafe_effect { "unsafe" } else { "pure" },
        custom.join("|")
    )
}

pub(super) fn effect_row_from_marker(marker: &str) -> Option<(bool, Vec<String>)> {
    match marker {
        EFFECT_PURE_MARKER => return Some((false, Vec::new())),
        EFFECT_UNSAFE_MARKER => return Some((true, Vec::new())),
        _ => {}
    }
    let row = marker.strip_prefix(EFFECT_ROW_MARKER_PREFIX)?;
    let (head, tail) = row.split_once('|')?;
    let unsafe_effect = match head {
        "pure" => false,
        "unsafe" => true,
        _ => return None,
    };
    let custom = if tail.is_empty() {
        Vec::new()
    } else {
        tail.split('|').map(str::to_owned).collect()
    };
    Some((unsafe_effect, custom))
}

pub(super) fn effect_row_source(
    unsafe_effect: bool,
    throws_error: Option<Type>,
    custom_effects: &[String],
) -> Type {
    Type::Named(
        effect_row_marker(unsafe_effect, custom_effects),
        throws_error.into_iter().collect(),
    )
}

pub(super) fn effect_row_from_source(source: &Type) -> Option<(bool, Option<Type>, Vec<String>)> {
    let Type::Named(marker, arguments) = source else {
        return None;
    };
    let (unsafe_effect, custom_effects) = effect_row_from_marker(marker)?;
    let throws_error = match arguments.as_slice() {
        [] => None,
        [error] => Some(error.clone()),
        _ => return None,
    };
    Some((unsafe_effect, throws_error, custom_effects))
}

pub(super) fn is_compile_value_marker(name: &str) -> bool {
    name.starts_with(EFFECT_ROW_MARKER_PREFIX)
        || name.starts_with(TYPE_CONSTRUCTOR_MARKER_PREFIX)
        || matches!(
            name,
            ACCESS_SHARED_MARKER
                | ACCESS_MUT_MARKER
                | PASSING_AUTO_MARKER
                | PASSING_COPY_MARKER
                | PASSING_MOVE_MARKER
                | EFFECT_PURE_MARKER
                | EFFECT_UNSAFE_MARKER
        )
}

pub(super) fn type_constructor_marker(name: &str) -> String {
    format!("{TYPE_CONSTRUCTOR_MARKER_PREFIX}{name}")
}

pub(super) fn type_constructor_from_marker(marker: &str) -> Option<String> {
    marker
        .strip_prefix(TYPE_CONSTRUCTOR_MARKER_PREFIX)
        .map(ToOwned::to_owned)
}

pub(super) fn source_effect_identity(effect: &Type) -> String {
    match effect {
        Type::I32 => "i32".to_owned(),
        Type::I64 => "i64".to_owned(),
        Type::U32 => "u32".to_owned(),
        Type::U64 => "u64".to_owned(),
        Type::Bool => "bool".to_owned(),
        Type::Unit => "()".to_owned(),
        Type::Named(name, arguments) if arguments.is_empty() => name.clone(),
        Type::Named(name, arguments) => format!(
            "{name}({})",
            arguments
                .iter()
                .map(source_effect_identity)
                .collect::<Vec<_>>()
                .join(", ")
        ),
        Type::NamedArgs(name, arguments) => format!(
            "{name}({})",
            arguments
                .iter()
                .map(|argument| {
                    let rendered = source_effect_identity(&argument.ty);
                    match &argument.label {
                        Some(label) => format!("{label}: {rendered}"),
                        None => rendered,
                    }
                })
                .collect::<Vec<_>>()
                .join(", ")
        ),
        Type::Borrow { .. } | Type::Array(_, _) | Type::Function { .. } => {
            format!("{effect:?}")
        }
    }
}

pub(super) fn source_effect_identities(effects: &[Type]) -> Vec<String> {
    let mut identities = effects
        .iter()
        .map(source_effect_identity)
        .collect::<Vec<_>>();
    identities.sort();
    identities.dedup();
    identities
}

pub(super) fn source_effect_source_map(effects: &[Type]) -> HashMap<String, Type> {
    effects
        .iter()
        .map(|effect| (source_effect_identity(effect), effect.clone()))
        .collect()
}

pub(super) fn source_type_mentions_any_name(source: &Type, names: &HashSet<String>) -> bool {
    match source {
        Type::Named(name, arguments) => {
            (arguments.is_empty() && names.contains(name))
                || arguments
                    .iter()
                    .any(|argument| source_type_mentions_any_name(argument, names))
        }
        Type::NamedArgs(name, arguments) => {
            (arguments.is_empty() && names.contains(name))
                || arguments
                    .iter()
                    .any(|argument| source_type_mentions_any_name(&argument.ty, names))
        }
        Type::Borrow { pointee, .. } => source_type_mentions_any_name(pointee, names),
        Type::Array(element, _) => source_type_mentions_any_name(element, names),
        Type::Function {
            groups,
            effects,
            result,
        } => {
            groups
                .iter()
                .flatten()
                .any(|argument| source_type_mentions_any_name(argument, names))
                || effects
                    .throws
                    .as_deref()
                    .is_some_and(|error| source_type_mentions_any_name(error, names))
                || effects
                    .custom
                    .iter()
                    .any(|effect| source_type_mentions_any_name(effect, names))
                || source_type_mentions_any_name(result, names)
        }
        Type::I32 | Type::I64 | Type::U32 | Type::U64 | Type::Bool | Type::Unit => false,
    }
}

pub(super) fn source_type_from_identity(identity: &str) -> Option<Type> {
    match identity {
        "i32" => return Some(Type::I32),
        "i64" => return Some(Type::I64),
        "u32" => return Some(Type::U32),
        "u64" => return Some(Type::U64),
        "bool" => return Some(Type::Bool),
        "()" => return Some(Type::Unit),
        _ => {}
    }
    let Some(open) = top_level_call_open(identity) else {
        return Some(Type::Named(identity.to_owned(), Vec::new()));
    };
    let name = identity[..open].to_owned();
    let inner = &identity[open + 1..identity.len() - 1];
    let arguments = split_top_level_arguments(inner)?
        .into_iter()
        .map(source_type_from_identity)
        .collect::<Option<Vec<_>>>()?;
    Some(Type::Named(name, arguments))
}

fn top_level_call_open(identity: &str) -> Option<usize> {
    if !identity.ends_with(')') {
        return None;
    }
    let mut depth = 0usize;
    let mut open = None;
    for (index, character) in identity.char_indices() {
        match character {
            '(' => {
                if depth == 0 {
                    open = Some(index);
                }
                depth += 1;
            }
            ')' => {
                depth = depth.checked_sub(1)?;
                if depth == 0 && index + character.len_utf8() != identity.len() {
                    return None;
                }
            }
            _ => {}
        }
    }
    (depth == 0).then_some(open?)
}

fn split_top_level_arguments(arguments: &str) -> Option<Vec<&str>> {
    if arguments.trim().is_empty() {
        return Some(Vec::new());
    }
    let mut result = Vec::new();
    let mut depth = 0usize;
    let mut start = 0usize;
    for (index, character) in arguments.char_indices() {
        match character {
            '(' => depth += 1,
            ')' => depth = depth.checked_sub(1)?,
            ',' if depth == 0 => {
                result.push(arguments[start..index].trim());
                start = index + character.len_utf8();
            }
            _ => {}
        }
    }
    if depth != 0 {
        return None;
    }
    result.push(arguments[start..].trim());
    Some(result)
}

pub(super) fn effect_identity_sources(effects: &[String]) -> Vec<Type> {
    effects
        .iter()
        .map(|effect| {
            source_type_from_identity(effect)
                .unwrap_or_else(|| Type::Named(effect.clone(), Vec::new()))
        })
        .collect()
}

pub(super) fn describe_compile_param_kind(kind: CompileParamKind) -> String {
    match kind {
        CompileParamKind::Type => "`type`".to_owned(),
        CompileParamKind::Region => "`region`".to_owned(),
        CompileParamKind::Access => "`access`".to_owned(),
        CompileParamKind::Passing => "`passing`".to_owned(),
        CompileParamKind::Effect => "`effect`".to_owned(),
        CompileParamKind::Parameters => "`parameters`".to_owned(),
        CompileParamKind::TypeConstructor { parameter_count } => {
            format!(
                "`({} type parameter{}): type`",
                parameter_count,
                if parameter_count == 1 { "" } else { "s" }
            )
        }
        CompileParamKind::EffectConstructor { parameter_count } => {
            format!(
                "`({} type parameter{}): effect`",
                parameter_count,
                if parameter_count == 1 { "" } else { "s" }
            )
        }
    }
}

pub(super) fn compile_parameter_kinds(
    groups: &[Vec<CompileParam>],
) -> HashMap<String, CompileParamKind> {
    groups
        .iter()
        .flatten()
        .map(|parameter| (parameter.name.clone(), parameter.kind))
        .collect()
}

pub(super) fn compile_parameter_groups_match(
    expected: &[Vec<CompileParam>],
    actual: &[Vec<CompileParam>],
) -> bool {
    expected.len() == actual.len()
        && expected.iter().zip(actual).all(|(expected, actual)| {
            expected.len() == actual.len()
                && expected.iter().zip(actual).all(|(expected, actual)| {
                    expected.name == actual.name && expected.kind == actual.kind
                })
        })
}

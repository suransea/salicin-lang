use std::collections::{HashMap, HashSet};

use crate::ast::{CompileParam, FunctionEffects, Sort, StaticExpr, Type, USizeConst};
use crate::static_semantics::StaticValue;

pub(super) const ACCESS_SHARED_MARKER: &str = "$access$shared";
pub(super) const ACCESS_MUT_MARKER: &str = "$access$mut";
pub(super) const PARAMETER_MODIFIER_COPY_MARKER: &str = "$parameters$modifier$copy";
pub(super) const PARAMETER_MODIFIER_MOVE_MARKER: &str = "$parameters$modifier$move";
pub(super) const USIZE_VALUE_PREFIX: &str = "$usize$value$";
pub(super) const EFFECT_PURE_MARKER: &str = "$effect$pure";
pub(super) const EFFECT_UNSAFE_MARKER: &str = "$effect$unsafe";

const EFFECT_ROW_MARKER_PREFIX: &str = "$effect$row$";
const TYPE_CONSTRUCTOR_MARKER_PREFIX: &str = "$type$constructor$";
const CLOSED_VALUE_MARKER_PREFIX: &str = "$closed$value$";

pub(super) fn access_mutability(name: &str) -> Option<bool> {
    match name {
        ACCESS_SHARED_MARKER | "shared" => Some(false),
        ACCESS_MUT_MARKER | "mut" => Some(true),
        _ => match name.rsplit([':', '.']).find(|segment| !segment.is_empty()) {
            Some("shared") => Some(false),
            Some("mut") => Some(true),
            _ => None,
        },
    }
}

pub(super) fn is_access_sort_name(name: &str) -> bool {
    name == "access" || name.ends_with("::access") || name.ends_with(".access")
}

pub(super) fn closed_value_marker(owner: &str, member: &str) -> String {
    match (owner, member) {
        ("access", "shared") => return ACCESS_SHARED_MARKER.to_owned(),
        ("access", "mut") => return ACCESS_MUT_MARKER.to_owned(),
        _ => {}
    }
    format!(
        "{CLOSED_VALUE_MARKER_PREFIX}{}:{owner}{member}",
        owner.len()
    )
}

pub(super) fn closed_value_from_marker(marker: &str) -> Option<(&str, &str)> {
    match marker {
        ACCESS_SHARED_MARKER => return Some(("access", "shared")),
        ACCESS_MUT_MARKER => return Some(("access", "mut")),
        _ => {}
    }
    let encoded = marker.strip_prefix(CLOSED_VALUE_MARKER_PREFIX)?;
    let (owner_len, value) = encoded.split_once(':')?;
    let owner_len = owner_len.parse::<usize>().ok()?;
    let owner = value.get(..owner_len)?;
    let member = value.get(owner_len..)?;
    Some((owner, member))
}

pub(super) fn closed_value_member<'a>(
    owner: &str,
    candidate: &'a str,
    members: &[String],
) -> Option<&'a str> {
    if members.iter().any(|member| member == candidate) {
        return Some(candidate);
    }
    let member = candidate.strip_prefix(owner)?.strip_prefix('.')?;
    members
        .iter()
        .any(|known| known == member)
        .then_some(member)
}

pub(super) fn usize_value_marker(value: u64) -> String {
    format!("{USIZE_VALUE_PREFIX}{value}")
}

pub(super) fn usize_value_from_marker(marker: &str) -> Option<u64> {
    marker.strip_prefix(USIZE_VALUE_PREFIX)?.parse().ok()
}

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

pub(super) fn effect_row_is_singleton(effects: &FunctionEffects) -> bool {
    effects.parameters.is_empty()
        && usize::from(effects.unsafe_effect)
            + usize::from(effects.throws.is_some())
            + effects.custom.len()
            == 1
}

pub(super) fn is_compile_value_marker(name: &str) -> bool {
    name.starts_with(USIZE_VALUE_PREFIX)
        || name.starts_with(EFFECT_ROW_MARKER_PREFIX)
        || name.starts_with(TYPE_CONSTRUCTOR_MARKER_PREFIX)
        || name.starts_with(CLOSED_VALUE_MARKER_PREFIX)
        || matches!(
            name,
            ACCESS_SHARED_MARKER
                | ACCESS_MUT_MARKER
                | PARAMETER_MODIFIER_COPY_MARKER
                | PARAMETER_MODIFIER_MOVE_MARKER
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

/// Decode the temporary source-type representation used by the existing
/// monomorphizer into the typed static-value IR.  Keeping this adapter in one
/// place lets later passes migrate away from marker-shaped runtime types
/// without changing Salicin's source syntax.
pub(super) fn static_value_from_source(source: &Type, sort: &Sort) -> Option<StaticValue> {
    match sort {
        Sort::Type => Some(StaticValue::Type(source.clone())),
        Sort::USize => match source {
            Type::CompileUSize(value) => Some(StaticValue::USize(*value)),
            _ => None,
        },
        Sort::Region => match source {
            Type::Named(name, arguments) if arguments.is_empty() => {
                Some(StaticValue::Region(name.clone()))
            }
            _ => None,
        },
        Sort::String => None,
        Sort::Effect => {
            let (unsafe_effect, throws, custom) = effect_row_from_source(source)?;
            let effects = FunctionEffects {
                unsafe_effect,
                throws: throws.map(Box::new),
                custom: effect_identity_sources(&custom),
                parameters: Vec::new(),
            };
            effect_row_is_singleton(&effects).then_some(StaticValue::Effect(effects))
        }
        Sort::Effects => {
            let (unsafe_effect, throws, custom) = effect_row_from_source(source)?;
            Some(StaticValue::Effects(FunctionEffects {
                unsafe_effect,
                throws: throws.map(Box::new),
                custom: effect_identity_sources(&custom),
                parameters: Vec::new(),
            }))
        }
        Sort::TypeConstructor { .. } => match source {
            Type::Named(name, arguments) if arguments.is_empty() => {
                Some(StaticValue::TypeConstructor {
                    name: type_constructor_from_marker(name).unwrap_or_else(|| name.clone()),
                    sort: sort.clone(),
                })
            }
            _ => None,
        },
        Sort::EffectConstructor { .. } => match source {
            Type::Named(name, arguments) if arguments.is_empty() => {
                Some(StaticValue::EffectConstructor {
                    name: name.clone(),
                    sort: sort.clone(),
                })
            }
            _ => None,
        },
        Sort::Named(expected_sort) => match source {
            Type::Named(marker, arguments) if arguments.is_empty() => {
                let (actual_sort, member) = closed_value_from_marker(marker)?;
                (actual_sort == expected_sort).then(|| StaticValue::Finite {
                    sort: actual_sort.to_owned(),
                    member: member.to_owned(),
                })
            }
            _ => None,
        },
        Sort::Parameters | Sort::ParameterPack | Sort::ParameterModifier => None,
    }
}

/// Encode a typed static value for legacy monomorphization passes.
pub(super) fn source_from_static_value(value: &StaticValue) -> Option<Type> {
    match value {
        StaticValue::Type(source) => Some(source.clone()),
        StaticValue::USize(value) => Some(Type::CompileUSize(*value)),
        StaticValue::Region(name) | StaticValue::Symbolic { name, .. } => {
            Some(Type::Named(name.clone(), Vec::new()))
        }
        StaticValue::String(_) => None,
        StaticValue::Effect(effects) | StaticValue::Effects(effects)
            if effects.parameters.is_empty() =>
        {
            Some(effect_row_source(
                effects.unsafe_effect,
                effects.throws.as_deref().cloned(),
                &source_effect_identities(&effects.custom),
            ))
        }
        StaticValue::Effect(_) | StaticValue::Effects(_) | StaticValue::ParameterSchema(_) => None,
        StaticValue::TypeConstructor { name, .. } | StaticValue::EffectConstructor { name, .. } => {
            Some(Type::Named(name.clone(), Vec::new()))
        }
        StaticValue::Finite { sort, member } => {
            Some(Type::Named(closed_value_marker(sort, member), Vec::new()))
        }
    }
}

pub(super) fn source_effect_identity(effect: &Type) -> String {
    match effect {
        Type::I8 => "i8".to_owned(),
        Type::I16 => "i16".to_owned(),
        Type::I32 => "i32".to_owned(),
        Type::I64 => "i64".to_owned(),
        Type::I128 => "i128".to_owned(),
        Type::ISize => "isize".to_owned(),
        Type::U8 => "u8".to_owned(),
        Type::U16 => "u16".to_owned(),
        Type::U32 => "u32".to_owned(),
        Type::U64 => "u64".to_owned(),
        Type::U128 => "u128".to_owned(),
        Type::USize => "usize".to_owned(),
        Type::Bool => "bool".to_owned(),
        Type::Unit => "()".to_owned(),
        Type::Tuple(fields) => {
            let mut rendered = fields
                .iter()
                .map(source_effect_identity)
                .collect::<Vec<_>>()
                .join(", ");
            if fields.len() == 1 {
                rendered.push(',');
            }
            format!("({rendered})")
        }
        Type::CompileUSize(value) => value.to_string(),
        Type::ArrayApplication {
            constructor,
            element,
            length,
        } => format!(
            "{constructor}({})({})",
            source_effect_identity(element),
            match length {
                USizeConst::Literal(value) => value.to_string(),
                USizeConst::Parameter(name) => name.clone(),
                USizeConst::Expression(expression) => render_static_expression(expression),
            }
        ),
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

pub(super) fn render_static_expression(expression: &StaticExpr) -> String {
    match expression {
        StaticExpr::USize(value) => value.to_string(),
        StaticExpr::Bool(value) => value.to_string(),
        StaticExpr::Name(name) => name.clone(),
        StaticExpr::Unary(operator, operand) => {
            let operator = match operator {
                crate::ast::UnaryOp::Neg => "-",
                crate::ast::UnaryOp::Not => "!",
                crate::ast::UnaryOp::Deref => "*",
            };
            format!("{operator}{}", render_static_expression(operand))
        }
        StaticExpr::Binary(left, operator, right) => format!(
            "({} {:?} {})",
            render_static_expression(left),
            operator,
            render_static_expression(right)
        ),
        StaticExpr::Call { function, groups } => {
            groups.iter().fold(function.clone(), |rendered, group| {
                let arguments = group
                    .iter()
                    .map(|argument| {
                        let value = render_static_expression(&argument.value);
                        argument
                            .label
                            .as_ref()
                            .map_or(value.clone(), |label| format!("{label}: {value}"))
                    })
                    .collect::<Vec<_>>()
                    .join(", ");
                format!("{rendered}({arguments})")
            })
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
        Type::Tuple(fields) => fields
            .iter()
            .any(|field| source_type_mentions_any_name(field, names)),
        Type::Array(element, _) => source_type_mentions_any_name(element, names),
        Type::ArrayApplication {
            element, length, ..
        } => {
            source_type_mentions_any_name(element, names)
                || match length {
                    USizeConst::Parameter(name) => names.contains(name),
                    USizeConst::Expression(expression) => {
                        static_expression_mentions_any_name(expression, names)
                    }
                    USizeConst::Literal(_) => false,
                }
        }
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
        | Type::CompileUSize(_) => false,
    }
}

fn static_expression_mentions_any_name(expression: &StaticExpr, names: &HashSet<String>) -> bool {
    match expression {
        StaticExpr::Name(name) => names.contains(name),
        StaticExpr::Unary(_, operand) => static_expression_mentions_any_name(operand, names),
        StaticExpr::Binary(left, _, right) => {
            static_expression_mentions_any_name(left, names)
                || static_expression_mentions_any_name(right, names)
        }
        StaticExpr::Call { groups, .. } => groups
            .iter()
            .flatten()
            .any(|argument| static_expression_mentions_any_name(&argument.value, names)),
        StaticExpr::USize(_) | StaticExpr::Bool(_) => false,
    }
}

pub(super) fn source_type_from_identity(identity: &str) -> Option<Type> {
    match identity {
        "i8" => return Some(Type::I8),
        "i16" => return Some(Type::I16),
        "i32" => return Some(Type::I32),
        "i64" => return Some(Type::I64),
        "i128" => return Some(Type::I128),
        "isize" => return Some(Type::ISize),
        "u8" => return Some(Type::U8),
        "u16" => return Some(Type::U16),
        "u32" => return Some(Type::U32),
        "u64" => return Some(Type::U64),
        "u128" => return Some(Type::U128),
        "usize" => return Some(Type::USize),
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

fn render_sort(sort: &Sort) -> String {
    match sort {
        Sort::Type => "type".to_owned(),
        Sort::USize => "usize".to_owned(),
        Sort::Region => "region".to_owned(),
        Sort::String => "string".to_owned(),
        Sort::Effect => "effect".to_owned(),
        Sort::Effects => "effects".to_owned(),
        Sort::Parameters => "parameters".to_owned(),
        Sort::ParameterPack => "...parameters".to_owned(),
        Sort::ParameterModifier => "(P: parameters): parameters".to_owned(),
        Sort::TypeConstructor { parameter_groups } => {
            render_constructor_sort(parameter_groups, "type")
        }
        Sort::EffectConstructor { parameter_groups } => {
            render_constructor_sort(parameter_groups, "effect")
        }
        Sort::Named(name) => name.clone(),
    }
}

fn render_constructor_sort(parameter_groups: &[Vec<Sort>], result: &str) -> String {
    let parameters = parameter_groups
        .iter()
        .map(|group| {
            format!(
                "({})",
                group.iter().map(render_sort).collect::<Vec<_>>().join(", ")
            )
        })
        .collect::<String>();
    format!("{parameters}: {result}")
}

pub(super) fn describe_compile_sort(sort: Sort) -> String {
    format!("`{}`", render_sort(&sort))
}

pub(super) fn compile_parameter_sorts(groups: &[Vec<CompileParam>]) -> HashMap<String, Sort> {
    groups
        .iter()
        .flatten()
        .map(|parameter| (parameter.name.clone(), parameter.kind.clone()))
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn legacy_static_value_adapter_round_trips_typed_values() {
        for value in [
            StaticValue::USize(12),
            StaticValue::Finite {
                sort: "optimization".into(),
                member: "speed".into(),
            },
            StaticValue::Effect(FunctionEffects {
                unsafe_effect: true,
                throws: None,
                custom: Vec::new(),
                parameters: Vec::new(),
            }),
            StaticValue::Effects(FunctionEffects {
                unsafe_effect: true,
                throws: Some(Box::new(Type::I32)),
                custom: vec![Type::Named("Log".into(), Vec::new())],
                parameters: Vec::new(),
            }),
        ] {
            let sort = value.sort();
            let source = source_from_static_value(&value).expect("value must have a legacy form");
            assert_eq!(
                static_value_from_source(&source, &sort),
                Some(value),
                "round trip through {source:?}"
            );
        }
    }

    #[test]
    fn singular_effect_sort_rejects_empty_and_multi_identity_rows() {
        let pure = effect_row_source(false, None, &[]);
        let unsafe_only = effect_row_source(true, None, &[]);
        let multiple = effect_row_source(true, Some(Type::I32), &["Audit".into()]);

        assert!(static_value_from_source(&pure, &Sort::Effect).is_none());
        assert!(matches!(
            static_value_from_source(&unsafe_only, &Sort::Effect),
            Some(StaticValue::Effect(_))
        ));
        assert!(static_value_from_source(&multiple, &Sort::Effect).is_none());
        assert!(matches!(
            static_value_from_source(&pure, &Sort::Effects),
            Some(StaticValue::Effects(_))
        ));
        assert!(matches!(
            static_value_from_source(&multiple, &Sort::Effects),
            Some(StaticValue::Effects(_))
        ));
    }
}

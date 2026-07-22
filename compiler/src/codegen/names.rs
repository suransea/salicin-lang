use std::fmt;

use crate::ast::PassMode;

use super::{
    CallableKind, ConstructorTraitImplKey, FunctionInstanceKey, NominalInstanceKey, NominalKind,
    TraitImplKey, Ty,
};

pub(super) fn function_instance_name(key: &FunctionInstanceKey) -> String {
    let mut canonical = String::from("$mono$fn$");
    push_canonical_component(&mut canonical, &key.template);
    canonical.push_str(&key.arguments.len().to_string());
    canonical.push(':');
    for argument in &key.arguments {
        let encoded = canonical_type_encoding(argument);
        push_canonical_component(&mut canonical, &encoded);
    }
    canonical
}

pub(super) fn nominal_instance_name(key: &NominalInstanceKey) -> String {
    let mut canonical = String::from("$mono$type$");
    push_canonical_component(
        &mut canonical,
        match key.kind {
            NominalKind::Struct => "struct",
            NominalKind::Enum => "enum",
        },
    );
    push_canonical_component(&mut canonical, &key.template);
    canonical.push_str(&key.arguments.len().to_string());
    canonical.push(':');
    for argument in &key.arguments {
        let encoded = canonical_type_encoding(argument);
        push_canonical_component(&mut canonical, &encoded);
    }
    canonical
}

pub(super) fn generic_validation_name(template: &str) -> String {
    let mut name = String::from("$generic$check$");
    push_canonical_component(&mut name, template);
    name
}

pub(super) fn generic_parameter_marker(template: &str, index: usize, parameter: &str) -> String {
    let mut name = String::from("$generic$param$");
    push_canonical_component(&mut name, template);
    name.push_str(&index.to_string());
    name.push(':');
    push_canonical_component(&mut name, parameter);
    name
}

fn push_canonical_component(output: &mut String, component: &str) {
    output.push_str(&component.len().to_string());
    output.push(':');
    output.push_str(component);
}

pub(super) fn canonical_type_encoding(ty: &Ty) -> String {
    match ty {
        Ty::I32 => "i32".to_owned(),
        Ty::I64 => "i64".to_owned(),
        Ty::U32 => "u32".to_owned(),
        Ty::U64 => "u64".to_owned(),
        Ty::Bool => "bool".to_owned(),
        Ty::Unit => "unit".to_owned(),
        Ty::Pointer { pointee, mutable } => {
            let mut encoded = if *mutable { "mutptr" } else { "ptr" }.to_owned();
            push_canonical_component(&mut encoded, &canonical_type_encoding(pointee));
            encoded
        }
        Ty::Reference {
            pointee,
            mutable,
            region,
        } => {
            let mut encoded = if *mutable { "mutref" } else { "ref" }.to_owned();
            if let Some(region) = region {
                push_canonical_component(&mut encoded, region);
            } else {
                encoded.push_str("0:");
            }
            push_canonical_component(&mut encoded, &canonical_type_encoding(pointee));
            encoded
        }
        Ty::Array(element, length) => {
            let element = canonical_type_encoding(element);
            let mut encoded = format!("array{length}:");
            push_canonical_component(&mut encoded, &element);
            encoded
        }
        Ty::Struct(name) => {
            let mut encoded = String::from("struct");
            push_canonical_component(&mut encoded, name);
            encoded
        }
        Ty::Enum(name) => {
            let mut encoded = String::from("enum");
            push_canonical_component(&mut encoded, name);
            encoded
        }
        Ty::Never => "Never".to_owned(),
        Ty::Function(function) => {
            let mut encoded = String::from("function");
            encoded.push_str(&function.groups.len().to_string());
            encoded.push(':');
            for group in &function.groups {
                encoded.push_str(&group.len().to_string());
                encoded.push(':');
                for parameter in group {
                    push_canonical_component(&mut encoded, &canonical_type_encoding(parameter));
                }
            }
            encoded.push_str(if function.unsafe_effect { "u:" } else { "p:" });
            match &function.throws_error {
                Some(error) => {
                    encoded.push_str("t:");
                    push_canonical_component(&mut encoded, &canonical_type_encoding(error));
                }
                None => encoded.push_str("n:"),
            }
            encoded.push_str(&function.custom_effects.len().to_string());
            encoded.push(':');
            for effect in &function.custom_effects {
                push_canonical_component(&mut encoded, effect);
            }
            push_canonical_component(&mut encoded, &canonical_type_encoding(&function.result));
            encoded
        }
        Ty::Callable(callable) => {
            let mut encoded = String::from("callable");
            match &callable.kind {
                CallableKind::Partial {
                    function,
                    consumed_groups,
                    is_fn_once,
                } => {
                    encoded.push('p');
                    push_canonical_component(&mut encoded, function);
                    encoded.push_str(&format!("{consumed_groups}:{}:", u8::from(*is_fn_once)));
                }
                CallableKind::Closure {
                    function,
                    is_fn_mut,
                    is_fn_once,
                } => {
                    encoded.push('c');
                    push_canonical_component(&mut encoded, function);
                    encoded.push_str(&format!(
                        "{}:{}:",
                        u8::from(*is_fn_mut),
                        u8::from(*is_fn_once)
                    ));
                }
            }
            for capture in &callable.captures {
                encoded.push_str(match capture.mode {
                    PassMode::Inferred => "i",
                    PassMode::Copy => "c",
                    PassMode::Move => "m",
                    PassMode::Borrow => "b",
                    PassMode::MutBorrow => "u",
                });
                push_canonical_component(&mut encoded, &canonical_type_encoding(&capture.ty));
            }
            push_canonical_component(
                &mut encoded,
                &canonical_type_encoding(&Ty::Function(callable.signature.clone())),
            );
            encoded
        }
        Ty::Continuation { input, output } => {
            let mut encoded = String::from("continuation");
            push_canonical_component(&mut encoded, &canonical_type_encoding(input));
            push_canonical_component(&mut encoded, &canonical_type_encoding(output));
            encoded
        }
        Ty::EffectCallable {
            input,
            output,
            answer,
        } => {
            let mut encoded = String::from("effect-callable");
            push_canonical_component(&mut encoded, &canonical_type_encoding(input));
            push_canonical_component(&mut encoded, &canonical_type_encoding(output));
            push_canonical_component(&mut encoded, &canonical_type_encoding(answer));
            encoded
        }
        Ty::EffectRow {
            unsafe_effect,
            throws_error,
            custom_effects,
        } => {
            let mut encoded = if *unsafe_effect {
                "effect-row-u".to_owned()
            } else {
                "effect-row-p".to_owned()
            };
            match throws_error {
                Some(error) => {
                    encoded.push_str("t:");
                    push_canonical_component(&mut encoded, &canonical_type_encoding(error));
                }
                None => encoded.push_str("n:"),
            }
            encoded.push_str(&custom_effects.len().to_string());
            encoded.push(':');
            for effect in custom_effects {
                push_canonical_component(&mut encoded, effect);
            }
            encoded
        }
        Ty::Error => "error".to_owned(),
    }
}

pub(super) fn inherent_method_name(target: &str, member: &str) -> String {
    format!("{target}::method::{member}")
}

pub(super) fn associated_function_name(target: &str, member: &str) -> String {
    format!("{target}::function::{member}")
}

pub(super) fn generic_inherent_function_name(target: &str, member: &str) -> String {
    format!("$generic$inherent${target}::function::{member}")
}

pub(super) fn associated_constant_name(target: &str, member: &str) -> String {
    format!("{target}::constant::{member}")
}

pub(super) fn trait_method_name(key: &TraitImplKey, member: &str) -> String {
    let mut canonical = String::from("$trait$impl$");
    push_canonical_component(&mut canonical, &key.trait_ref.name);
    push_canonical_component(&mut canonical, &canonical_type_encoding(&key.self_ty));
    canonical.push_str(&key.trait_ref.arguments.len().to_string());
    canonical.push(':');
    for argument in &key.trait_ref.arguments {
        push_canonical_component(&mut canonical, &canonical_type_encoding(argument));
    }
    push_canonical_component(&mut canonical, member);
    canonical
}

pub(super) fn constructor_trait_method_name(key: &ConstructorTraitImplKey, member: &str) -> String {
    let mut canonical = String::from("$trait$constructor$impl$");
    push_canonical_component(&mut canonical, &key.trait_ref.name);
    push_canonical_component(&mut canonical, &key.target.name);
    canonical.push_str(&key.target.parameter_count.to_string());
    canonical.push(':');
    canonical.push_str(&key.trait_ref.arguments.len().to_string());
    canonical.push(':');
    for argument in &key.trait_ref.arguments {
        push_canonical_component(&mut canonical, &canonical_type_encoding(argument));
    }
    push_canonical_component(&mut canonical, member);
    canonical
}

pub(super) fn assumed_trait_method_name(
    function: &str,
    key: &TraitImplKey,
    member: &str,
) -> String {
    let mut canonical = String::from("$generic$bound$");
    push_canonical_component(&mut canonical, function);
    push_canonical_component(&mut canonical, &trait_method_name(key, member));
    canonical
}

pub(super) fn function_symbol(name: &str) -> String {
    format!("sali.fn.{}", hex_name(name))
}

pub(super) fn drop_glue_symbol(ty: &Ty) -> String {
    format!("sali.drop.{}", hex_name(&canonical_type_encoding(ty)))
}

pub(super) fn global_symbol(name: &str) -> String {
    format!("sali.global.{}", hex_name(name))
}

pub(super) fn type_symbol(name: &str) -> String {
    format!("sali.type.{}", hex_name(name))
}

pub(super) fn hex_name(name: &str) -> String {
    let mut output = String::with_capacity(name.len() * 2);
    for byte in name.as_bytes() {
        use fmt::Write;
        write!(&mut output, "{byte:02x}").expect("writing to String cannot fail");
    }
    output
}

pub(super) fn llvm_comment(name: &str) -> String {
    name.replace(['\n', '\r'], " ")
}

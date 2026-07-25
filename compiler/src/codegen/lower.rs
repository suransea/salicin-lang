use crate::ast::{CallArg, Expr, PassMode, Type, UnaryOp};
use crate::core::LangItemKind;

use super::hir::{
    CallableCaptureTy, CallableKind, CallableTy, ClosureCapture, ClosureCaptureMode,
    ClosureCaptureUse, ClosureInfo, FunctionTy, HirArgument, HirExpr, HirExprKind, HirPlace,
    LocalCapability, ParamSig, PartialInfo, Ty,
};
use super::Analyzer;

#[derive(Debug, Clone)]
pub(super) struct InferredTypeArgument {
    pub(super) ty: Ty,
    pub(super) source: Option<Type>,
    pub(super) origin: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) enum TypeProbe {
    Known(Ty),
    KnownSource(Ty, Type),
    Defaultable(Ty),
    Unsupported,
}

impl Analyzer {
    pub(super) fn nominal_ty_from_probe(probe: &TypeProbe) -> Option<Ty> {
        match probe {
            TypeProbe::Known(ty) | TypeProbe::KnownSource(ty, _)
                if matches!(ty, Ty::Struct(_) | Ty::Enum(_) | Ty::Bool) || ty.is_integer() =>
            {
                Some(ty.clone())
            }
            TypeProbe::Known(_)
            | TypeProbe::KnownSource(_, _)
            | TypeProbe::Defaultable(_)
            | TypeProbe::Unsupported => None,
        }
    }

    pub(super) fn probe_matches_type(probe: &TypeProbe, expected: &Ty) -> bool {
        match probe {
            TypeProbe::Known(actual) | TypeProbe::KnownSource(actual, _) => actual == expected,
            TypeProbe::Defaultable(default) => default.is_integer() && expected.is_integer(),
            TypeProbe::Unsupported => true,
        }
    }
}

#[derive(Debug, Clone)]
pub(super) struct BinaryOperatorCandidate {
    pub(super) method: String,
    pub(super) rhs: Ty,
    pub(super) output: Ty,
}

#[derive(Debug, Clone)]
pub(super) struct CustomChainPlan {
    pub(super) item_source: Type,
    pub(super) output_source: Type,
    pub(super) result_ty: Ty,
    pub(super) result_source: Type,
}

#[derive(Clone, Copy)]
pub(super) enum BoundMethodConstraint<'a> {
    None,
    Nominal(&'a str),
    LangItem(LangItemKind),
}

pub(super) fn partial_callable_ty(
    function: String,
    consumed_groups: usize,
    signature: FunctionTy,
    arguments: &[HirArgument],
) -> Ty {
    let captures = arguments
        .iter()
        .map(|argument| match argument {
            HirArgument::Copy(value) => CallableCaptureTy {
                ty: value.ty.clone(),
                mode: PassMode::Copy,
            },
            HirArgument::Move(value) => CallableCaptureTy {
                ty: value.ty.clone(),
                mode: PassMode::Move,
            },
            HirArgument::SharedBorrow(place) => CallableCaptureTy {
                ty: place.ty.clone(),
                mode: PassMode::Borrow,
            },
            HirArgument::MutBorrow(place) => CallableCaptureTy {
                ty: place.ty.clone(),
                mode: PassMode::MutBorrow,
            },
            HirArgument::CallableCaptureBorrow {
                capture_ty,
                mutable,
                ..
            } => CallableCaptureTy {
                ty: capture_ty.clone(),
                mode: if *mutable {
                    PassMode::MutBorrow
                } else {
                    PassMode::Borrow
                },
            },
        })
        .collect::<Vec<_>>();
    let is_fn_once = captures
        .iter()
        .any(|capture| capture.mode == PassMode::Move);
    let is_fn_mut = !is_fn_once
        && captures
            .iter()
            .any(|capture| capture.mode == PassMode::MutBorrow);
    Ty::Callable(CallableTy {
        signature,
        captures,
        kind: CallableKind::Partial {
            function,
            consumed_groups,
            is_fn_mut,
            is_fn_once,
        },
    })
}

pub(super) fn partial_info_for_callable(ty: &Ty) -> Option<PartialInfo> {
    let Ty::Callable(callable) = ty else {
        return None;
    };
    let CallableKind::Partial {
        function,
        consumed_groups,
        is_fn_mut,
        is_fn_once,
    } = &callable.kind
    else {
        return None;
    };
    Some(PartialInfo {
        function: function.clone(),
        consumed_groups: *consumed_groups,
        capture_count: callable.captures.len(),
        is_fn_mut: *is_fn_mut,
        is_fn_once: *is_fn_once,
    })
}

pub(super) fn closure_info_for_callable(ty: &Ty) -> Option<ClosureInfo> {
    let Ty::Callable(callable) = ty else {
        return None;
    };
    let CallableKind::Closure {
        function,
        is_fn_mut,
        is_fn_once,
    } = &callable.kind
    else {
        return None;
    };
    let captures = callable
        .captures
        .iter()
        .map(|capture| ClosureCapture {
            place: HirPlace {
                local: usize::MAX,
                root_ty: capture.ty.clone(),
                projections: Vec::new(),
                dynamic_index: None,
                ty: capture.ty.clone(),
                capability: LocalCapability::Owned,
                root_mutable: false,
                loan: None,
                indirect: false,
            },
            mode: match capture.mode {
                PassMode::Borrow => ClosureCaptureMode::Shared,
                PassMode::MutBorrow => ClosureCaptureMode::Mutable,
                PassMode::Move | PassMode::Copy | PassMode::Inferred => ClosureCaptureMode::Move,
            },
            value: None,
            forwarded: None,
        })
        .collect();
    let groups = callable
        .signature
        .groups
        .iter()
        .enumerate()
        .map(|(group_index, group)| {
            group
                .iter()
                .enumerate()
                .map(|(parameter_index, ty)| ParamSig {
                    name: format!("arg{group_index}_{parameter_index}"),
                    ty: ty.clone(),
                    mode: PassMode::Inferred,
                })
                .collect()
        })
        .collect();
    Some(ClosureInfo {
        function: function.clone(),
        groups,
        unsafe_effect: callable.signature.unsafe_effect,
        throws_error: callable.signature.throws_error.as_deref().cloned(),
        custom_effects: callable.signature.custom_effects.clone(),
        result: (*callable.signature.result).clone(),
        captures,
        capture_names: Vec::new(),
        is_fn_mut: *is_fn_mut,
        is_fn_once: *is_fn_once,
    })
}

pub(super) fn error_expr() -> HirExpr {
    HirExpr {
        ty: Ty::Error,
        kind: HirExprKind::Unit,
    }
}

pub(super) fn display_region(region: Option<&str>) -> String {
    region.map_or_else(|| "an inferred region".to_owned(), display_region_argument)
}

pub(super) fn display_region_argument(region: &str) -> String {
    if region.chars().next().is_some_and(char::is_uppercase) {
        region.to_owned()
    } else {
        format!("'{region}")
    }
}

pub(super) fn contextual_reference_result(result: &Ty, expected: Option<&Ty>) -> Ty {
    match (result, expected) {
        (
            Ty::Reference {
                pointee,
                mutable,
                region: None,
            },
            Some(
                expected @ Ty::Reference {
                    pointee: expected_pointee,
                    mutable: expected_mutable,
                    ..
                },
            ),
        ) if pointee == expected_pointee && mutable == expected_mutable => expected.clone(),
        _ => result.clone(),
    }
}

pub(super) fn reference_value_types_compatible(actual: &Ty, expected: &Ty) -> bool {
    matches!(
        (actual, expected),
        (
            Ty::Reference {
                pointee: actual_pointee,
                mutable: actual_mutable,
                ..
            },
            Ty::Reference {
                pointee: expected_pointee,
                mutable: expected_mutable,
                ..
            }
        ) if actual_pointee == expected_pointee && actual_mutable == expected_mutable
    )
}

pub(super) fn flatten_call<'a>(expression: &'a Expr, groups: &mut Vec<&'a [CallArg]>) -> &'a Expr {
    match expression.unlocated() {
        Expr::Call(callee, arguments) => {
            let root = flatten_call(callee, groups);
            groups.push(arguments);
            root
        }
        expression => expression,
    }
}

pub(super) fn place_root_name(expression: &Expr) -> Option<&str> {
    match expression.unlocated() {
        Expr::Name(name) => Some(name),
        Expr::Member(base, _) | Expr::ChainMember(base, _) | Expr::Index { base, .. } => {
            place_root_name(base)
        }
        _ => None,
    }
}

pub(super) fn record_closure_capture(
    captures: &mut Vec<ClosureCaptureUse>,
    name: &str,
    mode: ClosureCaptureMode,
) {
    if let Some(capture) = captures.iter_mut().find(|capture| capture.name == name) {
        match mode {
            ClosureCaptureMode::Move => capture.mode = mode,
            ClosureCaptureMode::Mutable if capture.mode == ClosureCaptureMode::Shared => {
                capture.mode = mode;
            }
            ClosureCaptureMode::Shared | ClosureCaptureMode::Mutable => {}
        }
    } else {
        captures.push(ClosureCaptureUse {
            name: name.to_owned(),
            mode,
        });
    }
}

pub(super) fn integer_fits(value: u128, ty: &Ty) -> bool {
    match ty {
        Ty::I8 => value <= i8::MAX as u128,
        Ty::I16 => value <= i16::MAX as u128,
        Ty::I32 => value <= i32::MAX as u128,
        Ty::I64 => value <= i64::MAX as u128,
        Ty::I128 => value <= i128::MAX as u128,
        Ty::ISize => value <= isize::MAX as u128,
        Ty::U8 => u8::try_from(value).is_ok(),
        Ty::U16 => u16::try_from(value).is_ok(),
        Ty::U32 => value <= u32::MAX as u128,
        Ty::U64 => value <= u64::MAX as u128,
        Ty::USize => value <= usize::MAX as u128,
        Ty::U128 => true,
        _ => false,
    }
}

pub(super) fn negative_integer_fits(magnitude: u128, ty: &Ty) -> bool {
    match ty {
        Ty::I8 => magnitude <= (i8::MAX as u128) + 1,
        Ty::I16 => magnitude <= (i16::MAX as u128) + 1,
        Ty::I32 => magnitude <= (i32::MAX as u128) + 1,
        Ty::I64 => magnitude <= (i64::MAX as u128) + 1,
        Ty::ISize => magnitude <= (isize::MAX as u128) + 1,
        Ty::I128 => magnitude <= (i128::MAX as u128) + 1,
        _ => false,
    }
}

pub(super) fn integer_literal_bits(value: u128) -> i128 {
    value as i128
}

pub(super) fn integer_value_fits(value: i128, ty: &Ty) -> bool {
    match ty {
        Ty::I8 => i8::try_from(value).is_ok(),
        Ty::I16 => i16::try_from(value).is_ok(),
        Ty::I32 => i32::try_from(value).is_ok(),
        Ty::I64 => i64::try_from(value).is_ok(),
        Ty::ISize => isize::try_from(value).is_ok(),
        Ty::I128 => true,
        Ty::U8 => u8::try_from(value).is_ok(),
        Ty::U16 => u16::try_from(value).is_ok(),
        Ty::U32 => u32::try_from(value).is_ok(),
        Ty::U64 => u64::try_from(value).is_ok(),
        Ty::USize => usize::try_from(value).is_ok(),
        Ty::U128 => value >= 0,
        _ => false,
    }
}

pub(super) fn integer_bit_width(ty: &Ty) -> u32 {
    match ty {
        Ty::I8 | Ty::U8 => 8,
        Ty::I16 | Ty::U16 => 16,
        Ty::I32 | Ty::U32 => 32,
        Ty::I64 | Ty::U64 => 64,
        Ty::ISize | Ty::USize => usize::BITS,
        Ty::I128 | Ty::U128 => 128,
        _ => 0,
    }
}

pub(super) fn signed_integer_min(ty: &Ty) -> Option<i128> {
    match ty {
        Ty::I8 => Some(i128::from(i8::MIN)),
        Ty::I16 => Some(i128::from(i16::MIN)),
        Ty::I32 => Some(i128::from(i32::MIN)),
        Ty::I64 => Some(i128::from(i64::MIN)),
        Ty::I128 => Some(i128::MIN),
        Ty::ISize => Some(isize::MIN as i128),
        _ => None,
    }
}

pub(super) fn nominal_name(ty: &Ty) -> Option<&str> {
    match ty {
        Ty::Struct(name) | Ty::Enum(name) => Some(name),
        Ty::Array(element, _) => nominal_name(element),
        Ty::Tuple(fields) => fields.iter().find_map(nominal_name),
        _ => None,
    }
}

pub(super) fn ty_contains_nominal(ty: &Ty, nominal: &str) -> bool {
    match ty {
        Ty::Struct(name) | Ty::Enum(name) => name == nominal,
        Ty::Pointer { pointee, .. } | Ty::Reference { pointee, .. } | Ty::Array(pointee, _) => {
            ty_contains_nominal(pointee, nominal)
        }
        Ty::Tuple(fields) => fields
            .iter()
            .any(|field| ty_contains_nominal(field, nominal)),
        Ty::Function(function) => function_ty_contains_nominal(function, nominal),
        Ty::Callable(callable) => {
            function_ty_contains_nominal(&callable.signature, nominal)
                || callable
                    .captures
                    .iter()
                    .any(|capture| ty_contains_nominal(&capture.ty, nominal))
        }
        Ty::Continuation { input, output } => {
            ty_contains_nominal(input, nominal) || ty_contains_nominal(output, nominal)
        }
        Ty::EffectCallable {
            input,
            output,
            answer,
        } => {
            ty_contains_nominal(input, nominal)
                || ty_contains_nominal(output, nominal)
                || ty_contains_nominal(answer, nominal)
        }
        Ty::EffectRow { throws_error, .. } => throws_error
            .as_deref()
            .is_some_and(|error| ty_contains_nominal(error, nominal)),
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
        | Ty::Unit
        | Ty::Never
        | Ty::Error => false,
    }
}

pub(super) fn function_ty_contains_nominal(function: &FunctionTy, nominal: &str) -> bool {
    function
        .groups
        .iter()
        .flatten()
        .any(|parameter| ty_contains_nominal(parameter, nominal))
        || function
            .throws_error
            .as_deref()
            .is_some_and(|error| ty_contains_nominal(error, nominal))
        || ty_contains_nominal(&function.result, nominal)
}

pub(super) fn is_unconstrained_integer(expression: &Expr) -> bool {
    match expression.unlocated() {
        Expr::Integer(_) => true,
        Expr::Unary(UnaryOp::Neg, operand) => matches!(operand.as_ref(), Expr::Integer(_)),
        Expr::Block(_, Some(tail)) | Expr::DoBlock { body: tail } => is_unconstrained_integer(tail),
        _ => false,
    }
}

pub(super) fn integer_literal_value(expression: &Expr) -> Option<i128> {
    match expression.unlocated() {
        Expr::Integer(value) => i128::try_from(*value).ok(),
        Expr::Unary(UnaryOp::Neg, operand) => {
            let Expr::Integer(value) = operand.as_ref() else {
                return None;
            };
            if *value == (i128::MAX as u128) + 1 {
                Some(i128::MIN)
            } else {
                i128::try_from(*value).ok()?.checked_neg()
            }
        }
        Expr::Block(statements, Some(tail)) if statements.is_empty() => integer_literal_value(tail),
        Expr::DoBlock { body } => integer_literal_value(body),
        _ => None,
    }
}

use crate::ast::BinaryOp;

use super::hir::{LayoutQueryKind, Ty};
use super::target::NATIVE_TARGET;

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) enum IntegerEvalError {
    TypeMismatch,
    InvalidOperator,
    InvalidNegation,
    Overflow,
    DivisionByZero,
    RemainderByZero,
    InvalidShift { count: String, width: u32 },
}

/// One normalized runtime-typed value produced by compile-time evaluation.
///
/// Static metadata such as `type`, `effect`, and finite-sort members remains
/// in `StaticValue`; it must not be encoded in this runtime value domain.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct CtfeValue {
    pub(super) ty: Ty,
    pub(super) kind: CtfeValueKind,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) enum CtfeValueKind {
    /// A normalized two's-complement bit pattern. The enclosing `ty`
    /// determines signed interpretation and the significant width.
    Integer(u128),
    Bool(bool),
    String(String),
    Unit,
    Tuple(Vec<CtfeValue>),
    Array(Vec<CtfeValue>),
    Struct {
        name: String,
        fields: Vec<CtfeValue>,
    },
    Enum {
        name: String,
        variant: usize,
        fields: Vec<CtfeValue>,
    },
    LayoutQuery {
        queried: Ty,
        kind: LayoutQueryKind,
    },
}

impl CtfeValue {
    pub(super) fn integer_bits(ty: Ty, bits: u128) -> Self {
        debug_assert!(ty.is_integer());
        let width = NATIVE_TARGET
            .integer_width(&ty)
            .expect("integer CTFE value has an integer type");
        let bits = if width == 128 {
            bits
        } else {
            bits & ((1_u128 << width) - 1)
        };
        Self {
            ty,
            kind: CtfeValueKind::Integer(bits),
        }
    }

    pub(super) fn usize(value: u64) -> Self {
        Self::integer_bits(Ty::USize, u128::from(value))
    }

    pub(super) fn integer_literal(ty: Ty, magnitude: u128, negative: bool) -> Result<Self, String> {
        if !ty.is_integer() {
            return Err(format!("`{ty}` is not an integer type"));
        }
        if negative {
            let minimum_magnitude = NATIVE_TARGET
                .signed_min(&ty)
                .map(i128::unsigned_abs)
                .ok_or_else(|| format!("negative integer cannot have type `{ty}`"))?;
            if magnitude > minimum_magnitude {
                return Err(format!(
                    "integer literal `-{magnitude}` does not fit in `{ty}`"
                ));
            }
            Ok(Self::integer_bits(ty, 0_u128.wrapping_sub(magnitude)))
        } else {
            let fits = if ty.is_signed() {
                magnitude
                    <= NATIVE_TARGET
                        .signed_max(&ty)
                        .expect("signed integer has maximum") as u128
            } else {
                magnitude
                    <= NATIVE_TARGET
                        .unsigned_max(&ty)
                        .expect("unsigned integer has maximum")
            };
            if !fits {
                return Err(format!(
                    "integer literal `{magnitude}` does not fit in `{ty}`"
                ));
            }
            Ok(Self::integer_bits(ty, magnitude))
        }
    }

    pub(super) fn bool(value: bool) -> Self {
        Self {
            ty: Ty::Bool,
            kind: CtfeValueKind::Bool(value),
        }
    }

    pub(super) fn unit() -> Self {
        Self {
            ty: Ty::Unit,
            kind: CtfeValueKind::Unit,
        }
    }

    pub(super) fn integer_bits_value(&self) -> Option<u128> {
        match self.kind {
            CtfeValueKind::Integer(bits) => Some(bits),
            _ => None,
        }
    }

    pub(super) fn signed_integer_value(&self) -> Option<i128> {
        if !self.ty.is_signed() {
            return None;
        }
        let bits = self.integer_bits_value()?;
        let width = NATIVE_TARGET.integer_width(&self.ty)?;
        Some(if width == 128 {
            bits as i128
        } else {
            let shift = 128 - width;
            ((bits << shift) as i128) >> shift
        })
    }

    pub(super) fn unsigned_integer_value(&self) -> Option<u128> {
        (!self.ty.is_signed())
            .then(|| self.integer_bits_value())
            .flatten()
    }

    pub(super) fn nonnegative_integer_value(&self) -> Option<u128> {
        if self.ty.is_signed() {
            u128::try_from(self.signed_integer_value()?).ok()
        } else {
            self.unsigned_integer_value()
        }
    }

    pub(super) fn usize_value(&self) -> Option<u64> {
        (self.ty == Ty::USize)
            .then(|| self.unsigned_integer_value())
            .flatten()
            .and_then(|value| u64::try_from(value).ok())
    }

    pub(super) fn integer_display(&self) -> Option<String> {
        if self.ty.is_signed() {
            self.signed_integer_value().map(|value| value.to_string())
        } else {
            self.unsigned_integer_value().map(|value| value.to_string())
        }
    }

    pub(super) fn checked_integer_neg(&self) -> Result<Self, IntegerEvalError> {
        let value = self
            .signed_integer_value()
            .ok_or(IntegerEvalError::InvalidNegation)?;
        let value = value.checked_neg().ok_or(IntegerEvalError::Overflow)?;
        Self::from_signed(self.ty.clone(), value)
    }

    pub(super) fn checked_integer_binary(
        &self,
        operator: BinaryOp,
        right: &Self,
    ) -> Result<Self, IntegerEvalError> {
        if self.ty != right.ty || !self.ty.is_integer() {
            return Err(IntegerEvalError::TypeMismatch);
        }
        let width = NATIVE_TARGET
            .integer_width(&self.ty)
            .expect("integer value has an integer width");
        if matches!(operator, BinaryOp::Shl | BinaryOp::Shr) {
            let count = right
                .nonnegative_integer_value()
                .and_then(|count| u32::try_from(count).ok())
                .filter(|count| *count < width)
                .ok_or_else(|| IntegerEvalError::InvalidShift {
                    count: right
                        .integer_display()
                        .unwrap_or_else(|| "<invalid>".to_owned()),
                    width,
                })?;
            return self.checked_shift(operator, count);
        }
        if self.ty.is_signed() {
            let right_bits = right.to_bits(width);
            let left = self
                .signed_integer_value()
                .expect("signed integer has signed payload");
            let right = right
                .signed_integer_value()
                .expect("signed integer has signed payload");
            if matches!(operator, BinaryOp::Div | BinaryOp::Rem)
                && right == -1
                && NATIVE_TARGET.signed_min(&self.ty) == Some(left)
            {
                return Err(IntegerEvalError::Overflow);
            }
            let value = match operator {
                BinaryOp::Add => left.checked_add(right),
                BinaryOp::Sub => left.checked_sub(right),
                BinaryOp::Mul => left.checked_mul(right),
                BinaryOp::Div if right == 0 => return Err(IntegerEvalError::DivisionByZero),
                BinaryOp::Div => left.checked_div(right),
                BinaryOp::Rem if right == 0 => return Err(IntegerEvalError::RemainderByZero),
                BinaryOp::Rem => left.checked_rem(right),
                BinaryOp::BitAnd => {
                    return Ok(Self::integer_bits(
                        self.ty.clone(),
                        self.integer_bits_value().expect("integer payload") & right_bits,
                    ));
                }
                BinaryOp::BitOr => {
                    return Ok(Self::integer_bits(
                        self.ty.clone(),
                        self.integer_bits_value().expect("integer payload") | right_bits,
                    ));
                }
                BinaryOp::BitXor => {
                    return Ok(Self::integer_bits(
                        self.ty.clone(),
                        self.integer_bits_value().expect("integer payload") ^ right_bits,
                    ));
                }
                BinaryOp::Eq => return Ok(Self::bool(left == right)),
                BinaryOp::Ne => return Ok(Self::bool(left != right)),
                BinaryOp::Lt => return Ok(Self::bool(left < right)),
                BinaryOp::Le => return Ok(Self::bool(left <= right)),
                BinaryOp::Gt => return Ok(Self::bool(left > right)),
                BinaryOp::Ge => return Ok(Self::bool(left >= right)),
                BinaryOp::And | BinaryOp::Or | BinaryOp::Shl | BinaryOp::Shr => {
                    return Err(IntegerEvalError::InvalidOperator);
                }
            }
            .ok_or(IntegerEvalError::Overflow)?;
            Self::from_signed(self.ty.clone(), value)
        } else {
            let left = self
                .unsigned_integer_value()
                .expect("unsigned integer has unsigned payload");
            let right = right
                .unsigned_integer_value()
                .expect("unsigned integer has unsigned payload");
            let value = match operator {
                BinaryOp::Add => left.checked_add(right),
                BinaryOp::Sub => left.checked_sub(right),
                BinaryOp::Mul => left.checked_mul(right),
                BinaryOp::Div if right == 0 => return Err(IntegerEvalError::DivisionByZero),
                BinaryOp::Div => left.checked_div(right),
                BinaryOp::Rem if right == 0 => return Err(IntegerEvalError::RemainderByZero),
                BinaryOp::Rem => left.checked_rem(right),
                BinaryOp::BitAnd => Some(left & right),
                BinaryOp::BitOr => Some(left | right),
                BinaryOp::BitXor => Some(left ^ right),
                BinaryOp::Eq => return Ok(Self::bool(left == right)),
                BinaryOp::Ne => return Ok(Self::bool(left != right)),
                BinaryOp::Lt => return Ok(Self::bool(left < right)),
                BinaryOp::Le => return Ok(Self::bool(left <= right)),
                BinaryOp::Gt => return Ok(Self::bool(left > right)),
                BinaryOp::Ge => return Ok(Self::bool(left >= right)),
                BinaryOp::And | BinaryOp::Or | BinaryOp::Shl | BinaryOp::Shr => {
                    return Err(IntegerEvalError::InvalidOperator);
                }
            }
            .ok_or(IntegerEvalError::Overflow)?;
            Self::from_unsigned(self.ty.clone(), value)
        }
    }

    fn checked_shift(&self, operator: BinaryOp, count: u32) -> Result<Self, IntegerEvalError> {
        if self.ty.is_signed() {
            let value = self
                .signed_integer_value()
                .expect("signed integer has signed payload");
            let shifted = match operator {
                BinaryOp::Shr => value >> count,
                BinaryOp::Shl if count == 127 => match value {
                    -1 => i128::MIN,
                    0 => 0,
                    _ => return Err(IntegerEvalError::Overflow),
                },
                BinaryOp::Shl => value
                    .checked_mul(1_i128 << count)
                    .ok_or(IntegerEvalError::Overflow)?,
                _ => return Err(IntegerEvalError::InvalidOperator),
            };
            Self::from_signed(self.ty.clone(), shifted)
        } else {
            let value = self
                .unsigned_integer_value()
                .expect("unsigned integer has unsigned payload");
            let maximum = NATIVE_TARGET
                .unsigned_max(&self.ty)
                .expect("unsigned integer has maximum");
            let shifted = match operator {
                BinaryOp::Shr => value >> count,
                BinaryOp::Shl if value <= (maximum >> count) => value << count,
                BinaryOp::Shl => return Err(IntegerEvalError::Overflow),
                _ => return Err(IntegerEvalError::InvalidOperator),
            };
            Self::from_unsigned(self.ty.clone(), shifted)
        }
    }

    fn from_signed(ty: Ty, value: i128) -> Result<Self, IntegerEvalError> {
        let in_range = NATIVE_TARGET
            .signed_min(&ty)
            .zip(NATIVE_TARGET.signed_max(&ty))
            .is_some_and(|(minimum, maximum)| (minimum..=maximum).contains(&value));
        in_range
            .then(|| Self::integer_bits(ty, value as u128))
            .ok_or(IntegerEvalError::Overflow)
    }

    fn from_unsigned(ty: Ty, value: u128) -> Result<Self, IntegerEvalError> {
        NATIVE_TARGET
            .unsigned_max(&ty)
            .is_some_and(|maximum| value <= maximum)
            .then(|| Self::integer_bits(ty, value))
            .ok_or(IntegerEvalError::Overflow)
    }

    fn to_bits(&self, width: u32) -> u128 {
        let bits = self.integer_bits_value().expect("integer payload");
        if width == 128 {
            bits
        } else {
            bits & ((1_u128 << width) - 1)
        }
    }

    pub(super) fn bool_value(&self) -> Option<bool> {
        match self.kind {
            CtfeValueKind::Bool(value) => Some(value),
            _ => None,
        }
    }

    pub(super) fn projection(&self, index: usize) -> Option<&CtfeValue> {
        match &self.kind {
            CtfeValueKind::Tuple(fields) | CtfeValueKind::Array(fields) => fields.get(index),
            CtfeValueKind::Struct { fields, .. } => fields.get(index),
            CtfeValueKind::Enum { fields, .. } => fields.get(index),
            CtfeValueKind::Integer(_)
            | CtfeValueKind::Bool(_)
            | CtfeValueKind::String(_)
            | CtfeValueKind::Unit
            | CtfeValueKind::LayoutQuery { .. } => None,
        }
    }
}

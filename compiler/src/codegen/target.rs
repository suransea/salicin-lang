use super::hir::Ty;

/// The compilation target understood by the current native-only backend.
///
/// Keeping pointer width in an explicit target description prevents CTFE and
/// LLVM lowering from silently inheriting the Rust compiler host's `usize`.
/// Cross-target selection can replace this single value without changing
/// integer semantics again.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) struct Target {
    pub(super) pointer_width: u32,
}

pub(super) const NATIVE_TARGET: Target = Target { pointer_width: 64 };

impl Target {
    pub(super) fn integer_width(self, ty: &Ty) -> Option<u32> {
        match ty {
            Ty::I8 | Ty::U8 => Some(8),
            Ty::I16 | Ty::U16 => Some(16),
            Ty::I32 | Ty::U32 => Some(32),
            Ty::I64 | Ty::U64 => Some(64),
            Ty::ISize | Ty::USize => Some(self.pointer_width),
            Ty::I128 | Ty::U128 => Some(128),
            _ => None,
        }
    }

    pub(super) fn unsigned_max(self, ty: &Ty) -> Option<u128> {
        let width = self.integer_width(ty)?;
        Some(if width == 128 {
            u128::MAX
        } else {
            (1_u128 << width) - 1
        })
    }

    pub(super) fn signed_min(self, ty: &Ty) -> Option<i128> {
        if !ty.is_signed() {
            return None;
        }
        let width = self.integer_width(ty)?;
        Some(if width == 128 {
            i128::MIN
        } else {
            -(1_i128 << (width - 1))
        })
    }

    pub(super) fn signed_max(self, ty: &Ty) -> Option<i128> {
        if !ty.is_signed() {
            return None;
        }
        let width = self.integer_width(ty)?;
        Some(if width == 128 {
            i128::MAX
        } else {
            (1_i128 << (width - 1)) - 1
        })
    }
}

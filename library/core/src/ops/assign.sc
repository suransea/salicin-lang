/// Trait backing compound `+=`.
pub let add_assign(comptime rhs: type) = trait {
  /// Adds `rhs` into `self` in place.
  let add_assign(self: borrow(mut)(self))
    (rhs: rhs): ()
}

/// Trait backing compound `-=`.
pub let sub_assign(comptime rhs: type) = trait {
  /// Subtracts `rhs` from `self` in place.
  let sub_assign(self: borrow(mut)(self))
    (rhs: rhs): ()
}

/// Trait backing compound `*=`.
pub let mul_assign(comptime rhs: type) = trait {
  /// Multiplies `self` by `rhs` in place.
  let mul_assign(self: borrow(mut)(self))
    (rhs: rhs): ()
}

/// Trait backing compound `/=`.
pub let div_assign(comptime rhs: type) = trait {
  /// Divides `self` by `rhs` in place.
  let div_assign(self: borrow(mut)(self))
    (rhs: rhs): ()
}

/// Trait backing compound `%=`.
pub let rem_assign(comptime rhs: type) = trait {
  /// Replaces `self` with its remainder after division by `rhs`.
  let rem_assign(self: borrow(mut)(self))
    (rhs: rhs): ()
}

/// Trait backing compound `&=`.
pub let bit_and_assign(comptime rhs: type) = trait {
  /// Applies bitwise and with `rhs` in place.
  let bit_and_assign(self: borrow(mut)(self))
    (rhs: rhs): ()
}

/// Trait backing compound `|=`.
pub let bit_or_assign(comptime rhs: type) = trait {
  /// Applies bitwise or with `rhs` in place.
  let bit_or_assign(self: borrow(mut)(self))
    (rhs: rhs): ()
}

/// Trait backing compound `^=`.
pub let bit_xor_assign(comptime rhs: type) = trait {
  /// Applies bitwise xor with `rhs` in place.
  let bit_xor_assign(self: borrow(mut)(self))
    (rhs: rhs): ()
}

/// Trait backing compound `<<=`.
pub let shl_assign(comptime rhs: type) = trait {
  /// Shifts `self` left by `rhs` in place.
  let shl_assign(self: borrow(mut)(self))
    (rhs: rhs): ()
}

/// Trait backing compound `>>=`.
pub let shr_assign(comptime rhs: type) = trait {
  /// Shifts `self` right by `rhs` in place.
  let shr_assign(self: borrow(mut)(self))
    (rhs: rhs): ()
}

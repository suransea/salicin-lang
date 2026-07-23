/// Trait backing compound `+=`.
pub let AddAssign(Rhs: type) = trait {
  /// Adds `rhs` into `self` in place.
  let add_assign(self: borrow(mut)(Self))
    (move rhs: Rhs): ()
}

/// Trait backing compound `-=`.
pub let SubAssign(Rhs: type) = trait {
  /// Subtracts `rhs` from `self` in place.
  let sub_assign(self: borrow(mut)(Self))
    (move rhs: Rhs): ()
}

/// Trait backing compound `*=`.
pub let MulAssign(Rhs: type) = trait {
  /// Multiplies `self` by `rhs` in place.
  let mul_assign(self: borrow(mut)(Self))
    (move rhs: Rhs): ()
}

/// Trait backing compound `/=`.
pub let DivAssign(Rhs: type) = trait {
  /// Divides `self` by `rhs` in place.
  let div_assign(self: borrow(mut)(Self))
    (move rhs: Rhs): ()
}

/// Trait backing compound `%=`.
pub let RemAssign(Rhs: type) = trait {
  /// Replaces `self` with its remainder after division by `rhs`.
  let rem_assign(self: borrow(mut)(Self))
    (move rhs: Rhs): ()
}

/// Trait backing compound `&=`.
pub let BitAndAssign(Rhs: type) = trait {
  /// Applies bitwise and with `rhs` in place.
  let bit_and_assign(self: borrow(mut)(Self))
    (move rhs: Rhs): ()
}

/// Trait backing compound `|=`.
pub let BitOrAssign(Rhs: type) = trait {
  /// Applies bitwise or with `rhs` in place.
  let bit_or_assign(self: borrow(mut)(Self))
    (move rhs: Rhs): ()
}

/// Trait backing compound `^=`.
pub let BitXorAssign(Rhs: type) = trait {
  /// Applies bitwise xor with `rhs` in place.
  let bit_xor_assign(self: borrow(mut)(Self))
    (move rhs: Rhs): ()
}

/// Trait backing compound `<<=`.
pub let ShlAssign(Rhs: type) = trait {
  /// Shifts `self` left by `rhs` in place.
  let shl_assign(self: borrow(mut)(Self))
    (move rhs: Rhs): ()
}

/// Trait backing compound `>>=`.
pub let ShrAssign(Rhs: type) = trait {
  /// Shifts `self` right by `rhs` in place.
  let shr_assign(self: borrow(mut)(Self))
    (move rhs: Rhs): ()
}

pub let Add(Rhs: type) = trait {
  let Output: type
  let add(move self)(move rhs: Rhs): Output
}

pub let Sub(Rhs: type) = trait {
  let Output: type
  let sub(move self)(move rhs: Rhs): Output
}

pub let Mul(Rhs: type) = trait {
  let Output: type
  let mul(move self)(move rhs: Rhs): Output
}

pub let Div(Rhs: type) = trait {
  let Output: type
  let div(move self)(move rhs: Rhs): Output
}

pub let Rem(Rhs: type) = trait {
  let Output: type
  let rem(move self)(move rhs: Rhs): Output
}

pub let AddAssign(Rhs: type) = trait {
  let add_assign(borrow(mut) self)(move rhs: Rhs): ()
}

pub let SubAssign(Rhs: type) = trait {
  let sub_assign(borrow(mut) self)(move rhs: Rhs): ()
}

pub let MulAssign(Rhs: type) = trait {
  let mul_assign(borrow(mut) self)(move rhs: Rhs): ()
}

pub let DivAssign(Rhs: type) = trait {
  let div_assign(borrow(mut) self)(move rhs: Rhs): ()
}

pub let RemAssign(Rhs: type) = trait {
  let rem_assign(borrow(mut) self)(move rhs: Rhs): ()
}

pub let Eq(Rhs: type) = trait {
  let eq(borrow self)(borrow rhs: Rhs): bool
}

pub let PartialOrdering = enum {
  Less,
  Equal,
  Greater,
  Unordered,
}

pub let PartialOrd(Rhs: type) = trait {
  let partial_cmp(borrow self)(borrow rhs: Rhs): PartialOrdering
}

pub let Neg = trait {
  let Output: type
  let neg(move self)(): Output
}

pub let Not = trait {
  let Output: type
  let not(move self)(): Output
}

pub let BitAnd(Rhs: type) = trait {
  let Output: type
  let bit_and(move self)(move rhs: Rhs): Output
}

pub let BitOr(Rhs: type) = trait {
  let Output: type
  let bit_or(move self)(move rhs: Rhs): Output
}

pub let BitXor(Rhs: type) = trait {
  let Output: type
  let bit_xor(move self)(move rhs: Rhs): Output
}

pub let Shl(Rhs: type) = trait {
  let Output: type
  let shl(move self)(move rhs: Rhs): Output
}

pub let Shr(Rhs: type) = trait {
  let Output: type
  let shr(move self)(move rhs: Rhs): Output
}

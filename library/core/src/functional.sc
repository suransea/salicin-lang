// Higher-kinded functional protocols. These are normal standard-library
// traits over compile-time type constructors, not prelude items.
//
// Trait inheritance constraints such as `Applicative where F: Functor` are not
// part of the current trait-declaration grammar yet, so the protocols are
// declared independently for now. Law documentation belongs in the standard
// library docs and tests; the compiler does not prove these laws.
pub let Functor(F: (Value: type): type) = trait {
  let map(E: effect, A: type, B: type)(
    move value: F(A),
    move transform: (A): B with(E),
  ): F(B) with(E)
}

pub let Applicative(F: (Value: type): type) = trait {
  let pure(A: type)(move value: A): F(A)

  let apply(E: effect, A: type, B: type)(
    move transform: F((A): B with(E)),
    move value: F(A),
  ): F(B) with(E)
}

pub let Monad(M: (Value: type): type) = trait {
  let flat_map(E: effect, A: type, B: type)(
    move value: M(A),
    move next: (A): M(B) with(E),
  ): M(B) with(E)
}

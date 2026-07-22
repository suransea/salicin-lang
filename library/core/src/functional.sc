// Higher-kinded functional protocols. These are normal standard-library
// traits over compile-time type constructors, not prelude items. Law
// documentation belongs in the standard library docs and tests; the compiler
// does not prove these laws.
pub let Functor = trait(Self: (Value: type): type) {
  let map(E: effect, A: type, B: type)(
    move self: Self(A),
  )(
    move transform: (A): B with(E),
  ): Self(B) with(E)
}

pub let Applicative = trait(Self: (Value: type): type)
where Self: Functor {
  let pure(A: type)(move value: A): Self(A)

  let apply(E: effect, A: type, B: type)(
    move self: Self((A): B with(E)),
  )(
    move value: Self(A),
  ): Self(B) with(E)
}

pub let Monad = trait(Self: (Value: type): type)
where Self: Applicative {
  let flat_map(E: effect, A: type, B: type)(
    move self: Self(A),
  )(
    move next: (A): Self(B) with(E),
  ): Self(B) with(E)
}

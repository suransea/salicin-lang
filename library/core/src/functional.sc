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

pub let ResultWith(Error: type)(Value: type): type = Result(Value, Error)

extend Option: Functor {
  let map(E: effect, A: type, B: type)(
    move self: Option(A),
  )(
    move transform: (A): B with(E),
  ): Option(B) with(E) = {
    self match {
      Some(value) => Option(B).Some(transform(value)),
      None => Option(B).None,
    }
  }
}

extend Option: Applicative {
  let pure(A: type)(move value: A): Option(A) = {
    Option(A).Some(value)
  }

  let apply(E: effect, A: type, B: type)(
    move self: Option((A): B with(E)),
  )(
    move value: Option(A),
  ): Option(B) with(E) = {
    self match {
      Some(transform) => value match {
        Some(value) => Option(B).Some(transform(value)),
        None => Option(B).None,
      },
      None => Option(B).None,
    }
  }
}

extend Option: Monad {
  let flat_map(E: effect, A: type, B: type)(
    move self: Option(A),
  )(
    move next: (A): Option(B) with(E),
  ): Option(B) with(E) = {
    self match {
      Some(value) => next(value),
      None => Option(B).None,
    }
  }
}

extend(Error: type) ResultWith(Error): Functor {
  let map(E: effect, A: type, B: type)(
    move self: Result(A, Error),
  )(
    move transform: (A): B with(E),
  ): Result(B, Error) with(E) = {
    self match {
      Ok(value) => Result(B, Error).Ok(transform(value)),
      Err(error) => Result(B, Error).Err(error),
    }
  }
}

extend(Error: type) ResultWith(Error): Applicative {
  let pure(A: type)(move value: A): Result(A, Error) = {
    Result(A, Error).Ok(value)
  }

  let apply(E: effect, A: type, B: type)(
    move self: Result((A): B with(E), Error),
  )(
    move value: Result(A, Error),
  ): Result(B, Error) with(E) = {
    self match {
      Ok(transform) => value match {
        Ok(value) => Result(B, Error).Ok(transform(value)),
        Err(error) => Result(B, Error).Err(error),
      },
      Err(error) => Result(B, Error).Err(error),
    }
  }
}

extend(Error: type) ResultWith(Error): Monad {
  let flat_map(E: effect, A: type, B: type)(
    move self: Result(A, Error),
  )(
    move next: (A): Result(B, Error) with(E),
  ): Result(B, Error) with(E) = {
    self match {
      Ok(value) => next(value),
      Err(error) => Result(B, Error).Err(error),
    }
  }
}

extend core.Option: core.functional.Functor {
  let map(E: effect, A: type, B: type)(
    move self: core.Option(A),
  )(
    move transform: (A): B with(E),
  ): core.Option(B) with(E) = {
    self match {
      Some(value) => core.Option.Some(transform(value)),
      None => core.Option.None,
    }
  }
}

extend core.Option: core.functional.Applicative {
  let pure(A: type)(move value: A): core.Option(A) = {
    core.Option.Some(value)
  }

  let apply(E: effect, A: type, B: type)(
    move self: core.Option((A): B with(E)),
  )(
    move value: core.Option(A),
  ): core.Option(B) with(E) = {
    self match {
      Some(transform) => value match {
        Some(value) => core.Option.Some(transform(value)),
        None => core.Option.None,
      },
      None => core.Option.None,
    }
  }
}

extend core.Option: core.functional.Monad {
  let flat_map(E: effect, A: type, B: type)(
    move self: core.Option(A),
  )(
    move next: (A): core.Option(B) with(E),
  ): core.Option(B) with(E) = {
    self match {
      Some(value) => next(value),
      None => core.Option.None,
    }
  }
}

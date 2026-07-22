extend(Error: type) core.ResultWith(Error): core.functional.Functor {
  let map(E: effect, A: type, B: type)(
    move self: core.Result(A, Error),
  )(
    move transform: (A): B with(E),
  ): core.Result(B, Error) with(E) = {
    self match {
      Ok(value) => core.Result.Ok(transform(value)),
      Err(error) => core.Result.Err(error),
    }
  }
}

extend(Error: type) core.ResultWith(Error): core.functional.Applicative {
  let pure(A: type)(move value: A): core.Result(A, Error) = {
    core.Result.Ok(value)
  }

  let apply(E: effect, A: type, B: type)(
    move self: core.Result((A): B with(E), Error),
  )(
    move value: core.Result(A, Error),
  ): core.Result(B, Error) with(E) = {
    self match {
      Ok(transform) => value match {
        Ok(value) => core.Result.Ok(transform(value)),
        Err(error) => core.Result.Err(error),
      },
      Err(error) => core.Result.Err(error),
    }
  }
}

extend(Error: type) core.ResultWith(Error): core.functional.Monad {
  let flat_map(E: effect, A: type, B: type)(
    move self: core.Result(A, Error),
  )(
    move next: (A): core.Result(B, Error) with(E),
  ): core.Result(B, Error) with(E) = {
    self match {
      Ok(value) => next(value),
      Err(error) => core.Result.Err(error),
    }
  }
}

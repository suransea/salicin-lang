extend(Error: type) core.Result(Error): core.functional.Functor {
  let map(E: effect, A: type, B: type)(
    move self: core.Result(Error)(A),
  )(
    move transform: (A): B with(E),
  ): core.Result(Error)(B) with(E) = {
    self match {
      Ok(value) => core.Result.Ok(transform(value)),
      Err(error) => core.Result.Err(error),
    }
  }
}

extend(Error: type) core.Result(Error): core.functional.Applicative {
  let pure(A: type)(move value: A): core.Result(Error)(A) = {
    core.Result.Ok(value)
  }

  let apply(E: effect, A: type, B: type)(
    move self: core.Result(Error)((A): B with(E)),
  )(
    move value: core.Result(Error)(A),
  ): core.Result(Error)(B) with(E) = {
    self match {
      Ok(transform) => value match {
        Ok(value) => core.Result.Ok(transform(value)),
        Err(error) => core.Result.Err(error),
      },
      Err(error) => core.Result.Err(error),
    }
  }
}

extend(Error: type) core.Result(Error): core.functional.Monad {
  let flat_map(E: effect, A: type, B: type)(
    move self: core.Result(Error)(A),
  )(
    move next: (A): core.Result(Error)(B) with(E),
  ): core.Result(Error)(B) with(E) = {
    self match {
      Ok(value) => next(value),
      Err(error) => core.Result.Err(error),
    }
  }
}

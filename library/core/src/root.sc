pub let Option(T: type) = enum {
  Some(T),
  None,
}

pub let Result(T: type, E: type) = enum {
  Ok(T),
  Err(E),
}

pub let ResultWith(Error: type)(Value: type): type = Result(Value, Error)

extend(T: type) Option(T): core.ops.Chain {
  let Item = T
  let Rebind = Option

  let chain(E: effect, U: type)(
    move self,
  )(
    move transform: (T): U with(E),
  ): Option(U) with(E) = {
    self match {
      Some(value) => Option.Some(transform(value)),
      None => Option.None,
    }
  }
}

extend(T: type) Option(T): core.ops.Coalesce {
  let Item = T

  let coalesce(E: effect)(
    move self,
  )(
    move fallback: (): T with(E),
  ): T with(E) = {
    self match {
      Some(value) => value,
      None => fallback(),
    }
  }
}

extend(T: type, Error: type) Result(T, Error): core.ops.Chain {
  let Item = T
  let Rebind = ResultWith(Error)

  let chain(E: effect, U: type)(
    move self,
  )(
    move transform: (T): U with(E),
  ): Result(U, Error) with(E) = {
    self match {
      Ok(value) => Result.Ok(transform(value)),
      Err(error) => Result.Err(error),
    }
  }
}

extend(T: type, Error: type) Result(T, Error): core.ops.Coalesce {
  let Item = T

  let coalesce(E: effect)(
    move self,
  )(
    move fallback: (): T with(E),
  ): T with(E) = {
    self match {
      Ok(value) => value,
      Err(_) => fallback(),
    }
  }
}

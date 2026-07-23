pub let Option(T: type) = enum {
  Some(T),
  None,
}

pub let Result(E: type)
  (T: type) = enum {
  Ok(T),
  Err(E),
}

extend(T: type) Option(T): core.ops.Chain {
  let Item = T
  let Rebind = Option

  let chain(E: effect, U: type)
    (move self)
    (move transform: (T): U with(E)): Option(U) with(E) = {
    self match {
      Some(value) => Option.Some(transform(value)),
      None => Option.None,
    }
  }
}

extend(T: type) Option(T): core.ops.Coalesce {
  let Item = T

  let coalesce(E: effect)
    (move self)
    (move fallback: (): T with(E)): T with(E) = {
    self match {
      Some(value) => value,
      None => fallback(),
    }
  }
}

extend(Error: type, T: type) Result(Error)(T): core.ops.Chain {
  let Item = T
  let Rebind = Result(Error)

  let chain(E: effect, U: type)
    (move self)
    (move transform: (T): U with(E)): Result(Error)(U) with(E) = {
    self match {
      Ok(value) => Result.Ok(transform(value)),
      Err(error) => Result.Err(error),
    }
  }
}

extend(Error: type, T: type) Result(Error)(T): core.ops.Coalesce {
  let Item = T

  let coalesce(E: effect)
    (move self)
    (move fallback: (): T with(E)): T with(E) = {
    self match {
      Ok(value) => value,
      Err(_) => fallback(),
    }
  }
}

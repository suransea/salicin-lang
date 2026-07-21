pub let ControlFlow(Break: type, Continue: type) = enum {
  Continue(Continue),
  Break(Break),
}

pub let Try = trait {
  let Output: type
  let Residual: type
  let branch(move self)(): ControlFlow(Residual, Output)
  let from_output(move output: Output): Self
}

pub let FromResidual(R: type) = trait {
  let from_residual(move residual: R): Self
}

pub let FromError(E: type) = trait {
  let from_error(move error: E): Self
}

extend(T: type) root.Option(T): Try {
  let Output = T
  let Residual = ()
  let branch(move self)(): ControlFlow((), T) = self match {
    Some(value) => Continue(value),
    None => Break(()),
  }
  let from_output(move output: T): root.Option(T) = Some(output)
}

extend(T: type) root.Option(T): FromResidual(()) {
  let from_residual(move residual: ()): root.Option(T) = None
}

extend(T: type, E: type) root.Result(T, E): Try {
  let Output = T
  let Residual = E
  let branch(move self)(): ControlFlow(E, T) = self match {
    Ok(value) => Continue(value),
    Err(error) => Break(error),
  }
  let from_output(move output: T): root.Result(T, E) = Ok(output)
}

extend(T: type, E: type) root.Result(T, E): FromResidual(E) {
  let from_residual(move residual: E): root.Result(T, E) = Err(residual)
}

extend(T: type, E: type) root.Result(T, E): FromError(E) {
  let from_error(move error: E): root.Result(T, E) = Err(error)
}

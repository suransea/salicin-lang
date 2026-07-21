use core.control.{ControlFlow, FromError, FromResidual, Try}

let Flow = enum {
  Value(i32),
  Stop(bool),
}

extend Flow: Try {
  let Output = i32
  let Residual = bool
  let branch(move self)(): ControlFlow(bool, i32) = self match {
    Value(value) => Continue(value),
    Stop(error) => Break(error),
  }
  let from_output(move output: i32): Flow = Value(output)
}

extend Flow: FromResidual(bool) {
  let from_residual(move residual: bool): Flow = Stop(residual)
}

extend Flow: FromError(bool) {
  let from_error(move error: bool): Flow = Stop(error)
}

let read(fail: bool): Flow = if fail { Stop(true) } else { Value(40) }

let compute(fail: bool): Flow = read(fail).try + 2

let explicit_return(): Flow = return 42

let raised(): Flow = throw true

let main(): i32 = {
  let success = compute(false) match { Value(value) => value, Stop(_) => 0 }
  let propagated = compute(true) match { Value(_) => false, Stop(error) => error }
  let returned = explicit_return() match { Value(value) => value, Stop(_) => 0 }
  let thrown = raised() match { Value(_) => false, Stop(error) => error }
  if success == 42 && propagated && returned == 42 && thrown { 42 } else { 0 }
}

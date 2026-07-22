use core.effects.{Throws, Async}

let fail_with_answer(): Never with(Throws(i32)) = {
  Throws(i32).raise(42)
}

let fail_with_throw_sugar(): Never with(Throws(i32)) = {
  throw 42
}

let handled_throw(): i32 = {
  Throws(i32).handle(
    raise: { (error) -> error },
  ) {
    fail_with_answer()
  }
}

let handled_throw_sugar_function(): i32 = {
  Throws(i32).handle(
    raise: { (error) -> error },
  ) {
    fail_with_throw_sugar()
  }
}

let handled_throw_sugar_action(): i32 = {
  Throws(i32).handle(
    raise: { (error) -> error },
  ) {
    throw 42
  }
}

let tried_throw_sugar_function(): i32 = {
  let result: Result(i32, i32) = try {
    fail_with_throw_sugar()
  }
  result match {
    Ok(value) => value,
    Err(error) => error,
  }
}

let tried_throw_sugar_action(): i32 = {
  let result: Result(i32, i32) = try {
    throw 42
  }
  result match {
    Ok(value) => value,
    Err(error) => error,
  }
}

let handled_async(): i32 = {
  let mut seen = 0
  let value = Async.handle(
    suspend: { (resume) ->
      seen = 1;
      resume(())
    },
  ) {
    Async.suspend();
    1
  }
  value + seen + 40
}

let main(): i32 = {
  handled_throw() + handled_throw_sugar_function() + handled_throw_sugar_action() + tried_throw_sugar_function() + tried_throw_sugar_action() + handled_async() - 210
}

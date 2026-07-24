let Result = std.Result

let Throws = std.effect.Throws
let Async = std.effect.Async

let fail_with_answer(): Never with(Throws(i32)) = {
  Throws(i32).raise(42)
}

let fail_with_throw_sugar(): Never with(Throws(i32)) = {
  throw(42)
}

let choose_with_throw_sugar(fail: bool): i32 with(Throws(i32)) = {
  if fail { throw(42) } else { 1 }
}

let handled_throw(): i32 = {
  Throws(i32).handle raise { (error) -> error } action {
    fail_with_answer()
  }
}

let handled_throw_sugar_function(): i32 = {
  Throws(i32).handle raise { (error) -> error } action {
    fail_with_throw_sugar()
  }
}

let handled_throw_sugar_action(): i32 = {
  Throws(i32).handle raise { (error) -> error } action {
    throw(42)
  }
}

let tried_throw_sugar_function(): i32 = {
  let result: Result(i32)(i32) = try {
    fail_with_throw_sugar()
  }
  match result
    { Ok(value) -> value }
    { Err(error) -> error }
}

let tried_throw_sugar_action(): i32 = {
  let result: Result(i32)(i32) = try {
    throw(42)
  }
  match result
    { Ok(value) -> value }
    { Err(error) -> error }
}

let inferred_try_from_throw_sugar_function(): i32 = {
  let result = try {
    choose_with_throw_sugar(true)
  }
  match result
    { Ok(value) -> value }
    { Err(error) -> error }
}

let handled_async(): i32 = {
  let mut seen = 0
  let value = Async.handle suspend { (resume) ->
      seen = 1;
      resume(())
    } action {
    Async.suspend();
    1
  }
  value + seen + 40
}

let main(): i32 = {
  handled_throw() + handled_throw_sugar_function() + handled_throw_sugar_action() + tried_throw_sugar_function() + tried_throw_sugar_action() + inferred_try_from_throw_sugar_function() + handled_async() - 252
}

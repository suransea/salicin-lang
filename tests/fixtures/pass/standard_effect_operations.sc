let result = std.result

let throws = std.error.throws
let async_effect = std.async.async_effect

let fail_with_answer(): never with(throws(i32)) = {
  throws(i32).raise(42)
}

let fail_with_throw_sugar(): never with(throws(i32)) = {
  throw(42)
}

let choose_with_throw_sugar(fail: bool): i32 with(throws(i32)) = {
  if fail { throw(42) } else { 1 }
}

let handled_throw(): i32 = {
  throws(i32).handle raise { (error) -> error } action {
      fail_with_answer()
    }
}

let handled_throw_sugar_function(): i32 = {
  throws(i32).handle raise { (error) -> error } action {
      fail_with_throw_sugar()
    }
}

let handled_throw_sugar_action(): i32 = {
  throws(i32).handle raise { (error) -> error } action {
      throw(42)
    }
}

let tried_throw_sugar_function(): i32 = {
  let result: result(i32)(i32) = try {
    fail_with_throw_sugar()
  }
  match result
    { ok(value) -> value }
    { err(error) -> error }
}

let tried_throw_sugar_action(): i32 = {
  let result: result(i32)(i32) = try {
    throw(42)
  }
  match result
    { ok(value) -> value }
    { err(error) -> error }
}

let inferred_try_from_throw_sugar_function(): i32 = {
  let result = try {
    choose_with_throw_sugar(true)
  }
  match result
    { ok(value) -> value }
    { err(error) -> error }
}

let handled_async(): i32 = {
  let mut seen = 0
  let value = async_effect.handle suspend { (resume) ->
      seen = 1;
      resume(())
    } action {
      async_effect.suspend();
      1
    }
  value + seen + 40
}

let main(): i32 = {
  handled_throw() + handled_throw_sugar_function() + handled_throw_sugar_action() + tried_throw_sugar_function() + tried_throw_sugar_action() + inferred_try_from_throw_sugar_function() + handled_async() - 252
}

test("standard_effect_operations.sc") {
  main() == 42
}

let option = core.option
let result = core.result
let unsafety = core.unsafe.unsafety

let add_one(value: i32): i32 = { value + 1 }
let keep(value: i32): option(i32) = { option.some(value) }
let keep_result(value: i32): result(bool)(i32) = { result.ok(value) }
let map_error(value: bool): i32 = { if value { 1 } else { 0 } }
let option_fallback(): i32 = { 10 }
let result_fallback(error: bool): i32 = { if error { 11 } else { 10 } }
let make_error(): bool = { true }
let impossible(): i32 with(unsafety) = { unsafe { raw_trap() } }
let impossible_error(error: bool): i32 with(unsafety) = {
  unsafe { raw_trap() }
}

let main(): i32 = { unsafe {
  let mut maybe = option.some(1)
  let mut outcome: result(bool)(i32) = result.ok(2)

  do {
    let view = maybe.as_ref(mut)()
    match view
      { some(value) -> value = value + 1 }
      { none -> () }
  }
  do {
    let view = outcome.as_ref(mut)()
    match view
      { ok(value) -> value = value + 1 }
      { err(_) -> () }
  }

  let borrowed =
    match maybe.as_ref()
      { some(value) -> value }
      { none -> 0 } +
    match outcome.as_ref()
      { ok(value) -> value }
      { err(_) -> 0 }
  let states =
    if maybe.is_some() && !maybe.is_none() &&
       outcome.is_ok() && !outcome.is_err() { 1 } else { 0 }
  let mapped = option.some(1).map(add_one).and_then(keep).unwrap_or(0)
  let mapped_result =
    result(bool)(i32).ok(2).map(add_one).and_then(keep_result).unwrap_or(0)
  let mapped_error =
    result(bool)(i32).err(true).map_error(map_error).err().unwrap_or(0)
  let eager_option = option.some(4).unwrap_or_else(impossible)
  let eager_result = result(bool)(i32).ok(5).unwrap_or_else(impossible_error)
  let lazy_option = option(i32).none.unwrap_or_else(option_fallback)
  let lazy_result = result(bool)(i32).err(false).unwrap_or_else(result_fallback)
  let eager_error = option(i32).none.ok_or(true).err().unwrap_or(false)
  let lazy_error = option(i32).none.ok_or_else(make_error).err().unwrap_or(false)
  let success = result(bool)(i32).ok(6).ok().unwrap_or(0)
  let error = result(bool)(i32).err(true).err().unwrap_or(false)

  borrowed + states + mapped + mapped_result + mapped_error +
    eager_option + eager_result + lazy_option + lazy_result +
    (if eager_error { 1 } else { 0 }) +
    (if lazy_error { 1 } else { 0 }) +
    success + (if error { 1 } else { 0 }) - 8
} }

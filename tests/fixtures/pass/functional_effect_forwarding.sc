let option = std.option
let unsafety = std.unsafe.unsafety
let applicative = std.functional.applicative
let functor = std.functional.functor
let monad = std.functional.monad

let unsafe_add_one(value: i32): i32 with(unsafety) = {
  value + 1
}

let unsafe_next(value: i32): option(i32) with(unsafety) = {
  option(i32).some(value + 2)
}

let read_option(value: option(i32)): i32 = {
  match value
    { some(number) -> number }
    { none -> 0 }
}

let main(): i32 = {
  let mapped = unsafe {
    option(i32).some(40).map(unsafe_add_one)
  }
  let applied = unsafe {
    let transform: option((i32): i32 with(unsafety)) = option.some(unsafe_add_one)
    transform.apply(option(i32).some(1))
  }
  let chained = unsafe {
    option(i32).some(40).flat_map(unsafe_next)
  }
  read_option(mapped) + read_option(applied) + read_option(chained) - 43
}

test("functional_effect_forwarding.sc") {
  main() == 42
}

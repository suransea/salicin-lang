let identity(comptime t: type)(move value: t): t = { value }

let through_closure(comptime t: type)(move value: t): t = {
  let apply = { (item: t) -> identity(t)(item) }
  apply(value)
}

let main(): i32 = { through_closure(i32)(42) }

test("generic_call_inside_closure.sc") {
  main() == 42
}

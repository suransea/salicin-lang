let make(comptime t: type)(value: t): t = { value }
let subtract(left: i32, right: i32): i32 = { left - right }

let main(): i32 = {
  let inferred = make(value: subtract(left: 44, right: 2))
  make(t: i32)(value: inferred)
}

test("infer_named_arguments.sc") {
  main() == 42
}

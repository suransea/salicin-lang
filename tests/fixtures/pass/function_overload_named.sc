let choose(left: i32): i32 = { left + 1 }
let choose(right: i32): i32 = { right + 2 }

let main(): i32 = { choose(right: 40) }

test("function_overload_named.sc") {
  std.test.assert(main() == 42)
}

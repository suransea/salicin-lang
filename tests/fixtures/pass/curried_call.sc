let add(x: i32)(y: i32): i32 = { x + y }

let main(): i32 = { add(20)(22) }

test("curried_call.sc") {
  std.test.assert(main() == 42)
}

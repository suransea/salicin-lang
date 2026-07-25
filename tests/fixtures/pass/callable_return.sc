let add(left: i32)(right: i32): i32 = { left + right }

let make() = {
  let pending = add(40)
  pending
}

let main(): i32 = {
  let pending = make()
  pending(2)
}

test("callable_return.sc") {
  main() == 42
}

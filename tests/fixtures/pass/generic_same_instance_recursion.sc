let bounce(comptime t: type)(move value: t)(again: bool): t = {
  if again {
    bounce(t)(value)(false)
  } else {
    value
  }
}

let main(): i32 = { bounce(i32)(42)(true) }

test("generic_same_instance_recursion.sc") {
  main() == 42
}

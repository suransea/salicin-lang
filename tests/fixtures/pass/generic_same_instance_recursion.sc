let bounce(T: type)(move value: T)(again: bool): T = { if again {
  bounce(T)(value)(false)
} else {
  value
}
}

let main(): i32 = { bounce(i32)(42)(true) }

test("generic_same_instance_recursion.sc") {
  main() == 42
}

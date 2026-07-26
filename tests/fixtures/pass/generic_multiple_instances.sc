let identity(T: type)(move value: T): T = { value }

let main(): i32 = {
  if identity(bool)(true) {
    identity(i32)(42)
  } else {
    0
  }
}

test("generic_multiple_instances.sc") {
  main() == 42
}

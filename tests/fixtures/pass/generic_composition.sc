let identity(T: type)(move value: T): T = { value }

let wrap(T: type)(move value: T): T = { identity(T)(value) }

let main(): i32 = { wrap(i32)(42) }

test("generic_composition.sc") {
  main() == 42
}

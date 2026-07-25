let identity(T: type)(move value: T): T = { value }

let main(): i32 = { identity(42) }

test("infer_generic_function.sc") {
  main() == 42
}

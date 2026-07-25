let Cell(T: type) = struct { value: T }

let main(): i32 = { Cell { value: 42 }.value }

test("infer_generic_struct.sc") {
  main() == 42
}

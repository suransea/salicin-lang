let identity(T: type)(move value: T): T = { value }

let main(): i32 = {
  identity(do { let value = 42; value });
  42
}

test("infer_nonempty_block.sc") {
  main() == 42
}

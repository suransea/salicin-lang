let observe(value: borrow(())): () = { value }

let main(): i32 = {
  let unit = ()
  observe(unit)
  42
}

test("borrowed_unit_is_abi_erased.sc") {
  std.test.assert(main() == 42)
}

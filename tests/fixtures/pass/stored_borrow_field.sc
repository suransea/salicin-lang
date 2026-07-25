let Holder(R: region) = struct { value: borrow(R)(i32) }
let main(): i32 = { 42 }

test("stored_borrow_field.sc") {
  main() == 42
}

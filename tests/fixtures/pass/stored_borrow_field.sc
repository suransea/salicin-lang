let holder(comptime r: region) = struct { value: borrow(r)(i32) }
let main(): i32 = { 42 }

test("stored_borrow_field.sc") {
  main() == 42
}

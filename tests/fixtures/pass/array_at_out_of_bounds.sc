let read(value: borrow(i32)): i32 = { value }

let main(): i32 = {
  let values = [20, 22]
  let value = values.at(2)
  read(value)
}

test("array_at_out_of_bounds.sc") {
  main() == 42
}

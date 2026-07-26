let number = struct { value: i32 }

let add(value: i32)(number: borrow(number)): i32 = { value + number.value }

let main(): i32 = {
  let add_number = add(20)
  add_number(number { value: 22 })
}

test("temporary_borrow_partial_call.sc") {
  main() == 42
}

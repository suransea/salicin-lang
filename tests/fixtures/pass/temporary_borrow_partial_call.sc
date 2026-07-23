let Number = struct { value: i32 }

let add(value: i32)(number: borrow(Number)): i32 = { value + number.value }

let main(): i32 = {
  let add_number = add(20)
  add_number(Number { value: 22 })
}

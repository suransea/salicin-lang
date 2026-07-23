let Number = struct { value: i32 }

let add(number: borrow(Number))(value: i32): i32 = { number.value + value }

let main(): i32 = {
  let partial = add(Number { value: 20 })
  partial(22)
}

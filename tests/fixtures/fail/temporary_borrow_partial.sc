let number = struct { value: i32 }

let add(number: borrow(number))(value: i32): i32 = { number.value + value }

let main(): i32 = {
  let partial = add(number { value: 20 })
  partial(22)
}

let Number = struct(value: i32)

let first(borrow(mut) order: i32): Number = {
  order = order * 10 + 1
  Number(20)
}

let second(borrow(mut) order: i32): Number = {
  order = order * 10 + 2
  Number(22)
}

let combine(move left: Number, borrow right: Number): i32 = { left.value + right.value }

let main(): i32 = {
  let mut order = 0
  let result = combine(first(order), second(order))
  if order == 12 { result } else { 0 }
}

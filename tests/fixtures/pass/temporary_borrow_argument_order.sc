let Number = struct { value: i32 }

let first(order: borrow(mut)(i32)): Number = {
  order = order * 10 + 1
  Number { value: 20 }
}

let second(order: borrow(mut)(i32)): Number = {
  order = order * 10 + 2
  Number { value: 22 }
}

let combine(move left: Number, right: borrow(Number)): i32 = { left.value + right.value }

let main(): i32 = {
  let mut order = 0
  let result = combine(first(order), second(order))
  if order == 12 { result } else { 0 }
}

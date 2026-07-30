let number = struct { value: i32 }

let first(order: borrow(mut)(i32)): number = {
  order = order * 10 + 1
  number { value: 20 }
}

let second(order: borrow(mut)(i32)): number = {
  order = order * 10 + 2
  number { value: 22 }
}

let combine(move left: number, right: borrow(number)): i32 = { left.value + right.value }

let main(): i32 = {
  let mut order = 0
  let result = combine(first(order), second(order))
  if order == 12 { result } else { 0 }
}

test("temporary_borrow_argument_order.sc") {
  std.test.assert(main() == 42)
}

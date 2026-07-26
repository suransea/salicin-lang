let ask = effect {
  let value(): i32
}

let leak(value: borrow(mut)(i32)): (): i32 with(ask) = {
  let mut action: (): i32 with(ask) = { () ->
    value = value + 1
    ask.value() + value
  }
  action
}

let main(): i32 = {
  42
}

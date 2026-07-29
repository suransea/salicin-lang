let ask = effect {
  let value(): i32
}

let leak: with(ask)(value: borrow(mut)(i32)): (): i32 = {
  let mut action: with(ask)((): i32)  = { () ->
    value = value + 1
    ask.value() + value
  }
  action
}

let main(): i32 = {
  42
}

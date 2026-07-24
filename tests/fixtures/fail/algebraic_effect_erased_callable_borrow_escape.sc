let Ask = effect {
  let value(): i32
}

let leak(value: borrow(mut)(i32)): (): i32 with(Ask) = {
  let mut action: (): i32 with(Ask) = { () ->
    value = value + 1
    Ask.value() + value
  }
  action
}

let main(): i32 = {
  42
}

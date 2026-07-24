let Ask = effect {
  let value(): i32
}

let main(): i32 = {
  Ask.handle value { (resume) -> resume(10) } action {
    let mut total = 0
    let mut action: (i32): i32 with(Ask) = { (value: i32) ->
      total = total + value
      Ask.value() + total
    }
    let first = action(1)
    let second = action(2)
    first + second + 18
  }
}

let Ask = effect {
  let value(): i32
}

let main(): i32 = {
  Ask.handle value { (resume) -> resume(10) } action {
    let mut left_total = 0
    let mut right_total = 20
    let mut left: (i32): i32 with(Ask) = { (value: i32) ->
      left_total = left_total + value
      Ask.value() + left_total
    }
    let mut right: (i32): i32 with(Ask) = { (value: i32) ->
      right_total = right_total + value
      Ask.value() + right_total
    }
    let mut action: (i32): i32 with(Ask) = if true { left } else { right }
    let first = action(1)
    let second = action(2)
    first + second + 18
  }
}

let Ask = effect {
  let choose(): bool
  let value(): i32
}

let main(): i32 = {
  Ask.handle choose { (resume) -> resume(false) } value { (resume) -> resume(10) } action {
    let mut left_total = 0
    let mut middle_total = 10
    let mut right_total = 20
    let mut left: (i32): i32 with(Ask) = { (value: i32) ->
      left_total = left_total + value
      Ask.value() + left_total
    }
    let mut middle: (i32): i32 with(Ask) = { (value: i32) ->
      middle_total = middle_total + value
      Ask.value() + middle_total
    }
    let mut right: (i32): i32 with(Ask) = { (value: i32) ->
      right_total = right_total + value
      Ask.value() + right_total
    }
    let first: (i32): i32 with(Ask) = if true { left } else { middle }
    let second: (i32): i32 with(Ask) = if false { middle } else { right }
    let mut action: (i32): i32 with(Ask) = if Ask.choose() { first } else { second }
    let first_result = action(1)
    let second_result = action(2)
    first_result + second_result - 22
  }
}

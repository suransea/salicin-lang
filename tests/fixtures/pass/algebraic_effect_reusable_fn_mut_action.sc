let Ask = effect {
  let value(): i32
}

let run(move action: (i32): i32 with(Ask))(input: i32): i32 = {
  Ask.handle value { (resume) -> resume(10) } action {
    action(input)
  }
}

let main(): i32 = {
  let mut total = 0
  let mut action: (i32): i32 with(Ask) = { (input: i32) ->
    total = total + input
    Ask.value() + total
  }
  let mut alias = action
  let padding = 30
  let result = 1 + run(alias)(1) - 1
  result + total + padding
}

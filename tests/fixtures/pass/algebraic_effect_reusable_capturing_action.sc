let Ask = effect {
  let value(): i32
}

let run(move action: (i32): i32 with(Ask))(input: i32): i32 = {
  Ask.handle value { (resume) -> resume(10) } action {
    action(input)
  }
}

let main(): i32 = {
  let base = 30
  let action: (i32): i32 with(Ask) = { (input: i32) ->
    Ask.value() + input + base
  }
  run(action)(2)
}

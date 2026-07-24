let Ask = effect {
  let value(): i32
}

let invoke(action: (i32): i32 with(Ask))(input: i32): i32 with(Ask) = {
  action(input)
}

let main(): i32 = {
  Ask.handle value { (resume) -> resume(20) } action {
    let offset = 2
    let action: (i32): i32 with(Ask) = { (input: i32) ->
      Ask.value() + input + offset
    }
    invoke(action)(20)
  }
}

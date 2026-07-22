let Ask = effect {
  let value(): i32
}

let run(move action: (i32): i32 with(Ask))(input: i32): i32 = {
  Ask.handle(value: { (resume) -> resume(10) }) {
    action(input) + 31
  }
}

let main(): i32 = {
  let mut total = 0
  let mut action: (i32): i32 with(Ask) = { (input: i32) ->
    total = total + input
    Ask.value() + total
  }
  run(action)(1)
}

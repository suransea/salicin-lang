let Ask = effect {
  let value(): i32
}

let apply_twice(move action: (): i32 with(Ask)): i32 with(Ask) = {
  action() + action()
}

let run(move action: (): i32 with(Ask)): i32 = {
  Ask.handle value { (resume) -> resume(21) } action {
    apply_twice(action)
  }
}

let main(): i32 = {
  let action: (): i32 with(Ask) = { () ->
    Ask.value()
  }
  run(action)
}

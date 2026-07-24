let Ask = effect {
  let value(): i32
}

let apply(action: (): i32 with(Ask)): i32 with(Ask) = {
  action()
}

let run(move action: (): i32 with(Ask)): i32 = {
  Ask.handle value { (resume) -> resume(42) } action {
    apply(action)
  }
}

let main(): i32 = {
  let action: (): i32 with(Ask) = { () ->
    Ask.value()
  }
  run(action)
}

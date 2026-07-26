let Ask = effect {
  let value(): i32
}

let State = struct {
  value: i32,
}

let run(state: borrow(mut)(State))(move action: (): i32 with(Ask)): i32 = {
  Ask.handle value { (resume) -> resume(1) } action {
      action() + state.value
    }
}

let main(): i32 = {
  let mut state = State { value: 20 }
  run(state) { () ->
      state.value = state.value + 1
      Ask.value() + state.value
    }
}

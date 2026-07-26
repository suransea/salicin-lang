let ask = effect {
  let value(): i32
}

let state = struct {
  value: i32,
}

let run(state: borrow(mut)(state))(move action: (): i32 with(ask)): i32 = {
  ask.handle value { (resume) -> resume(1) } action {
      action() + state.value
    }
}

let main(): i32 = {
  let mut state = state { value: 20 }
  run(state) { () ->
      state.value = state.value + 1
      ask.value() + state.value
    }
}

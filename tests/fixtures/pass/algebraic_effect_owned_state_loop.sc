let Step = effect {
  let delta(): i32
}

let State = struct {
  value: i32,
}

let program(): i32 with(Step) = {
  let mut state = State { value: 40 }
  let mut count = 0
  while { count < 2 } do {
    let delta = Step.delta()
    state.value = state.value + delta
    count = count + 1
  }
  state.value
}

let main(): i32 = {
  Step.handle delta { (resume) ->
    resume(1)
  } action {
    program()
  }
}

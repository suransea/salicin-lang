let step = effect {
  let delta(): i32
}

let state = struct {
  value: i32,
}

let program(): i32 with(step) = {
  let mut state = state { value: 40 }
  let mut count = 0
  while { count < 2 } do {
    let delta = step.delta()
    state.value = state.value + delta
    count = count + 1
  }
  state.value
}

let main(): i32 = {
  step.handle delta { (resume) ->
      resume(1)
    } action {
      program()
    }
}

test("algebraic_effect_owned_state_loop.sc") {
  main() == 42
}

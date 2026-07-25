let Step = effect {
  let delta(): i32
}

let update(value: borrow(mut)(i32)): () with(Step) = {
  let delta = Step.delta()
  value = value + delta
}

let program(index: i32): i32 with(Step) = {
  let mut values = [40]
  update(values[index])
  values[0]
}

let main(): i32 = {
  Step.handle delta { (resume) ->
    resume(2)
  } action {
    program(1)
  }
}

test("algebraic_effect_owned_index_oob.sc") {
  main() == 42
}

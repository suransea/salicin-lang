let step = effect {
  let delta(): i32
}

let update: with(step)(value: borrow(mut)(i32)): () = {
  let delta = step.delta()
  value = value + delta
}

let program: with(step)(index: usize): i32 = {
  let mut values = [40]
  update(values[index])
  values[0]
}

let main(): i32 = {
  step.handle delta { (resume) ->
      resume(2)
    } action {
      program(1)
    }
}

test("algebraic_effect_owned_index_oob.sc") {
  main() == 42
}

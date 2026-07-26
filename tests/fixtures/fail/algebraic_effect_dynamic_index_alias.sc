let step = effect {
  let tick(): ()
}

let update(left: borrow(mut)(i32), right: borrow(mut)(i32)): () with(step) = {
  step.tick()
  left = left + 1
  right = right + 1
}

let main(): i32 = {
  let mut values = [20, 20]
  let left: i32 = 0
  let right: i32 = 1
  step.handle tick { (resume) ->
      resume(())
    } action {
      update(values[left], values[right])
      values[0] + values[1]
    }
}

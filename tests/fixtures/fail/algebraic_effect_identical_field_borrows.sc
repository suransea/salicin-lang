let step = effect {
  let tick(): ()
}

let pair = struct { left: i32, right: i32 }

let update(left: borrow(mut)(i32), right: borrow(mut)(i32)): () with(step) = {
  step.tick()
  left = left + 1
  right = right + 1
}

let main(): i32 = {
  let mut pair = pair { left: 20, right: 20 }
  step.handle tick { (resume) ->
      resume(())
    } action {
      update(pair.left, pair.left)
      pair.left
    }
}

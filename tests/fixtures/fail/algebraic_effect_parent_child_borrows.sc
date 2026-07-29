let step = effect {
  let tick(): ()
}

let pair = struct { left: i32, right: i32 }

let update: with(step)(pair: borrow(mut)(pair), left: borrow(mut)(i32)): () = {
  step.tick()
  pair.right = pair.right + 1
  left = left + 1
}

let main(): i32 = {
  let mut pair = pair { left: 20, right: 20 }
  step.handle tick { (resume) ->
      resume(())
    } action {
      update(pair, pair.left)
      pair.left + pair.right
    }
}

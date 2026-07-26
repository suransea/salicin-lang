let Step = effect {
  let tick(): ()
}

let Pair = struct { left: i32, right: i32 }

let update(pair: borrow(mut)(Pair), left: borrow(mut)(i32)): () with(Step) = {
  Step.tick()
  pair.right = pair.right + 1
  left = left + 1
}

let main(): i32 = {
  let mut pair = Pair { left: 20, right: 20 }
  Step.handle tick { (resume) ->
      resume(())
    } action {
      update(pair, pair.left)
      pair.left + pair.right
    }
}

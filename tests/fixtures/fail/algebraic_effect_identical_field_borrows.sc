let Step = effect {
  let tick(): ()
}

let Pair = struct { left: i32, right: i32 }

let update(left: borrow(mut)(i32), right: borrow(mut)(i32)): () with(Step) = {
  Step.tick()
  left = left + 1
  right = right + 1
}

let main(): i32 = {
  let mut pair = Pair { left: 20, right: 20 }
  Step.handle tick { (resume) ->
      resume(())
    } action {
      update(pair.left, pair.left)
      pair.left
    }
}

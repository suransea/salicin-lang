let step = effect {
  let delta(): i32
}

let state = struct {
  left: i32,
  right: i32,
  drops: ptr(mut)(i32),
}

extend state: droppable {
  let drop(self: borrow(mut)(self))(): () = {
    unsafe {
      *self.drops = *self.drops + 1
    }
  }
}

let update(left: borrow(mut)(i32), right: borrow(mut)(i32)): () with(step) = {
  let delta = step.delta()
  left = left + delta
  right = right + delta
}

let program(drops: ptr(mut)(i32)): i32 with(step) = {
  let mut state = state { left: 20, right: 20, drops: drops }
  update(state.left, state.right)
  state.left + state.right
}

let main(): i32 = {
  let drops = unsafe {
    raw_alloc(i32)(size_of(i32), align_of(i32))
  }
  unsafe { *drops = 0 }

  let resumed = step.handle delta { (resume) ->
      resume(1)
    } action {
      program(drops)
    }
  let abandoned = step.handle delta { (_) ->
      40
    } action {
      program(drops)
    }
  let drop_count = unsafe { *drops }

  unsafe {
    raw_dealloc(drops, size_of(i32), align_of(i32))
  }
  resumed + abandoned + drop_count - 42
}

test("algebraic_effect_disjoint_field_calls.sc") {
  main() == 42
}

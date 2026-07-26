let step = effect {
  let delta(): i32
}

let state = struct {
  value: i32,
  drops: ptr(mut)(i32),
}

extend state: droppable {
  let drop(self: borrow(mut)(self))(): () = {
    unsafe {
      *self.drops = *self.drops + 1
    }
  }
}

let walk(state: borrow(mut)(state), count: i32): i32 with(step) = {
  if count == 0 {
    return(state.value)
  }
  let delta = step.delta()
  state.value = state.value + delta
  let nested = walk(state, count - 1)
  nested + state.value
}

let run(drops: ptr(mut)(i32), abandon: bool): i32 = {
  let mut state = state { value: 18, drops: drops }
  let result = step.handle delta { (resume) ->
      if abandon { 40 } else { resume(1) }
    } action {
      walk(state, 2)
    }
  result + state.value
}

let main(): i32 = {
  let drops = unsafe {
    raw_alloc(i32)(size_of(i32), align_of(i32))
  }
  unsafe { *drops = 0 }

  let resumed = run(drops, false)
  let abandoned = run(drops, true)
  let drop_count = unsafe { *drops }

  unsafe {
    raw_dealloc(drops, size_of(i32), align_of(i32))
  }
  resumed + abandoned + drop_count - 98
}

test("algebraic_effect_owned_recursive_call.sc") {
  main() == 42
}

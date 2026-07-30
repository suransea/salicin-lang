let ask = effect {
  let value(): i32
}

let state = struct {
  left: i32,
  right: i32,
  drops: ptr(mut)(i32),
}

extend(state, droppable) {
  let drop(self: borrow(mut)(self))(): () = {
    unsafe {
      *self.drops = *self.drops + 1
    }
  }
}

let run(
  left: borrow(i32),
  right: borrow(mut)(i32),
  abandon: bool,
)(move action: with(ask)((): i32)): i32 = {
  ask.handle value { (resume) ->
      if abandon { 40 } else { resume(2) }
    } action {
      right = right + action()
      left + right
    }
}

let execute(drops: ptr(mut)(i32), abandon: bool): i32 = {
  let mut state = state { left: 10, right: 20, drops: drops }
  let mut order = 1
  let result = run(state.left, state.right, abandon) { () ->
      order = order * 2
      ask.value() + order
    }
  result + state.left + state.right + order
}

let main(): i32 = {
  let drops = unsafe {
    raw_alloc(i32)(size_of(i32), align_of(i32))
  }
  unsafe { *drops = 0 }

  let resumed = execute(drops, false)
  let abandoned = execute(drops, true)
  let drop_count = unsafe { *drops }

  unsafe {
    raw_dealloc(drops, size_of(i32), align_of(i32))
  }
  resumed + abandoned + drop_count - 102
}

test("algebraic_effect_reusable_borrowed_action.sc") {
  std.test.assert(main() == 42)
}

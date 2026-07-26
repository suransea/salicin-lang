let Ask = effect {
  let value(): i32
}

let State = struct {
  left: i32,
  right: i32,
  drops: Ptr(mut)(i32),
}

extend State: Drop {
  let drop(self: borrow(mut)(Self))(): () = {
    unsafe {
      *self.drops = *self.drops + 1
    }
  }
}

let run(
  left: borrow(i32),
  right: borrow(mut)(i32),
  abandon: bool,
)(move action: (): i32 with(Ask)): i32 = {
  Ask.handle value { (resume) ->
      if abandon { 40 } else { resume(2) }
    } action {
      right = right + action()
      left + right
    }
}

let execute(drops: Ptr(mut)(i32), abandon: bool): i32 = {
  let mut state = State { left: 10, right: 20, drops: drops }
  let mut order = 1
  let result = run(state.left, state.right, abandon) { () ->
      order = order * 2
      Ask.value() + order
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
  main() == 42
}

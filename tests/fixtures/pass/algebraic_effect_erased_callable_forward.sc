let ask = effect {
  let value(): i32
}

let resource = struct {
  bias: i32,
  drops: ptr(mut)(i32),
}

extend(resource, droppable) {
  let drop(self: borrow(mut)(self))(): () = {
    unsafe {
      *self.drops = *self.drops + 1
    }
  }
}

let consume(move resource: resource): i32 = {
  resource.bias
}

let apply: with(ask)(move action: with(ask)((): i32)): i32 = {
  action() + 1
}

let apply_input: with(ask)(seed: i32, move action: with(ask)((i32): i32)): i32 = {
  action(seed) + 1
}

let run(move action: with(ask)((): i32)): i32 = {
  ask.handle value { (resume) -> resume(10) } action {
      apply(action)
    }
}

let outer(move action: with(ask)((): i32), abandon: bool): i32 = {
  ask.handle value { (resume) ->
      if abandon { 40 } else { resume(20) }
    } action {
      run(action)
    }
}

let discard(move action: with(ask)((): i32)): i32 = {
  ask.handle value { (resume) -> resume(0) } action {
      42
    }
}

let run_input(move action: with(ask)((i32): i32)): i32 = {
  ask.handle value { (resume) -> resume(10) } action {
      apply_input(11, action)
    }
}

let outer_input(move action: with(ask)((i32): i32)): i32 = {
  ask.handle value { (resume) -> resume(20) } action {
      run_input(action)
    }
}

let execute(drops: ptr(mut)(i32), abandon: bool): i32 = {
  let resource = resource { bias: 21, drops: drops }
  let mut action: with(ask)((): i32)  = { () ->
    ask.value() + consume(resource)
  }
  outer(action, abandon)
}

let execute_discard(drops: ptr(mut)(i32)): i32 = {
  let resource = resource { bias: 21, drops: drops }
  let action: with(ask)((): i32)  = { () ->
    ask.value() + consume(resource)
  }
  discard(action)
}

let execute_mut(abandon: bool): i32 = {
  let mut order = 1
  let mut action: with(ask)((): i32)  = { () ->
    order = order + 1
    let value = ask.value()
    order = order + 1
    value + order
  }
  outer(action, abandon) + order
}

let execute_input(): i32 = {
  let base = 10
  let action: with(ask)((i32): i32)  = { (input: i32) ->
    ask.value() + input + base
  }
  outer_input(action)
}

let main(): i32 = {
  let drops = unsafe {
    raw_alloc(i32)(size_of(i32), align_of(i32))
  }
  unsafe { *drops = 0 }

  let resumed = execute(drops, false)
  let abandoned = execute(drops, true)
  let discarded = execute_discard(drops)
  let mutated = execute_mut(false)
  let mutated_abandoned = execute_mut(true)
  let with_input = execute_input()
  let drop_count = unsafe { *drops }

  unsafe {
    raw_dealloc(drops, size_of(i32), align_of(i32))
  }
  resumed + abandoned + discarded + mutated + mutated_abandoned + with_input + drop_count - 196
}

test("algebraic_effect_erased_callable_forward.sc") {
  std.test.assert(main() == 42)
}

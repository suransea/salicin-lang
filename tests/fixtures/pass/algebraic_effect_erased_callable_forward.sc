let Ask = effect {
  let value(): i32
}

let Resource = struct {
  bias: i32,
  drops: Ptr(mut)(i32),
}

extend Resource: Drop {
  let drop(self: borrow(mut)(Self))(): () = { unsafe {
    *self.drops = *self.drops + 1
  } }
}

let consume(move resource: Resource): i32 = {
  resource.bias
}

let apply(move action: (): i32 with(Ask)): i32 with(Ask) = {
  action() + 1
}

let apply_input(seed: i32, move action: (i32): i32 with(Ask)): i32 with(Ask) = {
  action(seed) + 1
}

let run(move action: (): i32 with(Ask)): i32 = {
  Ask.handle value { (resume) -> resume(10) } action {
    apply(action)
  }
}

let outer(move action: (): i32 with(Ask), abandon: bool): i32 = {
  Ask.handle value { (resume) ->
    if abandon { 40 } else { resume(20) }
  } action {
    run(action)
  }
}

let discard(move action: (): i32 with(Ask)): i32 = {
  Ask.handle value { (resume) -> resume(0) } action {
    42
  }
}

let run_input(move action: (i32): i32 with(Ask)): i32 = {
  Ask.handle value { (resume) -> resume(10) } action {
    apply_input(11, action)
  }
}

let outer_input(move action: (i32): i32 with(Ask)): i32 = {
  Ask.handle value { (resume) -> resume(20) } action {
    run_input(action)
  }
}

let execute(drops: Ptr(mut)(i32), abandon: bool): i32 = {
  let resource = Resource { bias: 21, drops: drops }
  let mut action: (): i32 with(Ask) = { () ->
    Ask.value() + consume(resource)
  }
  outer(action, abandon)
}

let execute_discard(drops: Ptr(mut)(i32)): i32 = {
  let resource = Resource { bias: 21, drops: drops }
  let action: (): i32 with(Ask) = { () ->
    Ask.value() + consume(resource)
  }
  discard(action)
}

let execute_mut(abandon: bool): i32 = {
  let mut order = 1
  let mut action: (): i32 with(Ask) = { () ->
    order = order + 1
    let value = Ask.value()
    order = order + 1
    value + order
  }
  outer(action, abandon) + order
}

let execute_input(): i32 = {
  let base = 10
  let action: (i32): i32 with(Ask) = { (input: i32) ->
    Ask.value() + input + base
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
  main() == 42
}

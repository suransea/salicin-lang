let abort = effect {
  let stop(): i32
}

let resource = struct { counter: ptr(mut)(i32) }

extend(resource, droppable) {
  let drop(self: borrow(mut)(self))(): () = {
    unsafe {
      *self.counter = *self.counter + 1
    }
  }
}

let consume(move resource: resource): i32 = { 0 }

let run(move action: (): i32 with(abort)): i32 = {
  abort.handle stop { (resume) -> 41 } action {
      action()
    }
}

let execute(counter: ptr(mut)(i32)): i32 = {
  let resource = resource { counter: counter }
  let action: (): i32 with(abort) = { () ->
    abort.stop() + consume(resource)
  }
  let alias = action
  let padding = 0
  let result = run(alias) + padding
  let drops = unsafe { *counter }
  result + drops
}

let main(): i32 = {
  let counter = unsafe {
    raw_alloc(i32)(size_of(i32), align_of(i32))
  }
  unsafe { *counter = 0 }
  let result = execute(counter)
  unsafe {
    raw_dealloc(counter, size_of(i32), align_of(i32))
  }
  result
}

test("algebraic_effect_reusable_fn_once_abort.sc") {
  main() == 42
}

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

let program: with(abort)(counter: ptr(mut)(i32)): i32 = {
  let resource = resource { counter: counter }
  let value = abort.stop()
  value + consume(resource)
}

let main(): i32 = {
  let counter = unsafe {
    raw_alloc(i32)(size_of(i32), align_of(i32))
  }
  unsafe { *counter = 0 }
  let result = abort.handle stop { (resume) -> 41 } action {
      program(counter)
    }
  let drops = unsafe { *counter }
  unsafe {
    raw_dealloc(counter, size_of(i32), align_of(i32))
  }
  result + drops
}

test("algebraic_effect_continuation_drop.sc") {
  std.test.assert(main() == 42)
}

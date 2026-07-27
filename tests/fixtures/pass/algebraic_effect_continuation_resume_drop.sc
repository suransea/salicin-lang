let stop = effect {
  let value(): i32
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

let program(counter: ptr(mut)(i32)): i32 with(stop) = {
  let resource = resource { counter: counter }
  let value = stop.value()
  value + consume(resource)
}

let main(): i32 = {
  let counter = unsafe {
    raw_alloc(i32)(size_of(i32), align_of(i32))
  }
  unsafe { *counter = 0 }
  let result = stop.handle value { (resume) -> resume(41) } action {
      program(counter)
    }
  let drops = unsafe { *counter }
  unsafe {
    raw_dealloc(counter, size_of(i32), align_of(i32))
  }
  result + drops
}

test("algebraic_effect_continuation_resume_drop.sc") {
  main() == 42
}

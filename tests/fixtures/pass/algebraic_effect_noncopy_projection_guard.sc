let check = effect {
  let accept(): bool
}

let resource = struct { counter: ptr(mut)(i32), value: i32 }

extend(resource, droppable) {
  let drop(self: borrow(mut)(self))(): () = {
    unsafe {
      *self.counter = *self.counter + 1
    }
  }
}

let event = enum {
  value( value: resource ),
  empty,
}

let consume(move resource: resource): i32 = { resource.value }

let evaluate(counter: ptr(mut)(i32), accepted: bool): i32 = {
  check.handle accept { (resume) -> resume(accepted) } action {
      let event = event.value( value: resource { counter: counter, value: 20 } )
      match event
        { event.value( value: resource ) if check.accept() && resource.value > 0 -> consume(resource) }
        { event.value( value: resource ) -> consume(resource) }
        { event.empty -> 0 }
    }
}

let main(): i32 = {
  let counter = unsafe {
    raw_alloc(i32)(size_of(i32), align_of(i32))
  }
  unsafe { *counter = 0 }
  let result = evaluate(counter, false) + evaluate(counter, true)
  let drops = unsafe { *counter }
  unsafe {
    raw_dealloc(counter, size_of(i32), align_of(i32))
  }
  result + drops
}

test("algebraic_effect_noncopy_projection_guard.sc") {
  std.test.assert(main() == 42)
}

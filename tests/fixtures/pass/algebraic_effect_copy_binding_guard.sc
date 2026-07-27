let check = effect {
  let accept(): bool
}

let resource = struct { counter: ptr(mut)(i32) }

extend(resource, droppable) {
  let drop(self: borrow(mut)(self))(): () = {
    unsafe {
      *self.counter = *self.counter + 1
    }
  }
}

let event = enum {
  value( value: resource, field1: i32 ),
  empty,
}

let consume(move resource: resource, value: i32): i32 = { value }

let evaluate(counter: ptr(mut)(i32), accepted: bool): i32 = {
  check.handle accept { (resume) -> resume(accepted) } action {
      let event = event.value( value: resource { counter: counter }, field1: 20 )
      match event
        { event.value( value: resource, field1: value ) if check.accept() && value > 0 -> consume(resource, value) }
        { event.value( value: resource, field1: value ) -> consume(resource, value) }
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

test("algebraic_effect_copy_binding_guard.sc") {
  main() == 42
}

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
  value( value: resource ),
  empty,
}

let main(): i32 = {
  let counter = unsafe {
    raw_alloc(i32)(size_of(i32), align_of(i32))
  }
  unsafe { *counter = 0 }
  let result: i32 = check.handle accept { (resume) -> resume(false) } action {
      let event = event.value( value: resource { counter: counter } )
      match event
        { event.value( value: _ ) if check.accept() -> 40 }
        { event.value( value: _ ) -> 41 }
        { event.empty -> 0 }
    }
  let drops = unsafe { *counter }
  unsafe {
    raw_dealloc(counter, size_of(i32), align_of(i32))
  }
  result + drops
}

test("algebraic_effect_noncopy_wildcard_guard.sc") {
  std.test.assert(main() == 42)
}

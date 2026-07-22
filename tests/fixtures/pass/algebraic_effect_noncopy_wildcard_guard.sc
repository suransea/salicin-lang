let Check = effect {
  let accept(): bool
}

let Resource = struct(counter: MutPtr(i32))

extend Resource: Drop {
  let drop(borrow(mut) self)(): () = { unsafe {
    *self.counter = *self.counter + 1
  } }
}

let Event = enum {
  Value(Resource),
  Empty,
}

let main(): i32 = {
  let counter = unsafe {
    raw_alloc(i32)(size_of(i32), align_of(i32))
  }
  unsafe { *counter = 0 }
  let result: i32 = Check.handle(accept: { (resume) -> resume(false) }) {
    let event = Event.Value(Resource(counter))
    event match {
      Event.Value(_) if Check.accept() => 40,
      Event.Value(_) => 41,
      Event.Empty => 0,
    }
  }
  let drops = unsafe { *counter }
  unsafe {
    raw_dealloc(counter, size_of(i32), align_of(i32))
  }
  result + drops
}

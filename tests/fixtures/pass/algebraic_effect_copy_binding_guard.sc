let Check = effect {
  let accept(): bool
}

let Resource = struct { counter: MutPtr(i32) }

extend Resource: Drop {
  let drop(self: borrow(mut)(Self))(): () = { unsafe {
    *self.counter = *self.counter + 1
  } }}

let Event = enum {
  Value( value: Resource, field1: i32 ),
  Empty,
}

let consume(move resource: Resource, value: i32): i32 = { value }

let evaluate(counter: MutPtr(i32), accepted: bool): i32 = {
  Check.handle accept { (resume) -> resume(accepted) } action {
    let event = Event.Value( value: Resource { counter: counter }, field1: 20 )
    match event
      { Event.Value( value: resource, field1: value ) if Check.accept() && value > 0 -> consume(resource, value) }
      { Event.Value( value: resource, field1: value ) -> consume(resource, value) }
      { Event.Empty -> 0 }
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

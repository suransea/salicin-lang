let Check = effect {
  let accept(): bool
}

let Resource = struct { counter: MutPtr(i32), value: i32 }

extend Resource: Drop {
  let drop(self: borrow(mut)(Self))(): () = { unsafe {
    *self.counter = *self.counter + 1
  } }}

let Event = enum {
  Value( value: Resource ),
  Empty,
}

let consume(move resource: Resource): i32 = { resource.value }

let evaluate(counter: MutPtr(i32), accepted: bool): i32 = {
  Check.handle(accept: { (resume) -> resume(accepted) }) {
    let event = Event.Value( value: Resource { counter: counter, value: 20 } )
    event match {
      Event.Value( value: resource ) if Check.accept() && resource.value > 0 => consume(resource),
      Event.Value( value: resource ) => consume(resource),
      Event.Empty => 0,
    }
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

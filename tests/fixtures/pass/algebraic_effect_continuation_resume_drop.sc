let Stop = effect {
  let value(): i32
}

let Resource = struct(counter: MutPtr(i32))

extend Resource: Drop {
  let drop(borrow(mut) self)(): () = { unsafe {
    *self.counter = *self.counter + 1
  } }
}

let consume(move resource: Resource): i32 = { 0 }

let program(counter: MutPtr(i32)): i32 with(Stop) = {
  let resource = Resource(counter)
  let value = Stop.value()
  value + consume(resource)
}

let main(): i32 = {
  let counter = unsafe {
    raw_alloc(i32)(size_of(i32), align_of(i32))
  }
  unsafe { *counter = 0 }
  let result = Stop.handle(
    value: { (resume) -> resume(41) },
  ) {
    program(counter)
  }
  let drops = unsafe { *counter }
  unsafe {
    raw_dealloc(counter, size_of(i32), align_of(i32))
  }
  result + drops
}

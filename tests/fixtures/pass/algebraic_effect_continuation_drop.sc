let Abort = effect {
  let stop(): i32
}

let Resource = struct { counter: MutPtr(i32) }

extend Resource: Drop {
  let drop(self: borrow(mut)(Self))(): () = { unsafe {
    *self.counter = *self.counter + 1
  } }}

let consume(move resource: Resource): i32 = { 0 }

let program(counter: MutPtr(i32)): i32 with(Abort) = {
  let resource = Resource { counter: counter }
  let value = Abort.stop()
  value + consume(resource)
}

let main(): i32 = {
  let counter = unsafe {
    raw_alloc(i32)(size_of(i32), align_of(i32))
  }
  unsafe { *counter = 0 }
  let result = Abort.handle stop { (resume) -> 41 } action {
    program(counter)
  }
  let drops = unsafe { *counter }
  unsafe {
    raw_dealloc(counter, size_of(i32), align_of(i32))
  }
  result + drops
}

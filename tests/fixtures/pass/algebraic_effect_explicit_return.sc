let Read = effect {
  let read(): i32
}

let Resource = struct { counter: MutPtr(i32) }

extend Resource: Drop {
  let drop(self: borrow(mut)(Self))(): () = { unsafe {
    *self.counter = *self.counter + 1
  } }}

let read_early(counter: MutPtr(i32)): i32 with(Read) = {
  let resource = Resource { counter: counter }
  let value = Read.read()
  return value
}

let main(): i32 = {
  let counter = unsafe {
    raw_alloc(i32)(size_of(i32), align_of(i32))
  }
  unsafe { *counter = 0 }
  let result = Read.handle(
    read: { (resume) -> resume(41) },
  ) {
    read_early(counter)
  }
  let drops = unsafe { *counter }
  unsafe {
    raw_dealloc(counter, size_of(i32), align_of(i32))
  }
  result + drops
}

let Payload = struct { left: i32, right: i32 }

extend Payload: Copy {}

let main(): i32 = {
  let pointer = unsafe {
    raw_alloc(Payload)(size_of(Payload), align_of(Payload))
  }
  unsafe {
    *pointer = Payload { left: 40, right: 2 }
  }
  let payload = unsafe {
    *pointer
  }
  unsafe {
    raw_dealloc(pointer, size_of(Payload), align_of(Payload))
  }
  payload.left + payload.right
}

let poll = std.async.poll
let future = std.async.future
let unsafe_effect = std.unsafe.unsafe_effect

let step = struct { counter: ptr(mut)(i32) }
let resource = struct { counter: ptr(mut)(i32) }

extend(step, droppable) {
  let drop(self: borrow(mut)(self))(): () = {
    unsafe {
      *self.counter = *self.counter + 1
    }
  }
}

extend(resource, droppable) {
  let drop(self: borrow(mut)(self))(): () = {
    unsafe {
      *self.counter = *self.counter + 1
    }
  }
}

let consume(move resource: resource): () = { () }

extend(step, future(())) {
  let output = i32

  let poll(comptime r: region)
    (self: borrow(mut)(r)(self))
    (): poll(i32) = {
    poll(i32).ready(40)
  }
}

let allocate(): ptr(mut)(i32) with(unsafe_effect) = {
  unsafe {
    raw_alloc(i32)(size_of(i32), align_of(i32))
  }
}

let release(counter: ptr(mut)(i32)): () with(unsafe_effect) = {
  unsafe {
    raw_dealloc(counter, size_of(i32), align_of(i32))
  }
}

let main(): i32 = {
  unsafe {
    let counter = allocate()
    *counter = 0

    let value = do {
      let resource = resource { counter: counter }
      let mut future = async {
        let value = await step { counter: counter }
        consume(resource)
        value
      }
      match future.poll()
        { pending -> 0 }
        { ready(value) -> value }
    }

    let drops = *counter
    release(counter)
    value + drops
  }
}

test("async_await_complete_drop.sc") {
  main() == 42
}

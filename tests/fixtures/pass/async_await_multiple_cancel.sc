let poll = std.async.poll
let future = std.async.future
let unsafety = std.unsafe.unsafety

let step = struct {
  counter: ptr(mut)(i32),
  polls: i32,
  value: i32
}
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

extend(step, future(())) {
  let output = i32

  let poll(comptime r: region)
    (self: borrow(mut)(r)(self))
    (): poll(i32) = {
    if self.polls == 0 {
      self.polls = 1
      poll(i32).pending
    } else {
      poll(i32).ready(self.value)
    }
  }
}

let consume(move resource: resource): () = { () }

let allocate(): ptr(mut)(i32) with(unsafety) = {
  unsafe {
    raw_alloc(i32)(size_of(i32), align_of(i32))
  }
}

let release(counter: ptr(mut)(i32)): () with(unsafety) = {
  unsafe {
    raw_dealloc(counter, size_of(i32), align_of(i32))
  }
}

let main(): i32 = {
  unsafe {
    let counter = allocate()
    *counter = 0

    do {
      let resource = resource { counter: counter }
      let mut future = async {
        let first = await step { counter: counter, polls: 0, value: 20 }
        let second = await step { counter: counter, polls: 0, value: 22 }
        consume(resource)
        first + second
      }
      match future.poll()
        { pending -> () }
        { ready(_) -> () }
      match future.poll()
        { pending -> () }
        { ready(_) -> () }
    }

    let drops = *counter
    release(counter)
    39 + drops
  }
}

test("async_await_multiple_cancel.sc") {
  main() == 42
}

let poll = core.async.poll
let future = core.async.future
let unsafety = core.unsafe.unsafety

let resource = struct {
  counter: ptr(mut)(i32),
  value: i32
}

extend(resource, droppable) {
  let drop(self: borrow(mut)(self))(): () = {
    unsafe {
      *self.counter = *self.counter + 1
    }
  }
}

let step = struct {
  counter: ptr(mut)(i32),
  polled: bool
}

extend(step, droppable) {
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
    if self.polled {
      poll(i32).ready(0)
    } else {
      self.polled = true
      poll(i32).pending
    }
  }
}

let allocate: with(unsafety)(): ptr(mut)(i32) = {
  unsafe {
    raw_alloc(i32)(size_of(i32), align_of(i32))
  }
}

let release: with(unsafety)(counter: ptr(mut)(i32)): () = {
  unsafe {
    raw_dealloc(counter, size_of(i32), align_of(i32))
  }
}

let main(): i32 = {
  unsafe {
    let counter = allocate()
    *counter = 0
    let mut future = async {
      let resource = resource { counter: counter, value: 40 }
      let awaited = await step { counter: counter, polled: false }
      resource.value + awaited
    }
    let pending = match future.poll()
      { pending -> 0 }
      { ready(_) -> 100 }
    let result = match future.poll()
      { ready(value) -> value }
      { pending -> 100 }

    do {
      let mut cancelled = async {
        let resource = resource { counter: counter, value: 0 }
        let awaited = await step { counter: counter, polled: false }
        resource.value + awaited
      }
      match cancelled.poll()
        { pending -> () }
        { ready(_) -> () }
    }

    let drops = *counter
    release(counter)
    pending + result + drops - 2
  }
}

test("async_await_retained_resource_drop.sc") {
  main() == 42
}

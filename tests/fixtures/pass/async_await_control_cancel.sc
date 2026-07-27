let poll = core.async.poll
let future = core.async.future
let unsafety = core.unsafe.unsafety

let first = struct {
  counter: ptr(mut)(i32)
}

let second = struct {
  counter: ptr(mut)(i32)
}

let marker = struct {
  counter: ptr(mut)(i32),
  amount: i32
}

extend(marker, droppable) {
  let drop(self: borrow(mut)(self))(): () = {
    unsafe {
      *self.counter = *self.counter + self.amount
    }
  }
}

extend(first, droppable) {
  let drop(self: borrow(mut)(self))(): () = {
    unsafe {
      *self.counter = *self.counter + 10
    }
  }
}

extend(second, droppable) {
  let drop(self: borrow(mut)(self))(): () = {
    unsafe {
      *self.counter = *self.counter + 1
    }
  }
}

extend(first, future(())) {
  let output = i32

  let poll(comptime r: region)
    (self: borrow(mut)(r)(self))
    (): poll(i32) = {
    poll(i32).pending
  }
}

extend(second, future(())) {
  let output = i32

  let poll(comptime r: region)
    (self: borrow(mut)(r)(self))
    (): poll(i32) = {
    poll(i32).pending
  }
}

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
      let mut future = async {
        if false {
          let marker = marker { counter: counter, amount: 1000 }
          await first { counter: counter }
        } else {
          let marker = marker { counter: counter, amount: 100 }
          await second { counter: counter }
        }
      }
      match future.poll()
        { pending -> () }
        { ready(_) -> () }
    }
    let drops = *counter
    release(counter)
    drops - 59
  }
}

test("async_await_control_cancel.sc") {
  main() == 42
}

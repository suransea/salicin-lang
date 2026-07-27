let poll = core.async.poll
let future = core.async.future

let resource = struct {
  drops: ptr(mut)(i32)
}

extend(resource, droppable) {
  let drop(self: borrow(mut)(self))(): () = {
    unsafe {
      *self.drops = *self.drops + 1
    }
  }
}

let step = struct {
  polled: bool,
  remaining: ptr(mut)(i32)
}

extend(step, future(())) {
  let output = bool

  let poll(comptime r: region)
    (self: borrow(mut)(r)(self))
    (): poll(bool) = {
    if self.polled {
      let done = unsafe {
        *self.remaining = *self.remaining - 1
        *self.remaining == 0
      }
      poll(bool).ready(done)
    } else {
      self.polled = true
      poll(bool).pending
    }
  }
}

let step(remaining: ptr(mut)(i32)): step = {
  step { polled: false, remaining: remaining }
}

let consume(move first: resource, move second: resource): i32 = {
  38
}

let main(): i32 = {
  let mut drops = 0
  let drops_ptr = ptr(mut)(borrow(mut)(drops))
  let output = do {
    let first_resource = resource { drops: drops_ptr }
    let second_resource = resource { drops: drops_ptr }
    let mut remaining = 2
    let remaining_ptr = ptr(mut)(borrow(mut)(remaining))
    let mut future = async {
      loop {
        let done = await step(remaining_ptr)
        if done {
          break(consume(first_resource, second_resource))
        } else {
          continue()
        }
      }
    }
    match future.poll()
      { pending -> () }
      { ready(_) -> () }
    match future.poll()
      { pending -> () }
      { ready(_) -> () }
    match future.poll()
      { pending -> 0 }
      { ready(value) -> value }
  }

  do {
    let first_resource = resource { drops: drops_ptr }
    let second_resource = resource { drops: drops_ptr }
    let mut remaining = 2
    let remaining_ptr = ptr(mut)(borrow(mut)(remaining))
    let mut cancelled = async {
      loop {
        let done = await step(remaining_ptr)
        if done {
          break(consume(first_resource, second_resource))
        } else {
          continue()
        }
      }
    }
    match cancelled.poll()
      { pending -> () }
      { ready(_) -> () }
  }

  output + unsafe { *drops_ptr }
}

test("async_await_loop_move_carry.sc") {
  main() == 42
}

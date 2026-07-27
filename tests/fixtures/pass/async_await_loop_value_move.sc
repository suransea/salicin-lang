let poll = std.async.poll
let future = std.async.future

let marker = struct {
  drops: ptr(mut)(i32)
}

extend(marker, droppable) {
  let drop(self: borrow(mut)(self))(): () = {
    unsafe {
      *self.drops = *self.drops + 1
    }
  }
}

let step = struct {
  drops: ptr(mut)(i32)
}

extend(step, future(())) {
  let output = marker

  let poll(comptime r: region)
    (self: borrow(mut)(r)(self))
    (): poll(marker) = {
    poll(marker).ready(marker { drops: self.drops })
  }
}

let step(drops: ptr(mut)(i32)): step = {
  step { drops: drops }
}

let main(): i32 = {
  let mut remaining = 2
  let mut drops = 0
  let remaining_ptr = ptr(mut)(borrow(mut)(remaining))
  let drops_ptr = ptr(mut)(borrow(mut)(drops))
  let mut future = async {
    loop {
      let marker = await step(drops_ptr)
      if unsafe {
        *remaining_ptr = *remaining_ptr - 1
        *remaining_ptr == 0
      } {
        break(marker)
      } else {
        continue()
      }
    }
  }

  match future.poll()
    { pending -> () }
    { ready(marker) -> () }
  40 + unsafe { *drops_ptr }
}

test("async_await_loop_value_move.sc") {
  main() == 42
}

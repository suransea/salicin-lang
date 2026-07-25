let Poll = std.async.Poll
let Future = std.async.Future

let Step = struct {
  remaining: Ptr(mut)(i32)
}

extend Step: Future(()) {
  let Output = i32

  let poll(R: region)
    (self: borrow(mut)(R)(Self))
    (): Poll(i32) = {
    let value = unsafe {
      *self.remaining = *self.remaining - 1
      *self.remaining
    }
    Poll(i32).Ready(value)
  }
}

let step(remaining: Ptr(mut)(i32)): Step = {
  Step { remaining: remaining }
}

let main(): i32 = {
  let mut remaining = 3
  let remaining_ptr = Ptr(mut)(borrow(mut)(remaining))
  let mut future = async {
    loop {
      let value = await step(remaining_ptr)
      if value == 0 {
        break(value + 42)
      } else {
        continue()
      }
    }
  }

  match future.poll()
    { Pending -> 0 }
    { Ready(value) -> value }
}

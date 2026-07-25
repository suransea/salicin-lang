let Poll = std.async.Poll
let Future = std.async.Future

let Marker = struct {
  drops: Ptr(mut)(i32)
}

extend Marker: Drop {
  let drop(self: borrow(mut)(Self))(): () = { unsafe {
    *self.drops = *self.drops + 1
  } }
}

let Step = struct {
  drops: Ptr(mut)(i32)
}

extend Step: Future(()) {
  let Output = Marker

  let poll(R: region)
    (self: borrow(mut)(R)(Self))
    (): Poll(Marker) = {
    Poll(Marker).Ready(Marker { drops: self.drops })
  }
}

let step(drops: Ptr(mut)(i32)): Step = {
  Step { drops: drops }
}

let main(): i32 = {
  let mut remaining = 2
  let mut drops = 0
  let remaining_ptr = Ptr(mut)(borrow(mut)(remaining))
  let drops_ptr = Ptr(mut)(borrow(mut)(drops))
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
    { Pending -> () }
    { Ready(marker) -> () }
  40 + unsafe { *drops_ptr }
}

test("async_await_loop_value_move.sc") {
  main() == 42
}

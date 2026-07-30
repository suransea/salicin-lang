let abort = effect {
  let choose(): bool
  let stop(): i32
}

let resource = struct { counter: ptr(mut)(i32) }

extend(resource, droppable) {
  let drop(self: borrow(mut)(self))(): () = {
    unsafe {
      *self.counter = *self.counter + 1
    }
  }
}

let consume(move resource: resource): i32 = { 0 }

let main(): i32 = {
  let counter = unsafe {
    raw_alloc(i32)(size_of(i32), align_of(i32))
  }
  unsafe { *counter = 0 }
  let result = abort.handle choose { (resume) -> resume(false) } stop { (resume) -> 39 } action {
      let left_resource = resource { counter: counter }
      let middle_resource = resource { counter: counter }
      let right_resource = resource { counter: counter }
      let left: with(abort)((): i32)  = { () -> abort.stop() + consume(left_resource) }
      let middle: with(abort)((): i32)  = { () -> abort.stop() + consume(middle_resource) }
      let right: with(abort)((): i32)  = { () -> abort.stop() + consume(right_resource) }
      let first: with(abort)((): i32)  = if true { left } else { middle }
      let second: with(abort)((): i32)  = if false { middle } else { right }
      let combined: with(abort)((): i32)  = if abort.choose() { first } else { second }
      combined()
    }
  let drops = unsafe { *counter }
  unsafe {
    raw_dealloc(counter, size_of(i32), align_of(i32))
  }
  result + drops
}

test("algebraic_effect_dynamic_union_drop.sc") {
  std.test.assert(main() == 42)
}

let abort = effect {
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
  let result = abort.handle stop { (resume) -> 40 } action {
      let left_resource = resource { counter: counter }
      let right_resource = resource { counter: counter }
      let left: (): i32 with(abort) = { () ->
        abort.stop() + consume(left_resource)
      }
      let right: (): i32 with(abort) = { () ->
        abort.stop() + consume(right_resource)
      }
      let first: (): i32 with(abort) = if true { left } else { right }
      let second: (): i32 with(abort) = if true { right } else { left }
      let mut selected = first
      selected = second
      selected()
    }
  let drops = unsafe { *counter }
  unsafe {
    raw_dealloc(counter, size_of(i32), align_of(i32))
  }
  result + drops
}

test("algebraic_effect_dynamic_assignment_drop.sc") {
  main() == 42
}

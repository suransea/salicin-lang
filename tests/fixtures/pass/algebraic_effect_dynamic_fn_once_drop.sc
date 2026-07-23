let Abort = effect {
  let stop(): i32
}

let Resource = struct { counter: MutPtr(i32) }

extend Resource: Drop {
  let drop(self: borrow(mut)(Self))(): () = { unsafe {
    *self.counter = *self.counter + 1
  } }}

let consume(move resource: Resource): i32 = { 0 }

let main(): i32 = {
  let counter = unsafe {
    raw_alloc(i32)(size_of(i32), align_of(i32))
  }
  unsafe { *counter = 0 }
  let result = Abort.handle(stop: { (resume) -> 40 }) {
    let left_resource = Resource { counter: counter }
    let right_resource = Resource { counter: counter }
    let left: (): i32 with(Abort) = { () ->
      let value = Abort.stop()
      value + consume(left_resource)
    }
    let right: (): i32 with(Abort) = { () ->
      let value = Abort.stop()
      value + consume(right_resource)
    }
    let action: (): i32 with(Abort) = if true { left } else { right }
    action()
  }
  let drops = unsafe { *counter }
  unsafe {
    raw_dealloc(counter, size_of(i32), align_of(i32))
  }
  result + drops
}

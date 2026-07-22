let Abort = effect {
  let stop(): i32
}

let Resource = struct(counter: MutPtr(i32))

extend Resource: Drop {
  let drop(borrow(mut) self)(): () = { unsafe {
    *self.counter = *self.counter + 1
  } }
}

let consume(move resource: Resource): i32 = { 0 }

let run(move action: (): i32 with(Abort)): i32 = {
  Abort.handle(stop: { (resume) -> 41 }) {
    action()
  }
}

let execute(counter: MutPtr(i32)): i32 = {
  let resource = Resource(counter)
  let action: (): i32 with(Abort) = { () ->
    Abort.stop() + consume(resource)
  }
  run(action)
}

let main(): i32 = {
  let counter = unsafe {
    raw_alloc(i32)(size_of(i32), align_of(i32))
  }
  unsafe { *counter = 0 }
  let result = execute(counter)
  let drops = unsafe { *counter }
  unsafe {
    raw_dealloc(counter, size_of(i32), align_of(i32))
  }
  result + drops
}

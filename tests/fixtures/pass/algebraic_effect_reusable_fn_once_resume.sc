let Ask = effect {
  let value(): i32
}

let Resource = struct { counter: MutPtr(i32) }

extend Resource: Drop {
  let drop(self: borrow(mut)(Self))(): () = { unsafe {
    *self.counter = *self.counter + 1
  } }}

let consume(move resource: Resource): i32 = { 0 }

let run(move action: (): i32 with(Ask)): i32 = {
  Ask.handle value { (resume) -> resume(41) } action {
    action()
  }
}

let execute(counter: MutPtr(i32)): i32 = {
  let resource = Resource { counter: counter }
  let action: (): i32 with(Ask) = { () ->
    Ask.value() + consume(resource)
  }
  let alias = action
  let padding = 0
  let result = run(alias) + padding
  let drops = unsafe { *counter }
  result + drops
}

let main(): i32 = {
  let counter = unsafe {
    raw_alloc(i32)(size_of(i32), align_of(i32))
  }
  unsafe { *counter = 0 }
  let result = execute(counter)
  unsafe {
    raw_dealloc(counter, size_of(i32), align_of(i32))
  }
  result
}

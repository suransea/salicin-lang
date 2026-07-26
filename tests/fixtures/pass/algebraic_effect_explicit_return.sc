let read = effect {
  let read(): i32
}

let resource = struct { counter: ptr(mut)(i32) }

extend resource: droppable {
  let drop(self: borrow(mut)(self))(): () = {
    unsafe {
      *self.counter = *self.counter + 1
    }
  }
}

let read_early(counter: ptr(mut)(i32)): i32 with(read) = {
  let resource = resource { counter: counter }
  let value = read.read()
  return(value)
}

let main(): i32 = {
  let counter = unsafe {
    raw_alloc(i32)(size_of(i32), align_of(i32))
  }
  unsafe { *counter = 0 }
  let result = read.handle read { (resume) -> resume(41) } action {
      read_early(counter)
    }
  let drops = unsafe { *counter }
  unsafe {
    raw_dealloc(counter, size_of(i32), align_of(i32))
  }
  result + drops
}

test("algebraic_effect_explicit_return.sc") {
  main() == 42
}

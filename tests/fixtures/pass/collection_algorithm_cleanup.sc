let slice = core.memory.slice

let token = struct {
  drops: ptr(mut)(i32),
}

extend(token, droppable) {
  let drop(self: borrow(mut)(self))(): () = {
    unsafe {
      *self.drops = *self.drops + 1
    }
  }
}

let read(value: borrow(i32)): i32 = { value }

let add_state(move state: (token, i32), value: borrow(i32)): (token, i32) = {
  match state {
    (owner, total) -> (owner, total + read(value))
  }
}

let main(): i32 = {
  let drops = unsafe {
    raw_alloc(i32)(size_of(i32), align_of(i32))
  }
  unsafe {
    *drops = 0
  }
  let values: array(i32)(4) = [3, 9, 12, 18]
  let view: borrow(slice(i32)) = borrow(values)

  let success = view.fold((token { drops: drops }, 0))(add_state)
  let success_total = match success {
    (owner, total) -> total
  }
  let drop_count = unsafe {
    *drops
  }
  unsafe {
    raw_dealloc(drops, size_of(i32), align_of(i32))
  }
  if success_total == 42 && drop_count == 1 {
    42
  } else {
    0
  }
}

test("collection_algorithm_cleanup.sc") {
  main() == 42
}

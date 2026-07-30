let resource = struct { value: i32 }

extend(resource, droppable) {
  let drop(self: borrow(mut)(self))(): () = {
    let trapped = 1 / self.value
  }
}

let consume(move value: resource): () = { () }

let main(): i32 = {
  let resource = resource { value: 0 }
  let once = { consume(resource) }
  0
}

test("drop_closure_abandon_trap.sc") {
  std.test.assert(main() == 42)
}

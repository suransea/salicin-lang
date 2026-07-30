let resource = struct { value: i32 }
let pair = struct { left: resource, right: resource }

extend(resource, droppable) {
  let drop(self: borrow(mut)(self))(): () = {
    let trapped = 1 / self.value
  }
}

let consume(move value: resource): () = { () }

let main(): i32 = {
  let pair = pair { left: resource { value: 1 }, right: resource { value: 0 } }
  consume(pair.left)
  0
}

test("drop_partial_field_trap.sc") {
  std.test.assert(main() == 42)
}

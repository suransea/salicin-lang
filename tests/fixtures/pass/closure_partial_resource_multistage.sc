let resource = struct { value: i32 }

extend resource: droppable {
  let drop(self: borrow(mut)(self))(): () = {
    let checked = 1 / self.value
    self.value = 0
  }
}

let consume(move value: resource): () = { () }

let main(): i32 = {
  let base = 40
  let finish = { (move resource: resource)(left: i32)(right: i32) ->
    consume(resource)
    base + left + right
  }
  let first = finish(resource { value: 1 })
  let second = first(1)
  second(1)
}

test("closure_partial_resource_multistage.sc") {
  main() == 42
}

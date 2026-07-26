let resource = struct { value: i32 }

extend resource: droppable {
  let drop(self: borrow(mut)(self))(): () = {
    let trapped = 1 / self.value
  }
}

let consume(move value: resource): () = { () }

let main(): i32 = {
  let resource = resource { value: 0 }
  let once = {
    (value: i32) -> do {
      consume(resource)
      value
    }
  }
  once(return(0))
}

test("drop_closure_early_trap.sc") {
  main() == 42
}

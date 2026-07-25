let Resource = struct { value: i32 }

extend Resource: Drop {
  let drop(self: borrow(mut)(Self))(): () = {
    let trapped = 1 / self.value
  }}

let consume(move value: Resource): () = { () }

let main(): i32 = {
  let base = 0
  let finish = { (move resource: Resource)(value: i32) ->
    consume(resource)
    base + value
  }
  let pending = finish(Resource { value: 0 })
  0
}

test("closure_partial_abandon_trap.sc") {
  main() == 42
}

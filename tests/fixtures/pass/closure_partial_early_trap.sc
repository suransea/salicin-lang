let Resource = struct { value: i32 }

extend Resource: Drop {
  let drop(self: borrow(mut)(Self))(): () = {
    let trapped = 1 / self.value
  }}

let consume(move value: Resource): () = { () }

let escape(): i32 = {
  let base = 0
  let finish = { (move resource: Resource)(value: i32) ->
    consume(resource)
    base + value
  }
  let pending = finish(Resource { value: 0 })
  pending(return(42))
}

let main(): i32 = { escape() }

test("closure_partial_early_trap.sc") {
  main() == 42
}

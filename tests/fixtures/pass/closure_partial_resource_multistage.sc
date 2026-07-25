let Resource = struct { value: i32 }

extend Resource: Drop {
  let drop(self: borrow(mut)(Self))(): () = {
    let checked = 1 / self.value
    self.value = 0
  }}

let consume(move value: Resource): () = { () }

let main(): i32 = {
  let base = 40
  let finish = { (move resource: Resource)(left: i32)(right: i32) ->
    consume(resource)
    base + left + right
  }
  let first = finish(Resource { value: 1 })
  let second = first(1)
  second(1)
}

test("closure_partial_resource_multistage.sc") {
  main() == 42
}

let Resource = struct { value: i32 }
let Bundle = struct { left: Resource, right: Resource }
let Choice = enum { Some(Bundle, Resource), None }

extend Resource: Drop {
  let drop(borrow(mut) self)(): () = {
    let trapped = 1 / self.value
  }}

let consume(move value: Resource): () = { () }

let main(): i32 = { Choice.Some(
  Bundle { left: Resource { value: 1 }, right: Resource { value: 0 } },
  Resource { value: 1 }
) match {
  Some(Bundle(left: left, right: _), _) => do {
    consume(left)
    0
  },
  None => 0
}
}

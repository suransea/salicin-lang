let Resource = struct { value: i32 }

extend Resource: Drop {
  let drop(self: borrow(mut)(Self))(): () = {
    let trapped = 1 / self.value
  }}

let consume(move value: Resource): () = { () }

let main(): i32 = {
  let resource = Resource { value: 0 }
  let once = { consume(resource) }
  0
}

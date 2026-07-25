let Resource = struct { value: i32 }

let consume(move value: Resource): () = { () }

let main(): i32 = {
  let pair = (Resource { value: 20 }, Resource { value: 22 })
  consume(pair.0)
  consume(pair.0)
  42
}

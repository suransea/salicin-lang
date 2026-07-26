let resource = struct { value: i32 }

let consume(move value: resource): () = { () }

let main(): i32 = {
  let pair = (resource { value: 20 }, resource { value: 22 })
  consume(pair.0)
  consume(pair.0)
  42
}

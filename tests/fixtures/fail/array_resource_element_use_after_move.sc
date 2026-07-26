let resource = struct { value: i32 }

let consume(move value: resource): () = { () }

let main(): i32 = {
  let values = [resource { value: 42 }]
  consume(values[0])
  values[0].value
}

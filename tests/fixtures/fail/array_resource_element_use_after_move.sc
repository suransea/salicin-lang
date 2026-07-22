let Resource = struct { value: i32 }

let consume(move value: Resource): () = { () }

let main(): i32 = {
  let values = [Resource { value: 42 }]
  consume(values[0])
  values[0].value
}

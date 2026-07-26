let resource = struct { value: i32 }

let consume(move value: resource): () = { () }
let consume_all(move values: array(resource)(2)): () = { () }

let main(): i32 = {
  let values = [resource { value: 20 }, resource { value: 22 }]
  consume(values[0])
  consume_all(values)
  42
}

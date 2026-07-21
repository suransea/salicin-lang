let Resource = struct(value: i32)

let consume(move value: Resource): () = { () }
let consume_all(move values: Array(Resource, 2)): () = { () }

let main(): i32 = {
  let values = [Resource(20), Resource(22)]
  consume(values[0])
  consume_all(values)
  42
}

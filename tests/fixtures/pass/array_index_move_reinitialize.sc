let consume(move value: i32): () = { () }

let main(): i32 = {
  let mut values = [20, 2]
  consume(values[0])
  values[0] = 40
  values[0] + values[1]
}

test("array_index_move_reinitialize.sc") {
  std.test.assert(main() == 42)
}

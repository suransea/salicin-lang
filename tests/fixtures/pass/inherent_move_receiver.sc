let payload = struct { value: i32 }

extend(payload) {
  let into_value(move self)(): i32 = { self.value }
}

let main(): i32 = {
  let payload = payload { value: 42 }
  payload.into_value()
}

test("inherent_move_receiver.sc") {
  main() == 42
}

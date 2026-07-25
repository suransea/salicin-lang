let Payload = struct { value: i32 }

extend Payload {
  let into_value(move self)(): i32 = { self.value }
}

let main(): i32 = {
  let payload = Payload { value: 42 }
  payload.into_value()
}

test("inherent_move_receiver.sc") {
  main() == 42
}

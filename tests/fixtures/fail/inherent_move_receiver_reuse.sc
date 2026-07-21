let Payload = struct(value: i32)

extend Payload {
  let into_value(move self)(): i32 = { self.value }
}

let main(): i32 = {
  let payload = Payload(42)
  let answer = payload.into_value()
  answer + payload.value
}

let payload = struct { value: i32 }

extend(payload) {
  let into_value(move self)(): i32 = { self.value }
}

let main(): i32 = {
  let payload = payload { value: 42 }
  let answer = payload.into_value()
  answer + payload.value
}

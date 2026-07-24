let Read = effect {
  let read(value: i32): i32
}

let once(value: i32): i32 with(Read) = {
  Read.read(value)
}

let main(): i32 = {
  Read.handle read { (value, resume) -> resume(value) } action {
    once(19) + once(23)
  }
}

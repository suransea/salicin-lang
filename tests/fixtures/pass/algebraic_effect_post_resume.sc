let Read = effect {
  let read(): i32
}

let program(): i32 with(Read) = {
  Read.read()
}

let main(): i32 = {
  Read.handle read { (resume) -> resume(40) + 1 } action {
    program() + 1
  }
}

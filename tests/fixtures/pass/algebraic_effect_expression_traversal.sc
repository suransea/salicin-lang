let Read = effect {
  let read(): i32
}

let main(): i32 = {
  Read.handle read { (resume) -> resume(0) } action {
    let values = [42, 0]
    match values[Read.read()]
      { 42 -> Read.read() + 42 }
      { _ -> 0 }
  }
}

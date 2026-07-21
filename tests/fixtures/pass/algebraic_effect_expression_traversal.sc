let Read = effect {
  let read(): i32
}

let main(): i32 = {
  Read.handle(
    read: { (resume) -> resume(0) },
  ) {
    let values = [42, 0]
    values[Read.read()] match {
      42 => Read.read() + 42,
      _ => 0
    }
  }
}

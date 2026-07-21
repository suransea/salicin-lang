let Read = effect {
  let read(): i32
}

let main(): i32 = {
  Read.handle(
    read: { (resume) -> resume(40) },
  ) {
    let inner = Read.handle(
      read: { (resume) -> resume(2) },
    ) {
      Read.read()
    }
    inner + Read.read()
  }
}

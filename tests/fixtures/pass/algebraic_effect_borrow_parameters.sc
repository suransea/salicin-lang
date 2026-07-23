let Read = effect {
  let read(): i32
}

let add_read(base: borrow(i32)): i32 with(Read) = {
  Read.read() + base
}

let update(base: borrow(mut)(i32)): () with(Read) = {
  base += Read.read()
}

let main(): i32 = {
  let mut base = 1
  Read.handle(
    read: { (resume) -> resume(20) },
  ) {
    let first = add_read(base)
    update(base)
    first + base
  }
}

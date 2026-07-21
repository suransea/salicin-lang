let Read = effect {
  let read(): i32
}

let Add = effect {
  let add(x: i32): i32
}

let program(): i32 with(Read, Add) = {
  Add.add(Read.read())
}

let main(): i32 = {
  Read.handle(
    read: { (resume) -> resume(20) },
  ) {
    Add.handle(
      add: { (x, resume) -> resume(x + Read.read() + 2) },
    ) {
      program()
    }
  }
}

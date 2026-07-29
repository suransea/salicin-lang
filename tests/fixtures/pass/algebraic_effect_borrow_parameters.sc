let read = effect {
  let read(): i32
}

let add_read: with(read)(base: borrow(i32)): i32 = {
  read.read() + base
}

let update: with(read)(base: borrow(mut)(i32)): () = {
  base += read.read()
}

let main(): i32 = {
  let mut base = 1
  read.handle read { (resume) -> resume(20) } action {
      let first = add_read(base)
      update(base)
      first + base
    }
}

test("algebraic_effect_borrow_parameters.sc") {
  main() == 42
}

let read = effect {
  let read(): i32
}

let add = effect {
  let add(x: i32): i32
}

let program(): i32 with(read, add) = {
  add.add(read.read())
}

let main(): i32 = {
  read.handle read { (resume) -> resume(20) } action {
      add.handle add { (x, resume) -> resume(x + read.read() + 2) } action {
        program()
      }
    }
}

test("algebraic_effect_composition.sc") {
  main() == 42
}

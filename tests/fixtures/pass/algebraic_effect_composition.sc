let read = effect {
  let read(): i32
}

let add_operator = effect {
  let add(x: i32): i32
}

let program(): i32 with(read, add_operator) = {
  add_operator.add(read.read())
}

let main(): i32 = {
  read.handle read { (resume) -> resume(20) } action {
      add_operator.handle add { (x, resume) -> resume(x + read.read() + 2) } action {
        program()
      }
    }
}

test("algebraic_effect_composition.sc") {
  main() == 42
}

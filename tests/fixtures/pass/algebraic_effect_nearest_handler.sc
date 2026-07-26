let read = effect {
  let read(): i32
}

let main(): i32 = {
  read.handle read { (resume) -> resume(40) } action {
      let inner = read.handle read { (resume) -> resume(2) } action {
        read.read()
      }
      inner + read.read()
    }
}

test("algebraic_effect_nearest_handler.sc") {
  main() == 42
}

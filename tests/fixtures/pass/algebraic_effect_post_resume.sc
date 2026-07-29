let read = effect {
  let read(): i32
}

let program: with(read)(): i32 = {
  read.read()
}

let main(): i32 = {
  read.handle read { (resume) -> resume(40) + 1 } action {
      program() + 1
    }
}

test("algebraic_effect_post_resume.sc") {
  main() == 42
}

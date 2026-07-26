let read = effect {
  let read(): i32
}

let main(): i32 = {
  read.handle read { (resume) -> resume(0) } action {
      let values = [42, 0]
      match values[read.read()]
        { 42 -> read.read() + 42 }
        { _ -> 0 }
    }
}

test("algebraic_effect_expression_traversal.sc") {
  main() == 42
}

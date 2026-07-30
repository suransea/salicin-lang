let read = effect {
  let read(): usize
}

let main(): i32 = {
  read.handle read { (resume) -> resume(0) } action {
      let values = [42, 0]
      match values[read.read()]
        { 42 -> if read.read() == 0 { 42 } else { 0 } }
        { _ -> 0 }
    }
}

test("algebraic_effect_expression_traversal.sc") {
  std.test.assert(main() == 42)
}

let read = effect {
  let read(): i32
}

let sum_reads(count: i32): i32 with(read) = {
  if count == 0 {
    return(0)
  }
  read.read() + sum_reads(count - 1)
}

let main(): i32 = {
  let value = 14
  read.handle read { (resume) -> resume(value) } action {
      sum_reads(3)
    }
}

test("algebraic_effect_recursion.sc") {
  main() == 42
}

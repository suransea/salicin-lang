let Read = effect {
  let read(): i32
}

let sum_reads(count: i32): i32 with(Read) = {
  if count == 0 {
    return 0
  }
  Read.read() + sum_reads(count - 1)
}

let main(): i32 = {
  let value = 14
  Read.handle(
    read: { (resume) -> resume(value) },
  ) {
    sum_reads(3)
  }
}

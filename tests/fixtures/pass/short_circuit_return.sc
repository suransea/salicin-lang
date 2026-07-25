let with_and(flag: bool): i32 = {
  flag && return(20)
  1
}

let with_or(flag: bool): i32 = {
  flag || return(22)
  1
}

let main(): i32 = { with_and(true) + with_or(false) }

test("short_circuit_return.sc") {
  main() == 42
}

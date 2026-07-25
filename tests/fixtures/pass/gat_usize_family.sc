let Fixed(L: usize): type = Array(i32)(L)

let Keep = trait {
  let Output(L: usize): type

  let keep(L: usize)(move value: Output(L)): Output(L)
}

let Marker = struct {}

extend Marker: Keep {
  let Output = Fixed

  let keep(L: usize)(move value: Array(i32)(L)): Array(i32)(L) = {
    value
  }
}

let main(): i32 = {
  let values = Marker.keep(2)([20, 22])
  values[0] + values[1]
}

test("gat_usize_family.sc") {
  main() == 42
}

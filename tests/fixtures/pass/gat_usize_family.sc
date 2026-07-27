let fixed(comptime l: usize): type = array(i32)(l)

let keep = trait {
  let output(comptime l: usize): type

  let keep(comptime l: usize)(move value: output(l)): output(l)
}

let marker = struct {}

extend(marker, keep) {
  let output = fixed

  let keep(comptime l: usize)(move value: array(i32)(l)): array(i32)(l) = {
    value
  }
}

let main(): i32 = {
  let values = marker.keep(2)([20, 22])
  values[0] + values[1]
}

test("gat_usize_family.sc") {
  main() == 42
}

let cell(comptime t: type) = struct { value: t }

let main(): i32 = {
  let cell = cell(cell(_)) { value: cell(i32) { value: 42 } }
  cell.value.value
}

let cell = struct { value: i32 }

let main(): i32 = {
  let cell = cell { value: 40, value: 2 }
  cell.value
}

let cell(comptime t: type) = struct { value: t }

extend(comptime t: type) cell(t): copyable
where t: copyable {}

let read_twice(copy cell: cell(cell(i32))): i32 = {
  let duplicate = cell
  duplicate.value.value + cell.value.value - 42
}

let main(): i32 = {
  let inner = cell { value: 42 }
  let outer = cell { value: inner }
  let duplicate = outer
  read_twice(outer) + duplicate.value.value - 42
}

test("copy_generic_blanket.sc") {
  main() == 42
}

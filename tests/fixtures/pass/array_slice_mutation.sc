let slice = core.memory.slice

let main(): i32 = {
  let mut order: array(i32)(5) = [1, 2, 3, 4, 5]
  order.swap(0, 4)
  order.swap(2, 2)
  order.reverse()

  let mut overlap_right: array(i32)(5) = [1, 2, 3, 4, 5]
  overlap_right.copy_within(0, 4, 1)

  let mut overlap_left: array(i32)(5) = [1, 2, 3, 4, 5]
  do {
    let values: borrow(mut)(slice(i32)) = borrow(mut)(overlap_left)
    values.copy_within(1, 5, 0)
    values.copy_within(5, 5, 5)
  }

  let mut filled: array(i32)(3) = [1, 2, 3]
  filled.fill(14)

  let source: array(i32)(3) = [12, 14, 16]
  let mut copied: array(i32)(3) = [0, 0, 0]
  do {
    let source_values: borrow(slice(i32)) = borrow(source)
    copied.copy_from(source_values)
  }

  let empty_source: array(i32)(0) = []
  let mut empty: array(i32)(0) = []
  empty.reverse()
  empty.fill(42)
  empty.copy_within(0, 0, 0)
  do {
    let source_values: borrow(slice(i32)) = borrow(empty_source)
    empty.copy_from(source_values)
  }

  if !(order[0] == 1 &&
    order[1] == 4 &&
    order[2] == 3 &&
    order[3] == 2 &&
    order[4] == 5) {
    return 1
  }
  if !(overlap_right[0] == 1 &&
    overlap_right[1] == 1 &&
    overlap_right[2] == 2 &&
    overlap_right[3] == 3 &&
    overlap_right[4] == 4) {
    return 2
  }
  if !(overlap_left[0] == 2 &&
    overlap_left[1] == 3 &&
    overlap_left[2] == 4 &&
    overlap_left[3] == 5 &&
    overlap_left[4] == 5) {
    return 3
  }
  if filled[0] + filled[1] + filled[2] != 42 {
    return 4
  }
  if copied[0] + copied[1] + copied[2] != 42 {
    return 5
  }
  42
}

test("array_slice_mutation.sc") {
  std.test.assert(main() == 42)
}

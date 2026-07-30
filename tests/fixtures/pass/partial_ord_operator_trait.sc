let partial_ord = core.ops.partial_ord
let partial_ordering = core.ops.partial_ordering

let number = struct { value: i32, unordered: bool }

extend(number, partial_ord(number)) {
  let partial_cmp(self: borrow(self))(rhs: borrow(number)): partial_ordering = {
    if self.unordered || rhs.unordered { unordered }
    else if self.value < rhs.value { less }
    else if self.value > rhs.value { greater }
    else { equal }
  }
}

let main(): i32 = {
  let low = number { value: 1, unordered: false }
  let high = number { value: 2, unordered: false }
  let none = number { value: 0, unordered: true }
  if low < high && low <= high && high > low && high >= low &&
    !(none < low) && !(none <= low) && !(none > low) && !(none >= low) {
    42
  } else {
    0
  }
}

test("partial_ord_operator_trait.sc") {
  std.test.assert(main() == 42)
}

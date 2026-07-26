let partial_order = std.ops.partial_order
let partial_ordering = std.ops.partial_ordering

let number = struct { value: i32, unordered: bool }

extend number: partial_order(number) {
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
  main() == 42
}

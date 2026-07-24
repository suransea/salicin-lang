let PartialOrd = std.ops.PartialOrd
let PartialOrdering = std.ops.PartialOrdering

let Number = struct { value: i32, unordered: bool }

extend Number: PartialOrd(Number) {
  let partial_cmp(self: borrow(Self))(rhs: borrow(Number)): PartialOrdering = {
    if self.unordered || rhs.unordered { Unordered }
    else if self.value < rhs.value { Less }
    else if self.value > rhs.value { Greater }
    else { Equal }
  }
}

let main(): i32 = {
  let low = Number { value: 1, unordered: false }
  let high = Number { value: 2, unordered: false }
  let none = Number { value: 0, unordered: true }
  if low < high && low <= high && high > low && high >= low &&
    !(none < low) && !(none <= low) && !(none > low) && !(none >= low) {
    42
  } else {
    0
  }
}

use core.ops.{PartialOrd, PartialOrdering}

let Number = struct(value: i32, unordered: bool)

extend Number: PartialOrd(Number) {
  let partial_cmp(borrow self)(borrow rhs: Number): PartialOrdering =
    if self.unordered || rhs.unordered { Unordered }
    else if self.value < rhs.value { Less }
    else if self.value > rhs.value { Greater }
    else { Equal }
}

let main(): i32 = {
  let low = Number(1, false)
  let high = Number(2, false)
  let none = Number(0, true)
  if low < high && low <= high && high > low && high >= low &&
    !(none < low) && !(none <= low) && !(none > low) && !(none >= low) {
    42
  } else {
    0
  }
}

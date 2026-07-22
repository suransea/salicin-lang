let Select = trait {
  let pick(borrow self)(left: i32): i32
  let pick(borrow self)(right: i32): i32
  let make(left: i32): i32
  let make(right: i32): i32
}

let Counter = struct { value: i32 }

extend Counter: Select {
  let pick(borrow self)(left: i32): i32 = { self.value + left }
  let pick(borrow self)(right: i32): i32 = { self.value + right + 1 }
  let make(left: i32): i32 = { left }
  let make(right: i32): i32 = { right + 1 }
}

let main(): i32 = {
  Counter { value: 0 }.pick(right: 20) + Counter.make(right: 20)
}

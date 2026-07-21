let choose(T: type)(left: T): T = { left }
let choose(T: type)(right: T): T = { right }

let Counter = struct(value: i32)

extend Counter {
  let add(T: type)(borrow self)(left: T): T = { left }
  let add(T: type)(borrow self)(right: T): T = { right }
}

let Cell(T: type) = struct(value: T)

extend(T: type) Cell(T) {
  let choose(left: T): T = { left }
  let choose(right: T): T = { right }
  let add(borrow self)(left: T): T = { left }
  let add(borrow self)(right: T): T = { right }
}

let main(): i32 = {
  choose(left: 10) + Cell.choose(right: 10) + Cell(i32)(0).add(left: 22)
}

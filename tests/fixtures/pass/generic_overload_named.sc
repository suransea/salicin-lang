let choose(T: type)(left: T): T = { left }
let choose(T: type)(right: T): T = { right }

let Counter = struct(value: i32)

extend Counter {
  let add(T: type)(borrow self)(left: T): T = { left }
  let add(T: type)(borrow self)(right: T): T = { right }
}

let main(): i32 = {
  choose(left: 20) + Counter(0).add(i32)(right: 22)
}

let Pair = struct(left: i32, flag: bool)

let Input = enum {
  Number(i32),
  Flag(bool),
  Pair(Pair),
  Empty,
}

let classify(value: Input): i32 = value match {
  Number(40) => 1,
  Number(42) if true => 20,
  Number(_) => 0,
  Flag(true) => 10,
  Flag(false) => 0,
  Flag(_) => 0,
  Pair(Pair(left: 10, flag: true)) => 11,
  Pair(_) => 0,
  Empty => 0,
}

let main(): i32 = {
  classify(Input.Number(40)) +
    classify(Input.Number(42)) +
    classify(Input.Flag(true)) +
    classify(Input.Pair(Pair(left: 10, flag: true)))
}

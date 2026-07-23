let Pair(K: type, V: type) = struct { key: K, value: V }

let PairAlias: (Key: type, Value: type): type = Pair

let Holds(Item: type) = trait {
  let get(self: borrow(Self))(): Item
}

extend Pair(i32, bool): Holds(Item: i32) {
  let get(self: borrow(Self))(): i32 = { self.key }
}

let read(T: type)(value: borrow(T)): i32
where T: Holds(Item: i32)
= {
  value.get()
}

let make(): PairAlias(Value: bool, Key: i32) = {
  Pair(K: i32, V: bool) { key: 41, value: true }
}

let main(): i32 = {
  let pair: Pair(V: bool, K: i32) = make()
  if pair.value { read(Pair(i32, bool))(pair) + 1 } else { 0 }
}

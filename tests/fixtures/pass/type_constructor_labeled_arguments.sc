let pair(comptime k: type, comptime v: type) = struct { key: k, value: v }

let pair_alias: (comptime key: type, comptime value: type): type = pair

let holds(comptime item: type) = trait {
  let get(self: borrow(self))(): item
}

extend(pair(i32, bool), holds(item: i32)) {
  let get(self: borrow(self))(): i32 = { self.key }
}

let read(comptime t: type)(value: borrow(t)): i32
where t: holds(item: i32)
= {
  value.get()
}

let make(): pair_alias(value: bool, key: i32) = {
  pair(k: i32, v: bool) { key: 41, value: true }
}

let main(): i32 = {
  let pair_value: pair(v: bool, k: i32) = make()
  if pair_value.value { read(t: pair(i32, bool))(pair_value) + 1 } else { 0 }
}

test("type_constructor_labeled_arguments.sc") {
  main() == 42
}

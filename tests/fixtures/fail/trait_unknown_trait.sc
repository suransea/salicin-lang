let Number = struct { value: i32 }

extend Number: MissingTrait {
  let read(borrow self)(): i32 = { self.value }
}

let main(): i32 = { 0 }

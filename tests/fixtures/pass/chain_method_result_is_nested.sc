let Boxed = struct(value: i32)

extend Boxed {
  let optional(move self)(): Option(i32) = { Option(i32).Some(self.value) }
}

let main(): i32 = {
  let nested = Option(Boxed).Some(Boxed(42))?.optional()
  nested match {
    Some(inner) => inner ?? 0,
    None => 0,
  }
}

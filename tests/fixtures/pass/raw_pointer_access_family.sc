let preserve(A: access)(T: type)(pointer: Ptr(A)(T)): Ptr(A)(T) = { pointer }

let main(): i32 = {
  let shared_value = 40
  let mut mutable_value = 1
  let shared_pointer = Ptr(i32)(borrow(shared_value))
  let mutable_pointer = Ptr(mut)(i32)(borrow(mut)(mutable_value))
  let shared = preserve(shared_pointer)
  let mutable = preserve(mut)(mutable_pointer)
  unsafe {
    *mutable = *mutable + 1
    *shared + *mutable
  }
}

test("raw_pointer_access_family.sc") {
  main() == 42
}

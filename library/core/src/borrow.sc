// Borrow type and value contracts built over the compile-time access, region,
// and type domains.
/// Type constructor for a borrow with access `A`, region `R`, and pointee `T`.
pub let borrow(A: access = shared)
  (R: region)
  (T: type): type

/// Creates or reborrows a borrow of an addressable pointee.
pub let borrow(A: access = shared)
  (R: region)
  (T: type)
  (value: T): borrow(A)(R)(T)

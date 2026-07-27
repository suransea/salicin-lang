let string = alloc.string.string

/// Product data owns its validated UTF-8 name.
pub(package) let product = struct {
  name: string,
  units: i32,
  unit_price: i32,
}

/// Computes a value without exposing a product's representation.
pub(package) let valued = trait {
  let value(self: borrow(self))(): i32
}

extend(product) {
  let new(move name: string, units: i32, unit_price: i32): product = {
    product { name: name, units: units, unit_price: unit_price }
  }

  let name_bytes(self: borrow(self))(): u64 = {
    self.name.len_bytes()
  }
}

extend(product, valued) {
  let value(self: borrow(self))(): i32 = {
    self.units * self.unit_price
  }
}

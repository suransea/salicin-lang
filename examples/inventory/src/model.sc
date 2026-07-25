let String = std.string.String

/// Product data owns its validated UTF-8 name.
pub(package) let Product = struct {
  name: String,
  units: i32,
  unit_price: i32,
}

/// Computes a value without exposing a product's representation.
pub(package) let Valued = trait {
  let value(self: borrow(Self))(): i32
}

extend Product {
  let new(move name: String, units: i32, unit_price: i32): Product = {
    Product { name: name, units: units, unit_price: unit_price }
  }

  let name_bytes(self: borrow(Self))(): u64 = {
    self.name.len_bytes()
  }
}

extend Product: Valued {
  let value(self: borrow(Self))(): i32 = {
    self.units * self.unit_price
  }
}

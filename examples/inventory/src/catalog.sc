let Vec = std.vec.Vec

/// Collection of owned products.
pub(package) let Inventory = struct {
  products: Vec(model.Product),
}

/// Consumes a collection and summarizes all entries through their trait API.
pub(package) let Summarize = trait {
  let total(move self)(): i32
}

extend Inventory {
  let new(): Inventory = {
    Inventory { products: Vec(model.Product).new() }
  }

  let push(self: borrow(mut)(Self))(move product: model.Product): () = {
    self.products.push(product)
  }
}

extend Inventory: Summarize {
  let total(move self)(): i32 = {
    let mut owner = self
    let products = owner.products.take()
    let mut total = 0
    for products { product ->
      total = total + product.value()
    }
    total
  }
}

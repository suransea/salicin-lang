// A complete effectful batch-processing program over the frozen M0 core. The
// process exits with 42 after applying four valid transactions, or 1 on overdraft.

let Option = std.Option
let Iterator = std.iter.Iterator
let IntoIterator = std.iter.IntoIterator
let OwnedItem = std.iter.OwnedItem

let Transaction = enum {
  Credit(i32),
  Debit(i32),
}

let Overdraft = effect {
  let reject(): Never
}

let Ledger = struct {
  balance: i32,
  processed: i32,
}

let Account = trait {
  let credit(self: borrow(mut)(Self))(amount: i32): ()
  let debit(self: borrow(mut)(Self))(amount: i32): ()
  let snapshot(self: borrow(Self))(): i32
}

extend Ledger: Account {
  let credit(self: borrow(mut)(Self))(amount: i32): () = {
    self.balance = self.balance + amount
    self.processed = self.processed + 1
  }

  let debit(self: borrow(mut)(Self))(amount: i32): () = {
    self.balance = self.balance - amount
    self.processed = self.processed + 1
  }

  let snapshot(self: borrow(Self))(): i32 = {
    if self.processed == 4 { self.balance } else { 0 }
  }
}

let Batch = struct {
  index: i32,
}

extend Batch: Iterator {
  let Item = OwnedItem(Transaction)

  let next(R: region)(self: borrow(mut)(R)(Self))(): Option(Transaction) = {
    let transaction: Option(Transaction) = match self.index
      { 0 -> Some(Transaction.Credit(30)) }
      { 1 -> Some(Transaction.Debit(8)) }
      { 2 -> Some(Transaction.Credit(25)) }
      { 3 -> Some(Transaction.Debit(5)) }
      { _ -> None }
    self.index = self.index + 1
    transaction
  }
}

extend Batch: IntoIterator {
  let IntoIter = Batch

  let into_iter(move self)(): Batch = {
    self
  }
}

let count_batch(move batch: Batch): i32 = {
  let mut count = 0
  for batch { _ ->
    count = count + 1
  }
  count
}

let apply(ledger: borrow(mut)(Ledger))(move transaction: Transaction): () with(Overdraft) = {
  match transaction
    { Credit(amount) -> ledger.credit(amount) }
    { Debit(amount) ->
      if amount > ledger.balance {
        Overdraft.reject()
      } else {
        ledger.debit(amount)
      }
    }
}

let process(move batch: Batch): i32 with(Overdraft) = {
  let mut ledger = Ledger { balance: 0, processed: 0 }
  for batch { transaction ->
    apply(ledger)(transaction)
  }
  ledger.snapshot()
}

let main(): i32 = {
  let balance = Overdraft.handle reject { () -> 1 } action {
    process(Batch { index: 0 })
  }
  let count = count_batch(Batch { index: 0 })
  if balance == 42 && count == 4 { 42 } else { 1 }
}

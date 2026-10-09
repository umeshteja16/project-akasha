# Ownership and borrowing in Rust

Every value in Rust has exactly one owner, and the value is dropped when its owner goes out of scope. Assigning or passing a value moves ownership unless the type is `Copy`.

References borrow a value without taking ownership. At any time you may have either any number of shared references (`&T`) or exactly one mutable reference (`&mut T`), never both. The borrow checker enforces this at compile time, which rules out data races and use-after-free bugs.

Lifetimes name how long references are valid. Most are inferred; you write them when a function returns a reference derived from one of several arguments, so the compiler knows which input the output borrows from.

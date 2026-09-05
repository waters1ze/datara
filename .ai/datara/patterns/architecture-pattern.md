# Datara Entity-Component-Role-Process (ECRP) Pattern

Datara completely rejects classic implementation inheritance (`extends` was removed) in favor of the **ECRP Architecture**:
1. **Entity**: Data record representing domain objects.
2. **Component**: Reusable flat state inlined at compile-time (zero runtime overhead).
3. **Role**: Contract interface specifying behavior requirements.
4. **Process**: Pipeline workflow describing execution order.

---

## 1. Code Architecture Example

```datara
// 1. Component: Inlined flat state
component Timestamped {
    created_at: Int
    updated_at: Int
}

// 2. Role: Contract without vtables
role Payable {
    pay(amount: Int) -> Int
}

// 3. Entity: Flat composition
entity User with Timestamped {
    name: Str
    balance: Int

    is_active() -> Bool => balance > 0
}

// 4. Behavior: Fulfilling role contract
behavior User {
    pay(amount: Int) -> Int {
        this.balance = this.balance - amount
        return this.balance
    }
}

// 5. Process: Named workflow pipeline
process checkout(user: User, price: Int) -> Int {
    user
    then user.pay(price)
}
```

---

## 2. Monomorphic Direct Dispatch

In Datara, method calls on entities and behaviors are monomorphically resolved at compile time:
- **0 Virtual Method Tables (vtables)**
- **0 Indirect Function Pointers**
- Inlining happens across module boundaries.

# Anti-Pattern: Module & Naming Mistakes

---

## 1. Naming Convention Violations

Forgen strictly audits naming styles:
- **`style::non_snake_case`**: Variables, parameters, and function names MUST be `snake_case`.
- **`style::non_camel_case_types`**: Classes, entities, components, roles, and enums MUST be `PascalCase`.

### Bad Code:
```datara
class userAccount { // Error: non_camel_case_types
    userAge: Int     // Error: non_snake_case
}

fn CalculateScore() { ... } // Error: non_snake_case
```

### Correct Code:
```datara
class UserAccount {
    user_age: Int
}

fn calculate_score() { ... }
```

---

## 2. Circular Module Dependencies (`E-RESOLVE-004`)

- **Mistake**: Having `module_a.dtr` import `module_b` while `module_b.dtr` imports `module_a`.
- **Fix**: Extract shared types or contracts into a third module (`types.dtr` or `core.dtr`).

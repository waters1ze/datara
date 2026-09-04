use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum ErrorCode {
    // Syntax Errors (E-SYNTAX-*)
    SyntaxUnexpectedToken,
    SyntaxUnterminatedString,
    SyntaxUnterminatedComment,
    SyntaxInvalidNumber,
    SyntaxInvalidChar,
    SyntaxExpectedExpression,
    SyntaxExpectedIdentifier,
    SyntaxExpectedType,

    // Resolution Errors (E-RESOLVE-*)
    ResolveUndefinedSymbol,
    ResolveDuplicateSymbol,
    ResolveUnknownType,
    ResolveCircularDependency,
    ResolveUnreachableModule,

    // Type Errors (E-TYPE-*)
    TypeMismatch,
    TypeCannotInfer,
    TypeMissingReturn,
    TypeInvalidBinaryOp,
    TypeInvalidUnaryOp,
    TypeInvalidMemberAccess,
    TypeGenericMismatch,

    // Borrow & Ownership Errors (E-BORROW-*)
    BorrowUseAfterMove,
    BorrowCannotMutateImmutable,
    BorrowConflictActiveView,
    BorrowMultipleMutableViews,
    BorrowEscapingView,

    // Effect Errors (E-EFFECT-*)
    EffectImpureInPureContext,
    EffectUnsafeOperation,
    EffectUnhandledIO,

    // Codegen Errors (E-CODEGEN-*)
    CodegenBackendFailed,
    CodegenLinkerFailed,
    CodegenIOError,

    // Security & Zero-Trust Gate Errors (E0940-E0943)
    SecurityViolation,
    ProofCarryingCodeViolation,
    UncheckedFFIViolation,
    DataRaceViolation,

    // Real-Time & Safety Verification Gate Errors (E0950-E0951)
    AllocationViolation,
    PanicViolation,
}

impl ErrorCode {
    pub fn as_str(&self) -> &'static str {
        match self {
            ErrorCode::SyntaxUnexpectedToken => "E-SYNTAX-001",
            ErrorCode::SyntaxUnterminatedString => "E-SYNTAX-002",
            ErrorCode::SyntaxUnterminatedComment => "E-SYNTAX-003",
            ErrorCode::SyntaxInvalidNumber => "E-SYNTAX-004",
            ErrorCode::SyntaxInvalidChar => "E-SYNTAX-005",
            ErrorCode::SyntaxExpectedExpression => "E-SYNTAX-006",
            ErrorCode::SyntaxExpectedIdentifier => "E-SYNTAX-007",
            ErrorCode::SyntaxExpectedType => "E-SYNTAX-008",

            ErrorCode::ResolveUndefinedSymbol => "E-RESOLVE-001",
            ErrorCode::ResolveDuplicateSymbol => "E-RESOLVE-002",
            ErrorCode::ResolveUnknownType => "E-RESOLVE-003",
            ErrorCode::ResolveCircularDependency => "E-RESOLVE-004",
            ErrorCode::ResolveUnreachableModule => "E-RESOLVE-005",

            ErrorCode::TypeMismatch => "E-TYPE-001",
            ErrorCode::TypeCannotInfer => "E-TYPE-002",
            ErrorCode::TypeMissingReturn => "E-TYPE-003",
            ErrorCode::TypeInvalidBinaryOp => "E-TYPE-004",
            ErrorCode::TypeInvalidUnaryOp => "E-TYPE-005",
            ErrorCode::TypeInvalidMemberAccess => "E-TYPE-006",
            ErrorCode::TypeGenericMismatch => "E-TYPE-007",

            ErrorCode::BorrowUseAfterMove => "E-BORROW-001",
            ErrorCode::BorrowCannotMutateImmutable => "E-BORROW-002",
            ErrorCode::BorrowConflictActiveView => "E-BORROW-003",
            ErrorCode::BorrowMultipleMutableViews => "E-BORROW-004",
            ErrorCode::BorrowEscapingView => "E-BORROW-005",

            ErrorCode::EffectImpureInPureContext => "E-EFFECT-001",
            ErrorCode::EffectUnsafeOperation => "E-EFFECT-002",
            ErrorCode::EffectUnhandledIO => "E-EFFECT-003",

            ErrorCode::CodegenBackendFailed => "E-CODEGEN-001",
            ErrorCode::CodegenLinkerFailed => "E-CODEGEN-002",
            ErrorCode::CodegenIOError => "E-CODEGEN-003",

            ErrorCode::SecurityViolation => "E0940",
            ErrorCode::ProofCarryingCodeViolation => "E0941",
            ErrorCode::UncheckedFFIViolation => "E0942",
            ErrorCode::DataRaceViolation => "E0943",
            ErrorCode::AllocationViolation => "E0950",
            ErrorCode::PanicViolation => "E0951",
        }
    }

    pub fn description(&self, locale: &str) -> &'static str {
        if locale == "ru" {
            match self {
                ErrorCode::SyntaxUnexpectedToken => "Неожиданный токен",
                ErrorCode::SyntaxUnterminatedString => "Незакрытая строковая константа",
                ErrorCode::SyntaxUnterminatedComment => "Незакрытый многострочный комментарий",
                ErrorCode::SyntaxInvalidNumber => "Некорректный числовой литерал",
                ErrorCode::SyntaxInvalidChar => "Недопустимый символ",
                ErrorCode::SyntaxExpectedExpression => "Ожидалось выражение",
                ErrorCode::SyntaxExpectedIdentifier => "Ожидался идентификатор",
                ErrorCode::SyntaxExpectedType => "Ожидалось имя типа",

                ErrorCode::ResolveUndefinedSymbol => "Неопределённый символ",
                ErrorCode::ResolveDuplicateSymbol => "Дублирующееся объявление символа",
                ErrorCode::ResolveUnknownType => "Неизвестный тип данных",
                ErrorCode::ResolveCircularDependency => "Циклическая зависимость",
                ErrorCode::ResolveUnreachableModule => "Недостижимый модуль",

                ErrorCode::TypeMismatch => "Несоответствие типов данных",
                ErrorCode::TypeCannotInfer => "Невозможно вывести тип выражения",
                ErrorCode::TypeMissingReturn => "Отсутствует возвращаемое значение",
                ErrorCode::TypeInvalidBinaryOp => "Недопустимая бинарная операция для типов",
                ErrorCode::TypeInvalidUnaryOp => "Недопустимая унарная операция",
                ErrorCode::TypeInvalidMemberAccess => "Поле или метод не существует в типе",
                ErrorCode::TypeGenericMismatch => "Несоответствие аргументов обобщённого типа",

                ErrorCode::BorrowUseAfterMove => {
                    "Использование значения после перемещения (use-after-move)"
                }
                ErrorCode::BorrowCannotMutateImmutable => {
                    "Попытка изменения неизменяемой переменной"
                }
                ErrorCode::BorrowConflictActiveView => {
                    "Конфликт заимствования: изменение при активном view"
                }
                ErrorCode::BorrowMultipleMutableViews => {
                    "Конфликт заимствования: множественные mut-view запрещены"
                }
                ErrorCode::BorrowEscapingView => {
                    "Утечка заимствования (view не может пережить локальную переменную)"
                }

                ErrorCode::EffectImpureInPureContext => "Побочный эффект в чистом контексте",
                ErrorCode::EffectUnsafeOperation => "Небезопасная операция",
                ErrorCode::EffectUnhandledIO => "Необработанный ввод-вывод",

                ErrorCode::CodegenBackendFailed => "Ошибка генерации нативного кода",
                ErrorCode::CodegenLinkerFailed => "Ошибка компоновщика",
                ErrorCode::CodegenIOError => "Ошибка ввода-вывода при компиляции",

                ErrorCode::SecurityViolation => {
                    "Нарушение безопасности: операция требует мандат полномочий (Capability)"
                }
                ErrorCode::ProofCarryingCodeViolation => {
                    "Нарушение Proof-Carrying Code: операция не имеет доказательства безопасности"
                }
                ErrorCode::UncheckedFFIViolation => {
                    "Небезопасный вызов FFI без блока 'unsafe(justification: ...)'"
                }
                ErrorCode::DataRaceViolation => {
                    "Нарушение параллелизма: потенциальная гонка данных переменной"
                }
                ErrorCode::AllocationViolation => {
                    "Нарушение режима реального времени: динамическое выделение памяти в контексте '@no_alloc'"
                }
                ErrorCode::PanicViolation => {
                    "Нарушение режима реального времени: недоказанный путь паники в контексте '@no_panic'"
                }
            }
        } else {
            match self {
                ErrorCode::SyntaxUnexpectedToken => "Unexpected token in source",
                ErrorCode::SyntaxUnterminatedString => "Unterminated string literal",
                ErrorCode::SyntaxUnterminatedComment => "Unterminated multi-line comment",
                ErrorCode::SyntaxInvalidNumber => "Invalid numeric literal format",
                ErrorCode::SyntaxInvalidChar => "Invalid character in input stream",
                ErrorCode::SyntaxExpectedExpression => "Expected an expression",
                ErrorCode::SyntaxExpectedIdentifier => "Expected an identifier",
                ErrorCode::SyntaxExpectedType => "Expected a type annotation",

                ErrorCode::ResolveUndefinedSymbol => "Undefined symbol reference",
                ErrorCode::ResolveDuplicateSymbol => "Duplicate symbol declaration",
                ErrorCode::ResolveUnknownType => "Unknown type identifier",
                ErrorCode::ResolveCircularDependency => "Circular module dependency detected",
                ErrorCode::ResolveUnreachableModule => "Unreachable module detected",

                ErrorCode::TypeMismatch => "Static type mismatch",
                ErrorCode::TypeCannotInfer => "Unable to infer expression type",
                ErrorCode::TypeMissingReturn => "Non-unit function missing return value",
                ErrorCode::TypeInvalidBinaryOp => "Binary operator not defined for operand types",
                ErrorCode::TypeInvalidUnaryOp => "Unary operator not defined for operand type",
                ErrorCode::TypeInvalidMemberAccess => "Field or method does not exist on type",
                ErrorCode::TypeGenericMismatch => "Generic type argument mismatch",

                ErrorCode::BorrowUseAfterMove => "Use of moved value (use-after-move)",
                ErrorCode::BorrowCannotMutateImmutable => {
                    "Cannot mutate or reassign immutable binding"
                }
                ErrorCode::BorrowConflictActiveView => {
                    "Cannot mutate value while active immutable view exists"
                }
                ErrorCode::BorrowMultipleMutableViews => {
                    "Cannot create multiple concurrent mutable views"
                }
                ErrorCode::BorrowEscapingView => {
                    "Escaping view: reference cannot outlive local binding"
                }

                ErrorCode::EffectImpureInPureContext => "Side effect occurred in pure context",
                ErrorCode::EffectUnsafeOperation => {
                    "Unsafe operation without explicit unsafe block"
                }
                ErrorCode::EffectUnhandledIO => "Unhandled IO operation",

                ErrorCode::CodegenBackendFailed => "Native codegen backend failed",
                ErrorCode::CodegenLinkerFailed => "Linker execution failed",
                ErrorCode::CodegenIOError => "IO failure during artifact generation",

                ErrorCode::SecurityViolation => {
                    "Security Violation: Operation requires capability token"
                }
                ErrorCode::ProofCarryingCodeViolation => {
                    "Proof-Carrying Code Violation: Unproven operation"
                }
                ErrorCode::UncheckedFFIViolation => {
                    "Security Violation: Foreign call requires unsafe justification"
                }
                ErrorCode::DataRaceViolation => {
                    "Concurrency Violation: Potential data race across threads"
                }
                ErrorCode::AllocationViolation => {
                    "Real-Time Violation: Dynamic memory allocation in '@no_alloc' context"
                }
                ErrorCode::PanicViolation => {
                    "Real-Time Violation: Unproven panic path in '@no_panic' context"
                }
            }
        }
    }
}

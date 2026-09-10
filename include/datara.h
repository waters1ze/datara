#ifndef DATARA_H
#define DATARA_H

#include <stdint.h>
#include <stddef.h>
#include <stdbool.h>

#ifdef __cplusplus
extern "C" {
#endif

#if defined(_WIN32) || defined(__CYGWIN__)
  #if defined(DATARA_BUILD_SHARED)
    #define DATARA_API __declspec(dllexport)
  #elif defined(DATARA_USE_SHARED)
    #define DATARA_API __declspec(dllimport)
  #else
    #define DATARA_API
  #endif
#else
  #if defined(__GNUC__) && __GNUC__ >= 4
    #define DATARA_API __attribute__((visibility("default")))
  #else
    #define DATARA_API
  #endif
#endif

/* ========================================================================= */
/* Datara 1.0 Polyglot Types (C ABI)                                         */
/* ========================================================================= */

/**
 * Immutable zero-copy contiguous buffer slice.
 */
typedef struct datara_slice_t {
    const void* data;
    size_t len;
} datara_slice_t;

/**
 * Mutable zero-copy contiguous buffer slice.
 */
typedef struct datara_slice_mut_t {
    void* data;
    size_t len;
} datara_slice_mut_t;

/**
 * Zero-copy UTF-8 string view.
 */
typedef struct datara_string_t {
    const char* ptr;
    size_t len;
} datara_string_t;

/**
 * Datara 1.0 Outcome<T, E> Sum-Type Representation.
 * Memory layout: 4-byte discriminant + 4-byte alignment pad + union payload.
 */
typedef struct datara_outcome_t {
    int32_t is_ok;
    int32_t pad;
    union {
        int64_t val_int;
        double val_float;
        datara_string_t val_str;
        datara_slice_t val_slice;
        const char* err_msg;
    } payload;
} datara_outcome_t;

/**
 * Proof-Carrying Scheduler Future representation.
 */
typedef struct datara_future_t {
    int32_t state;      /* 0: pending, 1: ready, 2: failed */
    int32_t error_code;
    int64_t task_id;
    int64_t value;
} datara_future_t;

/* ========================================================================= */
/* Zero-Copy Inlined Constructors & Accessors                                */
/* ========================================================================= */

static inline datara_slice_t datara_slice_make(const void* data, size_t len) {
    datara_slice_t s;
    s.data = data;
    s.len = len;
    return s;
}

static inline datara_slice_mut_t datara_slice_mut_make(void* data, size_t len) {
    datara_slice_mut_t s;
    s.data = data;
    s.len = len;
    return s;
}

static inline datara_string_t datara_string_make(const char* ptr, size_t len) {
    datara_string_t s;
    s.ptr = ptr;
    s.len = len;
    return s;
}

static inline datara_outcome_t datara_outcome_ok_int(int64_t val) {
    datara_outcome_t o;
    o.is_ok = 1;
    o.pad = 0;
    o.payload.val_int = val;
    return o;
}

static inline datara_outcome_t datara_outcome_ok_float(double val) {
    datara_outcome_t o;
    o.is_ok = 1;
    o.pad = 0;
    o.payload.val_float = val;
    return o;
}

static inline datara_outcome_t datara_outcome_ok_str(const char* ptr, size_t len) {
    datara_outcome_t o;
    o.is_ok = 1;
    o.pad = 0;
    o.payload.val_str.ptr = ptr;
    o.payload.val_str.len = len;
    return o;
}

static inline datara_outcome_t datara_outcome_err(const char* err_msg) {
    datara_outcome_t o;
    o.is_ok = 0;
    o.pad = 0;
    o.payload.err_msg = err_msg;
    return o;
}

static inline bool datara_outcome_is_ok(const datara_outcome_t* out) {
    return out && out->is_ok != 0;
}

static inline int64_t datara_outcome_unwrap_int(const datara_outcome_t* out) {
    return out ? out->payload.val_int : 0;
}

static inline const char* datara_outcome_unwrap_err(const datara_outcome_t* out) {
    return out ? out->payload.err_msg : "";
}

/* ========================================================================= */
/* Datara Host Runtime API                                                   */
/* ========================================================================= */

/**
 * Initialize the Datara embed runtime.
 * Returns 0 on success, negative error code on failure.
 */
DATARA_API int32_t forgen_init(void);

/**
 * Load and compile a Datara module (.dtr) into process memory.
 * Returns 0 on success, -1 on failure (call forgen_last_error() for details).
 */
DATARA_API int32_t forgen_load_module(const char* module_path);

/**
 * Call a function exported by the loaded Datara module.
 *
 * Parameters:
 *   func_name: Null-terminated function name
 *   args: Array of 64-bit integer or pointer arguments
 *   arg_count: Number of arguments in `args` (up to 8)
 *   out_result: Pointer where the 64-bit return value is written
 *
 * Returns 0 on success, -1 on failure.
 */
DATARA_API int32_t forgen_call_fn(const char* func_name, const int64_t* args, size_t arg_count, int64_t* out_result);

/**
 * Shut down the Datara embed runtime, freeing loaded modules and JIT memory.
 * Returns 0 on success.
 */
DATARA_API int32_t forgen_shutdown(void);

/**
 * Returns the last error message as a null-terminated UTF-8 string.
 */
DATARA_API const char* forgen_last_error(void);

#ifdef __cplusplus
}
#endif

#endif /* DATARA_H */

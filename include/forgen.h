#ifndef FORGEN_H
#define FORGEN_H

#include <stdint.h>
#include <stddef.h>

#ifdef __cplusplus
extern "C" {
#endif

/**
 * Initialize the Forgen embed runtime.
 * Returns 0 on success, negative error code on failure.
 */
int32_t forgen_init(void);

/**
 * Load and compile a Datara module (.dtr) into process memory.
 * Returns 0 on success, -1 on failure (call forgen_last_error() for details).
 */
int32_t forgen_load_module(const char* module_path);

/**
 * Call a function exported by the loaded Datara module.
 *
 * Parameters:
 *   func_name: Null-terminated function name
 *   args: Array of 64-bit integer or pointer arguments
 *   arg_count: Number of arguments in `args`
 *   out_result: Pointer where the 64-bit return value is written
 *
 * Returns 0 on success, -1 on failure.
 */
int32_t forgen_call_fn(const char* func_name, const int64_t* args, size_t arg_count, int64_t* out_result);

/**
 * Shut down the Forgen embed runtime, freeing loaded modules and JIT memory.
 * Returns 0 on success.
 */
int32_t forgen_shutdown(void);

/**
 * Returns the last error message as a null-terminated UTF-8 string.
 */
const char* forgen_last_error(void);

#ifdef __cplusplus
}
#endif

#endif /* FORGEN_H */

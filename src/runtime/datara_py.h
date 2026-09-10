#ifndef DATARA_PY_H
#define DATARA_PY_H

#include <stdint.h>
#include <stddef.h>
#include "datara_runtime.h"

#ifdef __cplusplus
extern "C" {
#endif

// Initialization & Diagnostics
int32_t     datara_py_init(void);
int32_t     datara_py_is_available(void);
const char* datara_py_last_error(void);
void        datara_py_clear_error(void);
int32_t     py_is_available(void);
const char* py_last_error(void);
void        py_clear_error(void);

// Expression Evaluation & Execution
const char* datara_py_eval(const char* code);
const char* datara_py_eval_safe(const char* code);
int64_t     datara_py_eval_int(const char* code);
double      datara_py_eval_float(const char* code);
int64_t     datara_py_exec(const char* code);
const char* py_eval(const char* code);
const char* py_eval_safe(const char* code);
int64_t     py_eval_int(const char* code);
double      py_eval_float(const char* code);
int64_t     py_exec(const char* code);

// Module Import & Function Invocation
int64_t     datara_py_import(const char* module_name);
const char* datara_py_call(const char* fn_name, const char* args_json);
const char* datara_py_call_1_str(const char* fn_name, const char* arg0);
double      datara_py_call_1_float(const char* fn_name, double arg0);
int64_t     py_import(const char* module_name);
const char* py_call(const char* fn_name, const char* args_json);
const char* py_call_1_str(const char* fn_name, const char* arg0);
double      py_call_1_float(const char* fn_name, double arg0);

// Zero-Copy DataraMemoryView Interop (Requirement 7)
int32_t     datara_py_export_memview(const char* var_name, const DataraMemoryView* view);
int64_t     datara_py_export_list_f64(const char* var_name, int64_t* list);
int64_t     datara_py_assert_same_ptr(const char* var_name, int64_t* list);
int32_t     datara_py_test_zerocopy(void);

// Self-Test Harness
int32_t     datara_py_self_test(void);

#ifdef __cplusplus
}
#endif

#endif // DATARA_PY_H

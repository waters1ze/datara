#ifndef DATARA_NAPI_H
#define DATARA_NAPI_H

#include <stddef.h>
#include <stdint.h>
#include <stdbool.h>

#ifdef __cplusplus
extern "C" {
#endif

// Status codes
typedef enum {
    napi_ok = 0,
    napi_invalid_arg,
    napi_object_expected,
    napi_string_expected,
    napi_name_expected,
    napi_function_expected,
    napi_number_expected,
    napi_boolean_expected,
    napi_array_expected,
    napi_generic_failure,
    napi_pending_exception,
    napi_cancelled,
    napi_escape_called_twice,
    napi_handle_scope_mismatch,
    napi_callback_scope_mismatch,
    napi_queue_full,
    napi_closing,
    napi_bigint_expected,
    napi_date_expected,
    napi_arraybuffer_expected,
    napi_detachable_arraybuffer_expected,
    napi_would_deadlock
} napi_status;

#define NAPI_AUTO_LENGTH SIZE_MAX

// Opaque handles
typedef struct napi_env__* napi_env;
typedef struct napi_value__* napi_value;
typedef struct napi_ref__* napi_ref;
typedef struct napi_handle_scope__* napi_handle_scope;
struct napi_callback_info__ {
    size_t argc;
    napi_value* argv;
    napi_value this_arg;
    void* data;
};
typedef struct napi_callback_info__* napi_callback_info;

typedef napi_value (*napi_callback)(napi_env env, napi_callback_info info);

// Function table passed to addons
typedef struct napi_table {
    napi_status (*napi_create_string_utf8)(napi_env env, const char* str, size_t length, napi_value* result);
    napi_status (*napi_get_value_string_utf8)(napi_env env, napi_value value, char* buf, size_t bufsize, size_t* result);
    napi_status (*napi_create_int64)(napi_env env, int64_t value, napi_value* result);
    napi_status (*napi_get_value_int64)(napi_env env, napi_value value, int64_t* result);
    napi_status (*napi_create_double)(napi_env env, double value, napi_value* result);
    napi_status (*napi_get_value_double)(napi_env env, napi_value value, double* result);
    napi_status (*napi_create_object)(napi_env env, napi_value* result);
    napi_status (*napi_set_named_property)(napi_env env, napi_value object, const char* utf8name, napi_value value);
    napi_status (*napi_get_named_property)(napi_env env, napi_value object, const char* utf8name, napi_value* result);
    napi_status (*napi_create_function)(napi_env env, const char* utf8name, size_t length, napi_callback cb, void* data, napi_value* result);
    napi_status (*napi_get_cb_info)(napi_env env, napi_callback_info cbinfo, size_t* argc, napi_value* argv, napi_value* this_arg, void** data);
    napi_status (*napi_call_function)(napi_env env, napi_value recv, napi_value func, size_t argc, const napi_value* argv, napi_value* result);
    napi_status (*napi_create_buffer)(napi_env env, size_t length, void** data, napi_value* result);
    napi_status (*napi_get_buffer_info)(napi_env env, napi_value value, void** data, size_t* length);
    napi_status (*napi_throw_error)(napi_env env, const char* code, const char* msg);
} napi_table;

struct napi_env__ {
    const napi_table* api;
    void* js_scope;
    void* last_error;
};

// Dispatch macros enabling zero-link addon compilation
#ifndef DATARA_NAPI_NO_MACROS
#define napi_create_string_utf8(env, str, len, res) ((env)->api->napi_create_string_utf8(env, str, len, res))
#define napi_get_value_string_utf8(env, val, buf, sz, res) ((env)->api->napi_get_value_string_utf8(env, val, buf, sz, res))
#define napi_create_int64(env, val, res) ((env)->api->napi_create_int64(env, val, res))
#define napi_get_value_int64(env, val, res) ((env)->api->napi_get_value_int64(env, val, res))
#define napi_create_double(env, val, res) ((env)->api->napi_create_double(env, val, res))
#define napi_get_value_double(env, val, res) ((env)->api->napi_get_value_double(env, val, res))
#define napi_create_object(env, res) ((env)->api->napi_create_object(env, res))
#define napi_set_named_property(env, obj, name, val) ((env)->api->napi_set_named_property(env, obj, name, val))
#define napi_get_named_property(env, obj, name, res) ((env)->api->napi_get_named_property(env, obj, name, res))
#define napi_create_function(env, name, len, cb, data, res) ((env)->api->napi_create_function(env, name, len, cb, data, res))
#define napi_get_cb_info(env, info, argc, argv, this_arg, data) ((env)->api->napi_get_cb_info(env, info, argc, argv, this_arg, data))
#define napi_call_function(env, recv, fn, argc, argv, res) ((env)->api->napi_call_function(env, recv, fn, argc, argv, res))
#define napi_create_buffer(env, len, data, res) ((env)->api->napi_create_buffer(env, len, data, res))
#define napi_get_buffer_info(env, val, data, len) ((env)->api->napi_get_buffer_info(env, val, data, len))
#define napi_throw_error(env, code, msg) ((env)->api->napi_throw_error(env, code, msg))
#endif

#ifdef _WIN32
#define NAPI_MODULE_EXPORT __declspec(dllexport)
#else
#define NAPI_MODULE_EXPORT __attribute__((visibility("default")))
#endif

#define NAPI_MODULE_INIT() \
    NAPI_MODULE_EXPORT napi_value napi_register_module_v1(napi_env env, napi_value exports)

// Runtime FFI exports
struct DJSVal;
struct DJSVal* datara_napi_load_addon(const char* path);
int32_t        datara_napi_is_available(void);
const char*    datara_napi_last_error(void);

#ifdef __cplusplus
}
#endif

#endif // DATARA_NAPI_H

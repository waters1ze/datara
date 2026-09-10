#ifndef DATARA_RUNTIME_H
#define DATARA_RUNTIME_H

#include <stdint.h>
#include <stddef.h>
#include "datara_rt_scheduler.h"

#ifdef __cplusplus
extern "C" {
#endif

// Console Output & Input
void        datara_rt_out_int(int64_t v);
void        datara_rt_out_bool(int64_t v);
const char* datara_rt_bool_to_str(int64_t v);
void        datara_rt_out_float(double v);
const char* datara_rt_float_to_str(double v);
void        datara_rt_out_str(const char* s);
void        datara_rt_out_dec64(int64_t v);
void        datara_rt_err(const char* s);
void        datara_rt_exit(int32_t code);
void        datara_rt_panic(const char* s);
void        datara_rt_print_backtrace(void);
void        datara_rt_println(const char* s);
void        datara_rt_print(const char* s);
void        datara_rt_eprintln(const char* s);
void        datara_rt_assert(int64_t cond, const char* msg);
int64_t     datara_rt_len(const char* s);
const char* datara_rt_input(const char* prompt);
int64_t     datara_rt_input_int(const char* prompt);
double      datara_rt_input_float(const char* prompt);

// ============================================================================
// Runtime ABI Versioning (Requirement 15)
// ============================================================================
#define DATARA_RT_ABI_VERSION 1u
uint32_t    datara_rt_abi_version(void);

// ============================================================================
// Datara Universal Polyglot Value (DataraValue) & 64-bit NaN-Boxing
// ============================================================================
typedef uint64_t DataraValue;

#define DATARA_QNAN_MASK      0xFFF8000000000000ULL
#define DATARA_QNAN_PREFIX    0x7FF8000000000000ULL
#define DATARA_TAG_MASK       0x0007000000000000ULL
#define DATARA_PAYLOAD_MASK   0x0000FFFFFFFFFFFFULL

#define DATARA_TAG_INT        0x0001000000000000ULL
#define DATARA_TAG_BOOL       0x0002000000000000ULL
#define DATARA_TAG_STR        0x0003000000000000ULL
#define DATARA_TAG_RAWPTR     0x0004000000000000ULL
#define DATARA_TAG_HANDLE     0x0005000000000000ULL
#define DATARA_TAG_NULL       0x0006000000000000ULL
#define DATARA_TAG_UNDEFINED  0x0007000000000000ULL

// 64-bit NaN-Boxing Runtime Support (Legacy & Polyglot Universal API)
uint64_t    datara_rt_nanbox_int(int64_t val);
int64_t     datara_rt_nanunbox_int(uint64_t box);
uint64_t    datara_rt_nanbox_bool(int64_t b);
int64_t     datara_rt_nanunbox_bool(uint64_t box);
uint64_t    datara_rt_nanbox_str(const char* s);
const char* datara_rt_nanunbox_str(uint64_t box);
void        datara_rt_out_val(uint64_t box);

// Universal DataraValue Constructors
DataraValue datara_val_from_float(double f);
DataraValue datara_val_from_int(int64_t val);
DataraValue datara_val_from_bool(int32_t b);
DataraValue datara_val_from_str(const char* s);
DataraValue datara_val_from_rawptr(void* ptr);
DataraValue datara_val_from_handle(uint32_t handle_id);
DataraValue datara_val_null(void);
DataraValue datara_val_undefined(void);

// Universal DataraValue Type Predicates
int32_t     datara_val_is_float(DataraValue v);
int32_t     datara_val_is_int(DataraValue v);
int32_t     datara_val_is_bool(DataraValue v);
int32_t     datara_val_is_str(DataraValue v);
int32_t     datara_val_is_rawptr(DataraValue v);
int32_t     datara_val_is_handle(DataraValue v);
int32_t     datara_val_is_null(DataraValue v);
int32_t     datara_val_is_undefined(DataraValue v);

// Universal DataraValue Unboxers
double      datara_val_to_float(DataraValue v);
int64_t     datara_val_to_int(DataraValue v);
int32_t     datara_val_to_bool(DataraValue v);
const char* datara_val_to_str(DataraValue v);
void*       datara_val_to_rawptr(DataraValue v);
uint32_t    datara_val_to_handle(DataraValue v);

// Foreign Handle Table Management (Thread-safe, mutex-guarded, auto-growing)
uint32_t    datara_rt_handle_alloc(void* ptr, const char* type_tag, void (*destructor)(void*));
void*       datara_rt_handle_get(uint32_t handle_id);
const char* datara_rt_handle_tag(uint32_t handle_id);
int32_t     datara_rt_handle_free(uint32_t handle_id);
void        datara_rt_handle_shutdown(void);
size_t      datara_rt_handle_count(void);

// ============================================================================
// DataraMemoryView (Multi-dimensional Buffer / Interop Spec)
// ============================================================================
typedef enum {
    DATARA_DTYPE_INT8    = 1,
    DATARA_DTYPE_UINT8   = 2,
    DATARA_DTYPE_INT16   = 3,
    DATARA_DTYPE_UINT16  = 4,
    DATARA_DTYPE_INT32   = 5,
    DATARA_DTYPE_UINT32  = 6,
    DATARA_DTYPE_INT64   = 7,
    DATARA_DTYPE_UINT64  = 8,
    DATARA_DTYPE_FLOAT32 = 9,
    DATARA_DTYPE_FLOAT64 = 10,
    DATARA_DTYPE_BOOL    = 11
} DataraDType;

typedef struct {
    void*       data;
    size_t      total_bytes;
    int32_t     ndim;
    int64_t     shape[8];
    int64_t     strides[8];
    DataraDType element_type;
} DataraMemoryView;

size_t           datara_dtype_itemsize(DataraDType dtype);
DataraMemoryView datara_memview_1d(void* data, int64_t length, DataraDType dtype);
DataraMemoryView datara_memview_2d(void* data, int64_t rows, int64_t cols, DataraDType dtype);
DataraMemoryView datara_memview_from_list_f64(int64_t* list);
DataraMemoryView datara_memview_from_vector(void* vec_data, int64_t len, DataraDType dtype);
int64_t          datara_memview_get_f64(const DataraMemoryView* view, int64_t idx, double* out);
int64_t          datara_memview_set_f64(DataraMemoryView* view, int64_t idx, double val);
int64_t          datara_memview_get_i64(const DataraMemoryView* view, int64_t idx, int64_t* out);
int64_t          datara_memview_set_i64(DataraMemoryView* view, int64_t idx, int64_t val);

// C Runtime Self-Test Harness for Polyglot Foundation
int32_t          datara_rt_polyglot_foundation_test(void);

// C FFI Flat Buffer Round-trip Test Helpers
int64_t          datara_rt_test_list_f64_multiply(int64_t* list, double factor);
int64_t          datara_rt_test_list_f64_sum(int64_t* list);

// Ultra-Fast Direct Streaming Terminal I/O
void        datara_rt_print_str(const char* s);
void        datara_rt_print_int(int64_t v);
void        datara_rt_print_float(double v);
void        datara_rt_print_bool(int64_t v);
void        datara_rt_print_space(void);
void        datara_rt_print_newline(void);
void        datara_rt_flush(void);
void        datara_rt_print_list(void* list);
void        datara_rt_set_capture(int32_t enable);
const char* datara_rt_get_capture(void);
void        datara_rt_clear_capture(void);

// String Operations
const char* datara_rt_int_to_str(int64_t v);
const char* datara_rt_str_concat(const char* a, const char* b);
const char* datara_rt_str_concat_3(const char* a, const char* b, const char* c);
const char* datara_rt_str_concat_4(const char* a, const char* b, const char* c, const char* d);
const char* datara_rt_str_concat_5(const char* a, const char* b, const char* c, const char* d, const char* e);
int64_t     datara_rt_str_eq(const char* a, const char* b);
int64_t     datara_rt_str_len(const char* s);
int64_t     datara_rt_byte_len(const char* s);
int64_t     datara_rt_str_chars(const char* s);
int64_t     datara_rt_char_len(const char* s);
int32_t     datara_rt_validate_utf8(const char* s);
const char* datara_rt_str_sanitize_utf8(const char* s);
const char* datara_rt_str_next_scalar(const char* s, int64_t* inout_offset);
const char* datara_rt_str_scalar_at(const char* s, int64_t offset);
int64_t     datara_rt_str_byte_at(const char* s, int64_t idx);
int64_t     datara_rt_str_next_offset(const char* s, int64_t current_offset);
int64_t     datara_rt_str_contains(const char* s, const char* sub);
int64_t     datara_rt_str_starts_with(const char* s, const char* pre);
int64_t     datara_rt_str_ends_with(const char* s, const char* suf);
int64_t     datara_rt_str_index_of(const char* s, const char* sub);
const char* datara_rt_str_trim(const char* s);
int64_t     datara_rt_str_to_int(const char* s);
double      datara_rt_str_to_float(const char* s);
const char* datara_rt_str_substring(const char* s, int64_t start, int64_t len);
const char* datara_rt_str_char_at(const char* s, int64_t idx);
const char* datara_rt_str_repeat(const char* s, int64_t count);
const char* datara_rt_str_pad_left(const char* s, int64_t total_len, const char* pad);
const char* datara_rt_str_pad_right(const char* s, int64_t total_len, const char* pad);
const char* datara_rt_str_replace(const char* s, const char* target, const char* replacement);
const char* datara_rt_str_to_upper(const char* s);
const char* datara_rt_str_to_lower(const char* s);
int64_t*    datara_rt_str_split(const char* s, const char* delim);
const char* datara_rt_str_join(const int64_t* list, const char* delim);
const char* datara_rt_format_percent(double val, int64_t decimals);
const char* datara_rt_format_int_with_commas(int64_t n);
const char* datara_rt_format_str_i64_str_i64(const char* s1, int64_t n1, const char* s2, int64_t n2);
const char* datara_rt_range_str(int64_t start, int64_t end);

// High-Speed JavaScript & Node.js Interop
const char* datara_js_eval(const char* code);
int64_t     datara_js_eval_int(const char* code);
double      datara_js_eval_float(const char* code);
int64_t     datara_js_require(const char* module_name);
const char* datara_js_call(const char* fn_name, const char* args_json);
const char* datara_js_call_0(const char* fn_name);
const char* datara_js_call_1(const char* fn_name, const char* arg0);
const char* datara_js_call_2(const char* fn_name, const char* arg0, const char* arg1);
int64_t     datara_js_set_global(const char* name, const char* json_val);
const char* datara_js_get_global(const char* name);

// File I/O
const char* datara_rt_file_read(const char* path);
int64_t     datara_rt_file_write(const char* path, const char* content);
int64_t     datara_rt_file_append(const char* path, const char* content);
int64_t     datara_rt_file_exists(const char* path);

// Zero-Trust Capability-Based OS I/O & Resources
void*       datara_rt_sys_caps_create(void);
void*       datara_rt_files_grant_readonly(void* prov, const char* path);
void*       datara_rt_files_grant_readwrite(void* prov, const char* path);
void*       datara_rt_net_grant_connect(void* prov, const char* host, int64_t port);
void*       datara_rt_file_open(void* token, const char* path);
const char* datara_rt_file_read_all(void* handle);
int64_t     datara_rt_file_close(void* handle);

// System, Environment & Timing
void        datara_rt_sleep(int64_t ms);
int64_t     datara_rt_now_ms(void);
int64_t     datara_rt_now_unix_ms(void);
int64_t     datara_rt_now_precise_ms(void);
int64_t     datara_rt_now_ns(void);
int64_t     now_ns(void);
int64_t     now_ms(void);
const char* datara_rt_env_get(const char* key);
const char* datara_rt_path_join(const char* a, const char* b);
void        datara_rt_set_args(int32_t argc, char** argv);
int64_t     datara_rt_args_count(void);
const char* datara_rt_args_get(int64_t index);

// Collections & Data Structures
int64_t*    datara_rt_list_create(int64_t cap);
int64_t*    datara_rt_list_create_capacity(int64_t cap);
int64_t*    datara_rt_list_create_1(int64_t a);
int64_t*    datara_rt_list_create_2(int64_t a, int64_t b);
int64_t*    datara_rt_list_create_3(int64_t a, int64_t b, int64_t c);
int64_t*    datara_rt_list_create_4(int64_t a, int64_t b, int64_t c, int64_t d);
int64_t*    datara_rt_list_create_5(int64_t a, int64_t b, int64_t c, int64_t d, int64_t e);
int64_t*    datara_rt_list_append(int64_t* list, int64_t val);
int64_t     datara_rt_list_get(int64_t* list, int64_t idx);
int64_t     datara_rt_list_get_unchecked(int64_t* list, int64_t idx);
int64_t     datara_rt_list_len(int64_t* list);
int64_t*    datara_rt_list_set(int64_t* list, int64_t idx, int64_t v);
int64_t*    datara_rt_list_set_unchecked(int64_t* list, int64_t idx, int64_t v);
int64_t     datara_rt_list_pop(int64_t* list);
int64_t*    datara_rt_slice(int64_t* list, int64_t start, int64_t end);
int64_t*    datara_rt_list_create_repeat(int64_t elem, int64_t count);
void*       datara_rt_map_create(void);
int64_t*    datara_rt_map_insert(int64_t* map, const char* key, int64_t val);
int64_t     datara_rt_map_get(int64_t* map, const char* key);
void*       datara_rt_map_create_1(const char* k0, int64_t v0);
int64_t*    datara_rt_map_create_2(const char* k0, int64_t v0, const char* k1, int64_t v1);
int64_t*    datara_rt_map_create_3(const char* k0, int64_t v0, const char* k1, int64_t v1,
                                   const char* k2, int64_t v2);
int64_t*    datara_rt_map_create_4(const char* k0, int64_t v0, const char* k1, int64_t v1,
                                   const char* k2, int64_t v2, const char* k3, int64_t v3);
int64_t*    datara_rt_map_create_5(const char* k0, int64_t v0, const char* k1, int64_t v1,
                                   const char* k2, int64_t v2, const char* k3, int64_t v3,
                                   const char* k4, int64_t v4);
void        datara_rt_map_free(void* map);

// Network Sockets
int64_t     datara_rt_socket_create(int64_t is_tcp);
int64_t     datara_rt_socket_bind(int64_t sock, const char* host, int64_t port);
int64_t     datara_rt_socket_listen(int64_t sock, int64_t backlog);
int64_t     datara_rt_socket_accept(int64_t sock);
int64_t     datara_rt_socket_connect(int64_t sock, const char* host, int64_t port);
int64_t     datara_rt_socket_send(int64_t sock, const char* data);
const char* datara_rt_socket_recv(int64_t sock, int64_t max_bytes);
void        datara_rt_socket_close(int64_t sock);
const char* datara_rt_http_get(const char* url);

// Cryptography & Entropy
const char* datara_rt_sha256(const char* input);
const char* datara_rt_base64_encode(const char* input);
const char* datara_rt_base64_decode(const char* input);
int64_t     datara_rt_random_bytes(uint8_t* buf, int64_t len);
int         datara_rt_rng_is_insecure(void); /* 1 if random_bytes fell back to the insecure clock-seeded LCG */
const char* datara_rt_uuid_v4(void);

// Native UI Dialogs (Cross-platform)
int64_t     datara_rt_dialog_info(const char* title, const char* msg);
int64_t     datara_rt_dialog_alert(const char* title, const char* msg);
int64_t     datara_rt_dialog_confirm(const char* title, const char* msg);

// Process & System
int64_t     datara_rt_system(const char* cmd);
const char* datara_rt_exec(const char* cmd);

// High-Performance Fast Math
double      datara_rt_math_sqrt(double x);
double      datara_rt_math_pow(double base, double exp);
double      datara_rt_math_abs(double x);
double      datara_rt_math_sin(double x);
double      datara_rt_math_cos(double x);
double      datara_rt_math_tan(double x);
double      datara_rt_math_floor(double x);
double      datara_rt_math_ceil(double x);
double      datara_rt_math_round(double x);
double      datara_rt_math_min(double a, double b);
double      datara_rt_math_max(double a, double b);
double      datara_rt_math_clamp(double val, double min_val, double max_val);
double      datara_rt_math_hypot(double a, double b);
double      datara_rt_math_log(double x);
double      datara_rt_math_exp(double x);
int64_t     datara_rt_math_min_int(int64_t a, int64_t b);
int64_t     datara_rt_math_max_int(int64_t a, int64_t b);
int64_t     datara_rt_math_clamp_int(int64_t val, int64_t min_val, int64_t max_val);
int64_t     datara_rt_math_abs_int(int64_t x);
int64_t     datara_rt_math_ctz(int64_t x);
int64_t     datara_rt_math_shr(int64_t v, int64_t s);
int64_t     datara_rt_math_shl(int64_t v, int64_t s);
int64_t     datara_rt_math_xor(int64_t a, int64_t b);
int64_t     datara_rt_math_and(int64_t a, int64_t b);
int64_t     datara_rt_math_or(int64_t a, int64_t b);

// First-Class Hardware Accelerated SIMD (AVX2 / NEON)
typedef struct { float x, y, z, w; } DataraFloat4;
typedef struct { int32_t x, y, z, w; } DataraInt4;
DataraFloat4 datara_rt_float4(double x, double y, double z, double w);
DataraInt4   datara_rt_int4(int64_t x, int64_t y, int64_t z, int64_t w);
double       datara_rt_float4_dot(DataraFloat4 a, DataraFloat4 b);
DataraFloat4 datara_rt_float4_min4(DataraFloat4 a, DataraFloat4 b);
DataraFloat4 datara_rt_float4_max4(DataraFloat4 a, DataraFloat4 b);
DataraInt4   datara_rt_int4_min4(DataraInt4 a, DataraInt4 b);
DataraInt4   datara_rt_int4_max4(DataraInt4 a, DataraInt4 b);
int          datara_rt_simd_enabled(void);
void        datara_rt_thread_pool_init(int64_t workers);
int64_t     datara_rt_num_workers(void);
void        datara_rt_parallel_for(int64_t start, int64_t end, void (*fn)(int64_t idx, void* ctx), void* ctx);
void        datara_rt_parallel_invoke(void (*fn1)(void* ctx1), void* ctx1, void (*fn2)(void* ctx2), void* ctx2);

// Memory Management, Ephemeral Frame Arena & RAII
void*       datara_rt_arena_alloc(int64_t bytes);
int64_t     datara_rt_arena_checkpoint(void);
void        datara_rt_arena_reset(int64_t saved_top);
void        datara_rt_free(void* ptr);
void        datara_rt_str_free(const char* s);
void        datara_rt_list_free(void* list);

// Graduated Ownership Runtime Guards (Per-thread 32-bit refcount slots)
int64_t     datara_rt_own_acquire(int64_t val);
void        datara_rt_own_release(int64_t val);

// Integer Overflow Policy (checked traps, wrapping, saturating)
void        datara_rt_overflow_panic(void);
void        datara_rt_div_zero_panic(void);
int64_t     datara_rt_checked_add(int64_t a, int64_t b);
int64_t     datara_rt_checked_sub(int64_t a, int64_t b);
int64_t     datara_rt_checked_mul(int64_t a, int64_t b);
int64_t     datara_rt_checked_div(int64_t a, int64_t b);
int64_t     datara_rt_checked_rem(int64_t a, int64_t b);
int64_t     datara_rt_saturating_add(int64_t a, int64_t b);
int64_t     datara_rt_saturating_sub(int64_t a, int64_t b);
int64_t     datara_rt_saturating_mul(int64_t a, int64_t b);
int64_t     datara_rt_wrapping_add(int64_t a, int64_t b);
int64_t     datara_rt_wrapping_sub(int64_t a, int64_t b);
int64_t     datara_rt_wrapping_mul(int64_t a, int64_t b);

#ifdef __cplusplus
}
#endif

#endif // DATARA_RUNTIME_H

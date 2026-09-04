#ifndef DATARA_RUNTIME_H
#define DATARA_RUNTIME_H

#include <stdint.h>
#include <stddef.h>

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
const char* datara_rt_input(const char* prompt);
int64_t     datara_rt_input_int(const char* prompt);
double      datara_rt_input_float(const char* prompt);

// Ultra-Fast Direct Streaming Terminal I/O
void        datara_rt_print_str(const char* s);
void        datara_rt_print_int(int64_t v);
void        datara_rt_print_float(double v);
void        datara_rt_print_bool(int64_t v);
void        datara_rt_print_space(void);
void        datara_rt_print_newline(void);
void        datara_rt_flush(void);
void        datara_rt_print_list(void* list);

// String Operations
const char* datara_rt_int_to_str(int64_t v);
const char* datara_rt_str_concat(const char* a, const char* b);
const char* datara_rt_str_concat_3(const char* a, const char* b, const char* c);
const char* datara_rt_str_concat_4(const char* a, const char* b, const char* c, const char* d);
const char* datara_rt_str_concat_5(const char* a, const char* b, const char* c, const char* d, const char* e);
int64_t     datara_rt_str_eq(const char* a, const char* b);
int64_t     datara_rt_str_len(const char* s);
int64_t     datara_rt_str_contains(const char* s, const char* sub);
int64_t     datara_rt_str_starts_with(const char* s, const char* pre);
int64_t     datara_rt_str_ends_with(const char* s, const char* suf);
int64_t     datara_rt_str_index_of(const char* s, const char* sub);
const char* datara_rt_str_trim(const char* s);
int64_t     datara_rt_str_to_int(const char* s);
double      datara_rt_str_to_float(const char* s);
const char* datara_rt_str_substring(const char* s, int64_t start, int64_t len);
const char* datara_rt_str_char_at(const char* s, int64_t idx);

// File I/O
const char* datara_rt_file_read(const char* path);
int64_t     datara_rt_file_write(const char* path, const char* content);
int64_t     datara_rt_file_append(const char* path, const char* content);
int64_t     datara_rt_file_exists(const char* path);

// System, Environment & Timing
void        datara_rt_sleep(int64_t ms);
int64_t     datara_rt_now_ms(void);
int64_t     datara_rt_now_unix_ms(void);
int64_t     datara_rt_now_precise_ms(void);
int64_t     now_ms(void);
const char* datara_rt_env_get(const char* key);
void        datara_rt_set_args(int32_t argc, char** argv);
int64_t     datara_rt_args_count(void);
const char* datara_rt_args_get(int64_t index);

// Collections & Data Structures
int64_t*    datara_rt_list_create(int64_t cap);
int64_t*    datara_rt_list_create_capacity(int64_t cap);
int64_t*    datara_rt_list_append(int64_t* list, int64_t val);
int64_t     datara_rt_list_len(int64_t* list);
int64_t     datara_rt_list_get(int64_t* list, int64_t idx);
int64_t     datara_rt_list_pop(int64_t* list);
void*       datara_rt_map_create(void);
int64_t*    datara_rt_map_insert(int64_t* map, const char* key, int64_t val);
int64_t     datara_rt_map_get(int64_t* map, const char* key);
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
const char* datara_rt_http_get(void);

// Cryptography
const char* datara_rt_sha256(const char* input);
const char* datara_rt_base64_encode(const char* input);
const char* datara_rt_base64_decode(const char* input);

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
double      datara_rt_math_hypot(double a, double b);
int64_t     datara_rt_math_min_int(int64_t a, int64_t b);
int64_t     datara_rt_math_max_int(int64_t a, int64_t b);
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

#ifdef __cplusplus
}
#endif

#endif // DATARA_RUNTIME_H

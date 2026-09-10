#include "datara_runtime.h"
#include "datara_js.h"
#include "datara_napi.h"

#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <stdint.h>
#include <stdbool.h>
#include <math.h>
#include <ctype.h>

#ifdef _WIN32
#ifndef WIN32_LEAN_AND_MEAN
#define WIN32_LEAN_AND_MEAN
#endif
#include <windows.h>
#include <winsock2.h>
#include <ws2tcpip.h>
#include <direct.h>
#define djs_mkdir(p) _mkdir(p)
#else
#include <sys/socket.h>
#include <netinet/in.h>
#include <arpa/inet.h>
#include <unistd.h>
#include <sys/stat.h>
#define closesocket close
#define SOCKET int
#define INVALID_SOCKET -1
#define SOCKET_ERROR -1
#define djs_mkdir(p) mkdir(p, 0755)
#endif

// ---------------------------------------------------------------------------
// Engine Global State
// ---------------------------------------------------------------------------

DJSScope* g_djs_global_scope = NULL;
static int g_djs_initialized = 0;

static DJSVal* g_djs_fs_module = NULL;
static DJSVal* g_djs_path_module = NULL;
static DJSVal* g_djs_crypto_module = NULL;
static DJSVal* g_djs_events_module = NULL;
static DJSVal* g_djs_http_module = NULL;

// Forward declarations
static DJSVal* djs_eval_internal(DJSScope* scope, const char* code);
static DJSVal* djs_call_val(DJSVal* fn, DJSVal* this_val, int argc, DJSVal** argv);

// ---------------------------------------------------------------------------
// Microtask Queue & Promise Internals
// ---------------------------------------------------------------------------

typedef struct DJSMicrotask {
    DJSVal* fn;
    DJSVal* arg;
    DJSVal* chained_promise;
    struct DJSMicrotask* next;
} DJSMicrotask;

static DJSMicrotask* g_microtask_head = NULL;
static DJSMicrotask* g_microtask_tail = NULL;

static void djs_enqueue_microtask(DJSVal* fn, DJSVal* arg, DJSVal* chained_promise) {
    if (!fn) return;
    DJSMicrotask* task = (DJSMicrotask*)malloc(sizeof(DJSMicrotask));
    if (!task) return;
    task->fn = fn;
    task->arg = arg;
    task->chained_promise = chained_promise;
    task->next = NULL;

    if (!g_microtask_tail) {
        g_microtask_head = task;
        g_microtask_tail = task;
    } else {
        g_microtask_tail->next = task;
        g_microtask_tail = task;
    }
}

static void djs_promise_resolve(DJSVal* promise, DJSVal* val);
static void djs_promise_reject(DJSVal* promise, DJSVal* reason);

static void djs_drain_microtasks(void) {
    while (g_microtask_head) {
        DJSMicrotask* task = g_microtask_head;
        g_microtask_head = task->next;
        if (!g_microtask_head) g_microtask_tail = NULL;

        DJSVal* argv[1] = { task->arg ? task->arg : djs_val_undefined() };
        DJSVal* res = djs_call_val(task->fn, NULL, 1, argv);

        if (task->chained_promise) {
            djs_promise_resolve(task->chained_promise, res ? res : djs_val_undefined());
        }
        free(task);
    }
}

static void djs_promise_resolve(DJSVal* p, DJSVal* val) {
    if (!p || p->type != DJS_PROMISE || p->u.promise.state != DJS_PROMISE_PENDING) return;
    p->u.promise.state = DJS_PROMISE_FULFILLED;
    p->u.promise.result = val ? val : djs_val_undefined();

    for (int i = 0; i < p->u.promise.then_count; i++) {
        djs_enqueue_microtask(p->u.promise.then_fns[i], p->u.promise.result, NULL);
    }
    p->u.promise.then_count = 0;
}

static void djs_promise_reject(DJSVal* p, DJSVal* reason) {
    if (!p || p->type != DJS_PROMISE || p->u.promise.state != DJS_PROMISE_PENDING) return;
    p->u.promise.state = DJS_PROMISE_REJECTED;
    p->u.promise.result = reason ? reason : djs_val_undefined();

    for (int i = 0; i < p->u.promise.catch_count; i++) {
        djs_enqueue_microtask(p->u.promise.catch_fns[i], p->u.promise.result, NULL);
    }
    p->u.promise.catch_count = 0;
}

static DJSVal* djs_promise_add_then(DJSVal* p, DJSVal* on_fulfilled, DJSVal* on_rejected) {
    if (!p || p->type != DJS_PROMISE) return djs_val_undefined();
    DJSVal* next_p = djs_val_promise();

    if (p->u.promise.state == DJS_PROMISE_FULFILLED) {
        if (on_fulfilled) {
            djs_enqueue_microtask(on_fulfilled, p->u.promise.result, next_p);
        } else {
            djs_promise_resolve(next_p, p->u.promise.result);
        }
    } else if (p->u.promise.state == DJS_PROMISE_REJECTED) {
        if (on_rejected) {
            djs_enqueue_microtask(on_rejected, p->u.promise.result, next_p);
        } else {
            djs_promise_reject(next_p, p->u.promise.result);
        }
    } else {
        // Pending: store callbacks
        if (on_fulfilled) {
            if (p->u.promise.then_count >= p->u.promise.then_cap) {
                p->u.promise.then_cap *= 2;
                p->u.promise.then_fns = (DJSVal**)realloc(p->u.promise.then_fns, sizeof(DJSVal*) * p->u.promise.then_cap);
            }
            p->u.promise.then_fns[p->u.promise.then_count++] = on_fulfilled;
        }
        if (on_rejected) {
            if (p->u.promise.catch_count >= p->u.promise.catch_cap) {
                p->u.promise.catch_cap *= 2;
                p->u.promise.catch_fns = (DJSVal**)realloc(p->u.promise.catch_fns, sizeof(DJSVal*) * p->u.promise.catch_cap);
            }
            p->u.promise.catch_fns[p->u.promise.catch_count++] = on_rejected;
        }
    }
    return next_p;
}

// ---------------------------------------------------------------------------
// Native Buffer Methods
// ---------------------------------------------------------------------------

static DJSVal* djs_native_buf_read_uint8(DJSVal* this_val, int argc, DJSVal** argv) {
    if (!this_val || this_val->type != DJS_BUFFER) return djs_val_int(0);
    size_t offset = (argc > 0 && argv[0]->type == DJS_INT) ? (size_t)argv[0]->u.i : 0;
    if (offset >= this_val->u.buffer.length) return djs_val_int(0);
    return djs_val_int((int64_t)this_val->u.buffer.data[offset]);
}

static DJSVal* djs_native_buf_write_uint8(DJSVal* this_val, int argc, DJSVal** argv) {
    if (!this_val || this_val->type != DJS_BUFFER || argc < 1) return djs_val_undefined();
    uint8_t byte_val = (uint8_t)(argv[0]->type == DJS_INT ? argv[0]->u.i : 0);
    size_t offset = (argc > 1 && argv[1]->type == DJS_INT) ? (size_t)argv[1]->u.i : 0;
    if (offset < this_val->u.buffer.length) {
        this_val->u.buffer.data[offset] = byte_val;
    }
    return djs_val_int((int64_t)(offset + 1));
}

static DJSVal* djs_native_buf_read_int32_le(DJSVal* this_val, int argc, DJSVal** argv) {
    if (!this_val || this_val->type != DJS_BUFFER) return djs_val_int(0);
    size_t offset = (argc > 0 && argv[0]->type == DJS_INT) ? (size_t)argv[0]->u.i : 0;
    if (offset + 4 > this_val->u.buffer.length) return djs_val_int(0);
    int32_t val;
    memcpy(&val, this_val->u.buffer.data + offset, 4);
    return djs_val_int((int64_t)val);
}

static DJSVal* djs_native_buf_write_int32_le(DJSVal* this_val, int argc, DJSVal** argv) {
    if (!this_val || this_val->type != DJS_BUFFER || argc < 1) return djs_val_undefined();
    int32_t val = (int32_t)(argv[0]->type == DJS_INT ? argv[0]->u.i : (int64_t)argv[0]->u.f);
    size_t offset = (argc > 1 && argv[1]->type == DJS_INT) ? (size_t)argv[1]->u.i : 0;
    if (offset + 4 <= this_val->u.buffer.length) {
        memcpy(this_val->u.buffer.data + offset, &val, 4);
    }
    return djs_val_int((int64_t)(offset + 4));
}

static DJSVal* djs_native_buf_read_double_le(DJSVal* this_val, int argc, DJSVal** argv) {
    if (!this_val || this_val->type != DJS_BUFFER) return djs_val_float(0.0);
    size_t offset = (argc > 0 && argv[0]->type == DJS_INT) ? (size_t)argv[0]->u.i : 0;
    if (offset + sizeof(double) > this_val->u.buffer.length) return djs_val_float(0.0);
    double val;
    memcpy(&val, this_val->u.buffer.data + offset, sizeof(double));
    return djs_val_float(val);
}

static DJSVal* djs_native_buf_write_double_le(DJSVal* this_val, int argc, DJSVal** argv) {
    if (!this_val || this_val->type != DJS_BUFFER || argc < 1) return djs_val_undefined();
    double val = (argv[0]->type == DJS_FLOAT) ? argv[0]->u.f : (double)argv[0]->u.i;
    size_t offset = (argc > 1 && argv[1]->type == DJS_INT) ? (size_t)argv[1]->u.i : 0;
    if (offset + sizeof(double) <= this_val->u.buffer.length) {
        memcpy(this_val->u.buffer.data + offset, &val, sizeof(double));
    }
    return djs_val_int((int64_t)(offset + sizeof(double)));
}

static DJSVal* djs_native_buf_to_string(DJSVal* this_val, int argc, DJSVal** argv) {
    (void)argc; (void)argv;
    if (!this_val || this_val->type != DJS_BUFFER) return djs_val_str("");
    char* s = (char*)malloc(this_val->u.buffer.length + 1);
    if (!s) return djs_val_str("");
    memcpy(s, this_val->u.buffer.data, this_val->u.buffer.length);
    s[this_val->u.buffer.length] = '\0';
    DJSVal* res = djs_val_str(s);
    free(s);
    return res;
}

// ---------------------------------------------------------------------------
// Native Promise Methods
// ---------------------------------------------------------------------------

static DJSVal* djs_native_promise_then(DJSVal* this_val, int argc, DJSVal** argv) {
    if (!this_val || this_val->type != DJS_PROMISE) return djs_val_undefined();
    DJSVal* on_fulfilled = argc > 0 ? argv[0] : NULL;
    DJSVal* on_rejected = argc > 1 ? argv[1] : NULL;
    return djs_promise_add_then(this_val, on_fulfilled, on_rejected);
}

static DJSVal* djs_native_promise_catch(DJSVal* this_val, int argc, DJSVal** argv) {
    if (!this_val || this_val->type != DJS_PROMISE) return djs_val_undefined();
    DJSVal* on_rejected = argc > 0 ? argv[0] : NULL;
    return djs_promise_add_then(this_val, NULL, on_rejected);
}

// ---------------------------------------------------------------------------
// Property Get / Set & Serialization
// ---------------------------------------------------------------------------

void djs_obj_set(DJSVal* obj, const char* key, DJSVal* val) {
    if (!obj || !key) return;

    if (obj->type == DJS_BUFFER) {
        char* end = NULL;
        long idx = strtol(key, &end, 10);
        if (end != key && idx >= 0 && (size_t)idx < obj->u.buffer.length) {
            uint8_t b = (uint8_t)(val->type == DJS_INT ? val->u.i :
                                  val->type == DJS_FLOAT ? (int64_t)val->u.f : 0);
            obj->u.buffer.data[idx] = b;
        }
        return;
    }

    if (obj->type != DJS_OBJECT) return;

    for (int i = 0; i < obj->u.o.count; i++) {
        if (strcmp(obj->u.o.props[i].key, key) == 0) {
            obj->u.o.props[i].val = val;
            return;
        }
    }
    if (obj->u.o.count >= obj->u.o.cap) {
        obj->u.o.cap *= 2;
        obj->u.o.props = (DJSProp*)realloc(obj->u.o.props, sizeof(DJSProp) * obj->u.o.cap);
    }
    obj->u.o.props[obj->u.o.count].key = djs_strdup(key);
    obj->u.o.props[obj->u.o.count].val = val;
    obj->u.o.count++;
}

DJSVal* djs_obj_get(DJSVal* obj, const char* key) {
    if (!obj || !key) return djs_val_undefined();

    if (obj->type == DJS_OBJECT) {
        for (int i = 0; i < obj->u.o.count; i++) {
            if (strcmp(obj->u.o.props[i].key, key) == 0) {
                return obj->u.o.props[i].val;
            }
        }
    } else if (obj->type == DJS_ARRAY) {
        if (strcmp(key, "length") == 0) {
            return djs_val_int(obj->u.a.count);
        }
        char* end = NULL;
        long idx = strtol(key, &end, 10);
        if (end != key && idx >= 0 && idx < obj->u.a.count) {
            return obj->u.a.items[idx];
        }
    } else if (obj->type == DJS_STRING) {
        if (strcmp(key, "length") == 0) {
            return djs_val_int((int64_t)strlen(obj->u.s ? obj->u.s : ""));
        }
    } else if (obj->type == DJS_BUFFER) {
        if (strcmp(key, "length") == 0) return djs_val_int((int64_t)obj->u.buffer.length);
        if (strcmp(key, "readUInt8") == 0) return djs_val_native_fn(djs_native_buf_read_uint8);
        if (strcmp(key, "writeUInt8") == 0) return djs_val_native_fn(djs_native_buf_write_uint8);
        if (strcmp(key, "readInt32LE") == 0) return djs_val_native_fn(djs_native_buf_read_int32_le);
        if (strcmp(key, "writeInt32LE") == 0) return djs_val_native_fn(djs_native_buf_write_int32_le);
        if (strcmp(key, "readDoubleLE") == 0) return djs_val_native_fn(djs_native_buf_read_double_le);
        if (strcmp(key, "writeDoubleLE") == 0) return djs_val_native_fn(djs_native_buf_write_double_le);
        if (strcmp(key, "toString") == 0) return djs_val_native_fn(djs_native_buf_to_string);

        char* end = NULL;
        long idx = strtol(key, &end, 10);
        if (end != key && idx >= 0 && (size_t)idx < obj->u.buffer.length) {
            return djs_val_int((int64_t)obj->u.buffer.data[idx]);
        }
    } else if (obj->type == DJS_PROMISE) {
        if (strcmp(key, "then") == 0) return djs_val_native_fn(djs_native_promise_then);
        if (strcmp(key, "catch") == 0) return djs_val_native_fn(djs_native_promise_catch);
    }
    return djs_val_undefined();
}

char* djs_to_string(DJSVal* v) {
    if (!v || v->type == DJS_UNDEFINED) return djs_strdup("undefined");
    if (v->type == DJS_NULL) return djs_strdup("null");
    if (v->type == DJS_BOOL) return djs_strdup(v->u.b ? "true" : "false");
    if (v->type == DJS_INT) {
        char buf[32];
        snprintf(buf, sizeof(buf), "%lld", (long long)v->u.i);
        return djs_strdup(buf);
    }
    if (v->type == DJS_FLOAT) {
        char buf[32];
        snprintf(buf, sizeof(buf), "%g", v->u.f);
        return djs_strdup(buf);
    }
    if (v->type == DJS_STRING) {
        return djs_strdup(v->u.s ? v->u.s : "");
    }
    if (v->type == DJS_BUFFER) {
        char* buf = (char*)malloc(v->u.buffer.length + 1);
        if (!buf) return djs_strdup("");
        memcpy(buf, v->u.buffer.data, v->u.buffer.length);
        buf[v->u.buffer.length] = '\0';
        return buf;
    }
    if (v->type == DJS_ARRAY) {
        size_t cap = 256;
        char* buf = (char*)malloc(cap);
        if (!buf) return djs_strdup("[]");
        buf[0] = '[';
        buf[1] = '\0';
        size_t len = 1;
        for (int i = 0; i < v->u.a.count; i++) {
            char* sub = djs_to_string(v->u.a.items[i]);
            size_t sub_len = strlen(sub);
            if (len + sub_len + 4 > cap) {
                cap = (len + sub_len + 4) * 2;
                buf = (char*)realloc(buf, cap);
            }
            if (i > 0) {
                strcat(buf, ",");
                len++;
            }
            strcat(buf, sub);
            len += sub_len;
            free(sub);
        }
        strcat(buf, "]");
        return buf;
    }
    if (v->type == DJS_OBJECT) {
        size_t cap = 256;
        char* buf = (char*)malloc(cap);
        if (!buf) return djs_strdup("{}");
        buf[0] = '{';
        buf[1] = '\0';
        size_t len = 1;
        for (int i = 0; i < v->u.o.count; i++) {
            char* sub = djs_to_string(v->u.o.props[i].val);
            size_t needed = strlen(v->u.o.props[i].key) + strlen(sub) + 8;
            if (len + needed > cap) {
                cap = (len + needed) * 2;
                buf = (char*)realloc(buf, cap);
            }
            if (i > 0) {
                strcat(buf, ",");
                len++;
            }
            snprintf(buf + len, cap - len, "\"%s\":%s", v->u.o.props[i].key, sub);
            len = strlen(buf);
            free(sub);
        }
        strcat(buf, "}");
        return buf;
    }
    if (v->type == DJS_FUNC || v->type == DJS_NATIVE_FUNC || v->type == DJS_NAPI_FUNC) {
        return djs_strdup("[Function]");
    }
    if (v->type == DJS_PROMISE) {
        return djs_strdup("[Promise]");
    }
    return djs_strdup("");
}

// ---------------------------------------------------------------------------
// Call Evaluator Helper
// ---------------------------------------------------------------------------

static DJSVal* djs_call_val(DJSVal* fn, DJSVal* this_val, int argc, DJSVal** argv) {
    if (!fn) return djs_val_undefined();

    if (fn->type == DJS_NATIVE_FUNC && fn->u.native_fn) {
        return fn->u.native_fn(this_val, argc, argv);
    }

    if (fn->type == DJS_NAPI_FUNC && fn->u.napi_fn.cb) {
        struct napi_callback_info__ info;
        info.argc = (size_t)argc;
        info.argv = (napi_value*)argv;
        info.this_arg = (napi_value)this_val;
        info.data = fn->u.napi_fn.data;
        napi_callback cb = (napi_callback)fn->u.napi_fn.cb;
        napi_env env = (napi_env)fn->u.napi_fn.env;
        napi_value r = cb(env, &info);
        return r ? (DJSVal*)r : djs_val_undefined();
    }

    if (fn->type == DJS_FUNC && fn->u.fn.body) {
        DJSScope* fn_scope = djs_scope_new(fn->u.fn.closure ? fn->u.fn.closure : g_djs_global_scope);
        for (int i = 0; i < fn->u.fn.param_count && i < argc; i++) {
            djs_scope_define(fn_scope, fn->u.fn.params[i], argv[i]);
        }
        return djs_eval_internal(fn_scope, fn->u.fn.body);
    }

    return djs_val_undefined();
}

// ---------------------------------------------------------------------------
// Standard SHA-256 Implementation (RFC 6234)
// ---------------------------------------------------------------------------

typedef struct {
    uint32_t state[8];
    uint64_t count;
    uint8_t buffer[64];
} DJS_SHA256_CTX;

#define DJS_ROTR(x, n) (((x) >> (n)) | ((x) << (32 - (n))))
#define DJS_CH(x, y, z) (((x) & (y)) ^ (~(x) & (z)))
#define DJS_MAJ(x, y, z) (((x) & (y)) ^ ((x) & (z)) ^ ((y) & (z)))
#define DJS_EP0(x) (DJS_ROTR(x, 2) ^ DJS_ROTR(x, 13) ^ DJS_ROTR(x, 22))
#define DJS_EP1(x) (DJS_ROTR(x, 6) ^ DJS_ROTR(x, 11) ^ DJS_ROTR(x, 25))
#define DJS_SIG0(x) (DJS_ROTR(x, 7) ^ DJS_ROTR(x, 18) ^ ((x) >> 3))
#define DJS_SIG1(x) (DJS_ROTR(x, 17) ^ DJS_ROTR(x, 19) ^ ((x) >> 10))

static const uint32_t K256[64] = {
    0x428a2f98,0x71374491,0xb5c0fbcf,0xe9b5dba5,0x3956c25b,0x59f111f1,0x923f82a4,0xab1c5ed5,
    0xd807aa98,0x12835b01,0x243185be,0x550c7dc3,0x72be5d74,0x80deb1fe,0x9bdc06a7,0xc19bf174,
    0xe49b69c1,0xefbe4786,0x0fc19dc6,0x240ca1cc,0x2de92c6f,0x4a7484aa,0x5cb0a9dc,0x76f988da,
    0x983e5152,0xa831c66d,0xb00327c8,0xbf597fc7,0xc6e00bf3,0xd5a79147,0x06ca6351,0x14292967,
    0x27b70a85,0x2e1b2138,0x4d2c6dfc,0x53380d13,0x650a7354,0x766a0abb,0x81c2c92e,0x92722c85,
    0xa2bfe8a1,0xa81a664b,0xc24b8b70,0xc76c51a3,0xd192e819,0xd6990624,0xf40e3585,0x106aa070,
    0x19a4c116,0x1e376c08,0x2748774c,0x34b0bcb5,0x391c0cb3,0x4ed8aa4a,0x5b9cca4f,0x682e6ff3,
    0x748f82ee,0x78a5636f,0x84c87814,0x8cc70208,0x90befffa,0xa4506ceb,0xbef9a3f7,0xc67178f2
};

static void djs_sha256_transform(DJS_SHA256_CTX* ctx, const uint8_t data[]) {
    uint32_t a, b, c, d, e, f, g, h, m[64];
    for (int i = 0, j = 0; i < 16; ++i, j += 4)
        m[i] = ((uint32_t)data[j] << 24) | ((uint32_t)data[j + 1] << 16) | ((uint32_t)data[j + 2] << 8) | ((uint32_t)data[j + 3]);
    for (int i = 16; i < 64; ++i)
        m[i] = DJS_SIG1(m[i - 2]) + m[i - 7] + DJS_SIG0(m[i - 15]) + m[i - 16];

    a = ctx->state[0]; b = ctx->state[1]; c = ctx->state[2]; d = ctx->state[3];
    e = ctx->state[4]; f = ctx->state[5]; g = ctx->state[6]; h = ctx->state[7];

    for (int i = 0; i < 64; ++i) {
        uint32_t t1 = h + DJS_EP1(e) + DJS_CH(e, f, g) + K256[i] + m[i];
        uint32_t t2 = DJS_EP0(a) + DJS_MAJ(a, b, c);
        h = g; g = f; f = e; e = d + t1;
        d = c; c = b; b = a; a = t1 + t2;
    }
    ctx->state[0] += a; ctx->state[1] += b; ctx->state[2] += c; ctx->state[3] += d;
    ctx->state[4] += e; ctx->state[5] += f; ctx->state[6] += g; ctx->state[7] += h;
}

static void djs_sha256_init(DJS_SHA256_CTX* ctx) {
    ctx->count = 0;
    ctx->state[0] = 0x6a09e667; ctx->state[1] = 0xbb67ae85;
    ctx->state[2] = 0x3c6ef372; ctx->state[3] = 0xa54ff53a;
    ctx->state[4] = 0x510e527f; ctx->state[5] = 0x9b05688c;
    ctx->state[6] = 0x1f83d9ab; ctx->state[7] = 0x5be0cd19;
}

static void djs_sha256_update(DJS_SHA256_CTX* ctx, const uint8_t* data, size_t len) {
    size_t i = 0;
    size_t idx = (size_t)((ctx->count >> 3) & 0x3f);
    ctx->count += ((uint64_t)len << 3);
    size_t part_len = 64 - idx;

    if (len >= part_len) {
        memcpy(&ctx->buffer[idx], data, part_len);
        djs_sha256_transform(ctx, ctx->buffer);
        for (i = part_len; i + 63 < len; i += 64)
            djs_sha256_transform(ctx, &data[i]);
        idx = 0;
    }
    memcpy(&ctx->buffer[idx], &data[i], len - i);
}

static void djs_sha256_final(DJS_SHA256_CTX* ctx, uint8_t hash[32]) {
    uint8_t final_count[8];
    for (int i = 0; i < 8; i++)
        final_count[i] = (uint8_t)((ctx->count >> ((7 - i) * 8)) & 0xff);

    uint8_t pad = 0x80;
    djs_sha256_update(ctx, &pad, 1);
    uint8_t zero = 0;
    while ((ctx->count & 0x1f8) != 0x1c0) {
        djs_sha256_update(ctx, &zero, 1);
    }
    djs_sha256_update(ctx, final_count, 8);

    for (int i = 0; i < 8; i++) {
        hash[i * 4]     = (uint8_t)((ctx->state[i] >> 24) & 0xff);
        hash[i * 4 + 1] = (uint8_t)((ctx->state[i] >> 16) & 0xff);
        hash[i * 4 + 2] = (uint8_t)((ctx->state[i] >> 8)  & 0xff);
        hash[i * 4 + 3] = (uint8_t)(ctx->state[i] & 0xff);
    }
}

// ---------------------------------------------------------------------------
// Native Built-ins: Console, Math, JSON, Buffer, EventEmitter, Modules
// ---------------------------------------------------------------------------

static DJSVal* djs_native_console_log(DJSVal* this_val, int argc, DJSVal** argv) {
    (void)this_val;
    for (int i = 0; i < argc; i++) {
        char* str = djs_to_string(argv[i]);
        if (i > 0) printf(" ");
        printf("%s", str);
        free(str);
    }
    printf("\n");
    fflush(stdout);
    return djs_val_undefined();
}

static DJSVal* djs_native_math_floor(DJSVal* this_val, int argc, DJSVal** argv) {
    (void)this_val;
    if (argc < 1) return djs_val_int(0);
    double d = (argv[0]->type == DJS_FLOAT) ? argv[0]->u.f : (double)argv[0]->u.i;
    return djs_val_int((int64_t)floor(d));
}

static DJSVal* djs_native_math_ceil(DJSVal* this_val, int argc, DJSVal** argv) {
    (void)this_val;
    if (argc < 1) return djs_val_int(0);
    double d = (argv[0]->type == DJS_FLOAT) ? argv[0]->u.f : (double)argv[0]->u.i;
    return djs_val_int((int64_t)ceil(d));
}

static DJSVal* djs_native_math_round(DJSVal* this_val, int argc, DJSVal** argv) {
    (void)this_val;
    if (argc < 1) return djs_val_int(0);
    double d = (argv[0]->type == DJS_FLOAT) ? argv[0]->u.f : (double)argv[0]->u.i;
    return djs_val_int((int64_t)round(d));
}

static DJSVal* djs_native_math_abs(DJSVal* this_val, int argc, DJSVal** argv) {
    (void)this_val;
    if (argc < 1) return djs_val_int(0);
    if (argv[0]->type == DJS_FLOAT) return djs_val_float(fabs(argv[0]->u.f));
    return djs_val_int(argv[0]->u.i < 0 ? -argv[0]->u.i : argv[0]->u.i);
}

static DJSVal* djs_native_math_sqrt(DJSVal* this_val, int argc, DJSVal** argv) {
    (void)this_val;
    if (argc < 1) return djs_val_float(0.0);
    double d = (argv[0]->type == DJS_FLOAT) ? argv[0]->u.f : (double)argv[0]->u.i;
    return djs_val_float(sqrt(d));
}

static DJSVal* djs_native_math_min(DJSVal* this_val, int argc, DJSVal** argv) {
    (void)this_val;
    if (argc == 0) return djs_val_float(1e300);
    bool has_float = false;
    double min_f = 0.0;
    int64_t min_i = 0;
    for (int i = 0; i < argc; i++) {
        if (argv[i]->type == DJS_FLOAT) {
            if (!has_float && i > 0) min_f = (double)min_i;
            has_float = true;
            if (i == 0 || argv[i]->u.f < min_f) min_f = argv[i]->u.f;
        } else {
            int64_t v = argv[i]->type == DJS_INT ? argv[i]->u.i : 0;
            if (has_float) {
                if ((double)v < min_f) min_f = (double)v;
            } else {
                if (i == 0 || v < min_i) min_i = v;
            }
        }
    }
    return has_float ? djs_val_float(min_f) : djs_val_int(min_i);
}

static DJSVal* djs_native_math_max(DJSVal* this_val, int argc, DJSVal** argv) {
    (void)this_val;
    if (argc == 0) return djs_val_float(-1e300);
    bool has_float = false;
    double max_f = 0.0;
    int64_t max_i = 0;
    for (int i = 0; i < argc; i++) {
        if (argv[i]->type == DJS_FLOAT) {
            if (!has_float && i > 0) max_f = (double)max_i;
            has_float = true;
            if (i == 0 || argv[i]->u.f > max_f) max_f = argv[i]->u.f;
        } else {
            int64_t v = argv[i]->type == DJS_INT ? argv[i]->u.i : 0;
            if (has_float) {
                if ((double)v > max_f) max_f = (double)v;
            } else {
                if (i == 0 || v > max_i) max_i = v;
            }
        }
    }
    return has_float ? djs_val_float(max_f) : djs_val_int(max_i);
}

static DJSVal* djs_native_math_pow(DJSVal* this_val, int argc, DJSVal** argv) {
    (void)this_val;
    if (argc < 2) return djs_val_float(0.0);
    double b = (argv[0]->type == DJS_FLOAT) ? argv[0]->u.f : (double)argv[0]->u.i;
    double e = (argv[1]->type == DJS_FLOAT) ? argv[1]->u.f : (double)argv[1]->u.i;
    return djs_val_float(pow(b, e));
}

static DJSVal* djs_native_math_random(DJSVal* this_val, int argc, DJSVal** argv) {
    (void)this_val; (void)argc; (void)argv;
    return djs_val_float((double)rand() / (double)RAND_MAX);
}

static DJSVal* djs_native_json_stringify(DJSVal* this_val, int argc, DJSVal** argv) {
    (void)this_val;
    if (argc < 1) return djs_val_str("undefined");
    char* s = djs_to_string(argv[0]);
    DJSVal* res = djs_val_str(s);
    free(s);
    return res;
}

static DJSVal* djs_native_json_parse(DJSVal* this_val, int argc, DJSVal** argv) {
    (void)this_val;
    if (argc < 1 || argv[0]->type != DJS_STRING) return djs_val_null();
    return djs_eval_internal(NULL, argv[0]->u.s);
}

// Buffer.alloc(size)
static DJSVal* djs_native_buffer_alloc(DJSVal* this_val, int argc, DJSVal** argv) {
    (void)this_val;
    size_t sz = (argc > 0 && argv[0]->type == DJS_INT) ? (size_t)argv[0]->u.i : 0;
    return djs_val_buffer_alloc(sz);
}

// Buffer.from(val)
static DJSVal* djs_native_buffer_from(DJSVal* this_val, int argc, DJSVal** argv) {
    (void)this_val;
    if (argc < 1) return djs_val_buffer_alloc(0);
    if (argv[0]->type == DJS_STRING) {
        const char* s = argv[0]->u.s ? argv[0]->u.s : "";
        size_t len = strlen(s);
        DJSVal* b = djs_val_buffer_alloc(len);
        if (b && len > 0) memcpy(b->u.buffer.data, s, len);
        return b;
    }
    if (argv[0]->type == DJS_ARRAY) {
        size_t len = argv[0]->u.a.count;
        DJSVal* b = djs_val_buffer_alloc(len);
        if (b) {
            for (size_t i = 0; i < len; i++) {
                DJSVal* it = argv[0]->u.a.items[i];
                b->u.buffer.data[i] = (uint8_t)(it->type == DJS_INT ? it->u.i : 0);
            }
        }
        return b;
    }
    if (argv[0]->type == DJS_BUFFER) {
        size_t len = argv[0]->u.buffer.length;
        DJSVal* b = djs_val_buffer_alloc(len);
        if (b && len > 0) memcpy(b->u.buffer.data, argv[0]->u.buffer.data, len);
        return b;
    }
    return djs_val_buffer_alloc(0);
}

// Buffer.isBuffer(val)
static DJSVal* djs_native_buffer_is_buffer(DJSVal* this_val, int argc, DJSVal** argv) {
    (void)this_val;
    if (argc < 1) return djs_val_bool(false);
    return djs_val_bool(argv[0]->type == DJS_BUFFER);
}

// Promise.resolve(val)
static DJSVal* djs_native_promise_resolve(DJSVal* this_val, int argc, DJSVal** argv) {
    (void)this_val;
    DJSVal* p = djs_val_promise();
    djs_promise_resolve(p, argc > 0 ? argv[0] : djs_val_undefined());
    return p;
}

// Promise.reject(val)
static DJSVal* djs_native_promise_reject(DJSVal* this_val, int argc, DJSVal** argv) {
    (void)this_val;
    DJSVal* p = djs_val_promise();
    djs_promise_reject(p, argc > 0 ? argv[0] : djs_val_undefined());
    return p;
}

// EventEmitter Implementation
static DJSVal* djs_native_ee_on(DJSVal* this_val, int argc, DJSVal** argv) {
    if (!this_val || argc < 2) return this_val ? this_val : djs_val_undefined();
    const char* event = argv[0]->type == DJS_STRING ? argv[0]->u.s : "";
    DJSVal* fn = argv[1];

    DJSVal* listeners = djs_obj_get(this_val, "_listeners");
    if (listeners->type != DJS_OBJECT) {
        listeners = djs_val_obj();
        djs_obj_set(this_val, "_listeners", listeners);
    }
    DJSVal* arr = djs_obj_get(listeners, event);
    if (arr->type != DJS_ARRAY) {
        arr = djs_val_arr();
        djs_obj_set(listeners, event, arr);
    }
    djs_arr_push(arr, fn);
    return this_val;
}

static DJSVal* djs_native_ee_emit(DJSVal* this_val, int argc, DJSVal** argv) {
    if (!this_val || argc < 1) return djs_val_bool(false);
    const char* event = argv[0]->type == DJS_STRING ? argv[0]->u.s : "";

    DJSVal* listeners = djs_obj_get(this_val, "_listeners");
    if (listeners->type != DJS_OBJECT) return djs_val_bool(false);

    DJSVal* arr = djs_obj_get(listeners, event);
    if (arr->type != DJS_ARRAY || arr->u.a.count == 0) return djs_val_bool(false);

    DJSVal* sub_args[16];
    int sub_argc = argc - 1;
    if (sub_argc > 16) sub_argc = 16;
    for (int i = 0; i < sub_argc; i++) sub_args[i] = argv[i + 1];

    int count = arr->u.a.count;
    for (int i = 0; i < count; i++) {
        djs_call_val(arr->u.a.items[i], this_val, sub_argc, sub_args);
    }
    return djs_val_bool(true);
}

static DJSVal* djs_native_ee_remove_listener(DJSVal* this_val, int argc, DJSVal** argv) {
    if (!this_val || argc < 2) return this_val ? this_val : djs_val_undefined();
    const char* event = argv[0]->type == DJS_STRING ? argv[0]->u.s : "";
    DJSVal* fn = argv[1];

    DJSVal* listeners = djs_obj_get(this_val, "_listeners");
    if (listeners->type == DJS_OBJECT) {
        DJSVal* arr = djs_obj_get(listeners, event);
        if (arr->type == DJS_ARRAY) {
            for (int i = 0; i < arr->u.a.count; i++) {
                if (arr->u.a.items[i] == fn) {
                    for (int j = i; j < arr->u.a.count - 1; j++) {
                        arr->u.a.items[j] = arr->u.a.items[j + 1];
                    }
                    arr->u.a.count--;
                    break;
                }
            }
        }
    }
    return this_val;
}

static DJSVal* djs_create_event_emitter_instance(void) {
    DJSVal* ee = djs_val_obj();
    djs_obj_set(ee, "on", djs_val_native_fn(djs_native_ee_on));
    djs_obj_set(ee, "addListener", djs_val_native_fn(djs_native_ee_on));
    djs_obj_set(ee, "emit", djs_val_native_fn(djs_native_ee_emit));
    djs_obj_set(ee, "removeListener", djs_val_native_fn(djs_native_ee_remove_listener));
    djs_obj_set(ee, "off", djs_val_native_fn(djs_native_ee_remove_listener));
    djs_obj_set(ee, "_listeners", djs_val_obj());
    return ee;
}

static DJSVal* djs_native_event_emitter_ctor(DJSVal* this_val, int argc, DJSVal** argv) {
    (void)argc; (void)argv;
    if (this_val && this_val->type == DJS_OBJECT) {
        djs_obj_set(this_val, "on", djs_val_native_fn(djs_native_ee_on));
        djs_obj_set(this_val, "addListener", djs_val_native_fn(djs_native_ee_on));
        djs_obj_set(this_val, "emit", djs_val_native_fn(djs_native_ee_emit));
        djs_obj_set(this_val, "removeListener", djs_val_native_fn(djs_native_ee_remove_listener));
        djs_obj_set(this_val, "off", djs_val_native_fn(djs_native_ee_remove_listener));
        djs_obj_set(this_val, "_listeners", djs_val_obj());
        return this_val;
    }
    return djs_create_event_emitter_instance();
}

// ---------------------------------------------------------------------------
// Node Core Module: path
// ---------------------------------------------------------------------------

static DJSVal* djs_native_path_join(DJSVal* this_val, int argc, DJSVal** argv) {
    (void)this_val;
    char buf[1024] = {0};
    size_t len = 0;
    for (int i = 0; i < argc; i++) {
        char* part = djs_to_string(argv[i]);
        if (part && part[0] != '\0') {
            if (len > 0 && buf[len - 1] != '/' && buf[len - 1] != '\\') {
#ifdef _WIN32
                buf[len++] = '\\';
#else
                buf[len++] = '/';
#endif
                buf[len] = '\0';
            }
            strncat(buf, part, sizeof(buf) - len - 1);
            len = strlen(buf);
        }
        free(part);
    }
    return djs_val_str(buf);
}

static DJSVal* djs_native_path_basename(DJSVal* this_val, int argc, DJSVal** argv) {
    (void)this_val;
    if (argc < 1) return djs_val_str("");
    char* p = djs_to_string(argv[0]);
    char* last_slash = strrchr(p, '/');
    char* last_bslash = strrchr(p, '\\');
    char* base = p;
    if (last_slash && last_slash >= base) base = last_slash + 1;
    if (last_bslash && last_bslash >= base) base = last_bslash + 1;

    // Optional ext removal: basename(p, ext)
    if (argc > 1 && argv[1]->type == DJS_STRING && argv[1]->u.s) {
        size_t blen = strlen(base);
        size_t elen = strlen(argv[1]->u.s);
        if (blen >= elen && strcmp(base + blen - elen, argv[1]->u.s) == 0) {
            char* trimmed = djs_strdup(base);
            trimmed[blen - elen] = '\0';
            DJSVal* r = djs_val_str(trimmed);
            free(trimmed);
            free(p);
            return r;
        }
    }

    DJSVal* r = djs_val_str(base);
    free(p);
    return r;
}

static DJSVal* djs_native_path_dirname(DJSVal* this_val, int argc, DJSVal** argv) {
    (void)this_val;
    if (argc < 1) return djs_val_str(".");
    char* p = djs_to_string(argv[0]);
    char* last_slash = strrchr(p, '/');
    char* last_bslash = strrchr(p, '\\');
    char* sep = last_slash > last_bslash ? last_slash : last_bslash;
    if (sep) {
        *sep = '\0';
        DJSVal* r = djs_val_str(p[0] == '\0' ? "/" : p);
        free(p);
        return r;
    }
    free(p);
    return djs_val_str(".");
}

static DJSVal* djs_native_path_extname(DJSVal* this_val, int argc, DJSVal** argv) {
    (void)this_val;
    if (argc < 1) return djs_val_str("");
    char* p = djs_to_string(argv[0]);
    char* dot = strrchr(p, '.');
    DJSVal* r = dot ? djs_val_str(dot) : djs_val_str("");
    free(p);
    return r;
}

// ---------------------------------------------------------------------------
// Node Core Module: fs
// ---------------------------------------------------------------------------

static DJSVal* djs_native_fs_read_file_sync(DJSVal* this_val, int argc, DJSVal** argv) {
    (void)this_val;
    if (argc < 1) return djs_val_undefined();
    char* path = djs_to_string(argv[0]);
    const char* content = datara_rt_file_read(path);
    free(path);
    if (!content) return djs_val_str("");
    return djs_val_str(content);
}

static DJSVal* djs_native_fs_write_file_sync(DJSVal* this_val, int argc, DJSVal** argv) {
    (void)this_val;
    if (argc < 2) return djs_val_undefined();
    char* path = djs_to_string(argv[0]);
    char* content = djs_to_string(argv[1]);
    datara_rt_file_write(path, content);
    free(path);
    free(content);
    return djs_val_undefined();
}

static DJSVal* djs_native_fs_exists_sync(DJSVal* this_val, int argc, DJSVal** argv) {
    (void)this_val;
    if (argc < 1) return djs_val_bool(false);
    char* path = djs_to_string(argv[0]);
    int64_t ex = datara_rt_file_exists(path);
    free(path);
    return djs_val_bool(ex != 0);
}

static DJSVal* djs_native_fs_unlink_sync(DJSVal* this_val, int argc, DJSVal** argv) {
    (void)this_val;
    if (argc < 1) return djs_val_undefined();
    char* path = djs_to_string(argv[0]);
    remove(path);
    free(path);
    return djs_val_undefined();
}

static DJSVal* djs_native_fs_mkdir_sync(DJSVal* this_val, int argc, DJSVal** argv) {
    (void)this_val;
    if (argc < 1) return djs_val_undefined();
    char* path = djs_to_string(argv[0]);
    djs_mkdir(path);
    free(path);
    return djs_val_undefined();
}

// ---------------------------------------------------------------------------
// Node Core Module: crypto
// ---------------------------------------------------------------------------

typedef struct {
    DJS_SHA256_CTX ctx;
} DJSHashWrapper;

static DJSVal* djs_native_hash_update(DJSVal* this_val, int argc, DJSVal** argv) {
    if (!this_val || argc < 1) return this_val ? this_val : djs_val_undefined();
    DJSVal* raw_ptr = djs_obj_get(this_val, "_ptr");
    if (raw_ptr->type != DJS_INT) return this_val;
    DJSHashWrapper* wrap = (DJSHashWrapper*)(uintptr_t)raw_ptr->u.i;

    if (argv[0]->type == DJS_BUFFER) {
        djs_sha256_update(&wrap->ctx, argv[0]->u.buffer.data, argv[0]->u.buffer.length);
    } else {
        char* str = djs_to_string(argv[0]);
        djs_sha256_update(&wrap->ctx, (const uint8_t*)str, strlen(str));
        free(str);
    }
    return this_val;
}

static DJSVal* djs_native_hash_digest(DJSVal* this_val, int argc, DJSVal** argv) {
    (void)argc; (void)argv;
    if (!this_val) return djs_val_str("");
    DJSVal* raw_ptr = djs_obj_get(this_val, "_ptr");
    if (raw_ptr->type != DJS_INT) return djs_val_str("");
    DJSHashWrapper* wrap = (DJSHashWrapper*)(uintptr_t)raw_ptr->u.i;

    uint8_t hash[32];
    djs_sha256_final(&wrap->ctx, hash);

    char hex[65];
    for (int i = 0; i < 32; i++) {
        snprintf(&hex[i * 2], 3, "%02x", hash[i]);
    }
    hex[64] = '\0';
    free(wrap);
    djs_obj_set(this_val, "_ptr", djs_val_int(0));
    return djs_val_str(hex);
}

static DJSVal* djs_native_crypto_create_hash(DJSVal* this_val, int argc, DJSVal** argv) {
    (void)this_val;
    const char* algo = (argc > 0 && argv[0]->type == DJS_STRING) ? argv[0]->u.s : "sha256";
    (void)algo; // Currently sha256

    DJSHashWrapper* wrap = (DJSHashWrapper*)malloc(sizeof(DJSHashWrapper));
    djs_sha256_init(&wrap->ctx);

    DJSVal* hash_obj = djs_val_obj();
    djs_obj_set(hash_obj, "_ptr", djs_val_int((int64_t)(uintptr_t)wrap));
    djs_obj_set(hash_obj, "update", djs_val_native_fn(djs_native_hash_update));
    djs_obj_set(hash_obj, "digest", djs_val_native_fn(djs_native_hash_digest));
    return hash_obj;
}

// ---------------------------------------------------------------------------
// Node Core Module: http (Sockets & Round-Trip Server)
// ---------------------------------------------------------------------------

#ifdef _WIN32
static int g_wsa_initialized = 0;
static void djs_ensure_winsock(void) {
    if (!g_wsa_initialized) {
        WSADATA wsa;
        WSAStartup(MAKEWORD(2, 2), &wsa);
        g_wsa_initialized = 1;
    }
}
#else
static void djs_ensure_winsock(void) {}
#endif

typedef struct {
    SOCKET srv_sock;
    int port;
    DJSVal* req_handler;
} DJSHttpServer;

static DJSVal* djs_native_http_server_listen(DJSVal* this_val, int argc, DJSVal** argv) {
    if (!this_val || argc < 1) return this_val ? this_val : djs_val_undefined();
    DJSVal* ptr_val = djs_obj_get(this_val, "_server_ptr");
    if (ptr_val->type != DJS_INT) return this_val;
    DJSHttpServer* srv = (DJSHttpServer*)(uintptr_t)ptr_val->u.i;

    int port = (int)(argv[0]->type == DJS_INT ? argv[0]->u.i : 0);
    DJSVal* cb = NULL;
    if (argc > 1 && (argv[1]->type == DJS_FUNC || argv[1]->type == DJS_NATIVE_FUNC)) {
        cb = argv[1];
    } else if (argc > 2 && (argv[2]->type == DJS_FUNC || argv[2]->type == DJS_NATIVE_FUNC)) {
        cb = argv[2];
    }

    djs_ensure_winsock();
    srv->srv_sock = socket(AF_INET, SOCK_STREAM, IPPROTO_TCP);
    if (srv->srv_sock != INVALID_SOCKET) {
        int opt = 1;
        setsockopt(srv->srv_sock, SOL_SOCKET, SO_REUSEADDR, (const char*)&opt, sizeof(opt));

        struct sockaddr_in addr;
        memset(&addr, 0, sizeof(addr));
        addr.sin_family = AF_INET;
        addr.sin_addr.s_addr = inet_addr("127.0.0.1");
        addr.sin_port = htons((u_short)port);

        if (bind(srv->srv_sock, (struct sockaddr*)&addr, sizeof(addr)) == 0) {
            listen(srv->srv_sock, 5);
            struct sockaddr_in bound;
            int blen = sizeof(bound);
            if (getsockname(srv->srv_sock, (struct sockaddr*)&bound, &blen) == 0) {
                srv->port = ntohs(bound.sin_port);
            } else {
                srv->port = port;
            }
            djs_obj_set(this_val, "port", djs_val_int(srv->port));
        }
    }

    if (cb) {
        djs_call_val(cb, this_val, 0, NULL);
    }
    return this_val;
}

static DJSVal* djs_native_http_server_close(DJSVal* this_val, int argc, DJSVal** argv) {
    if (!this_val) return djs_val_undefined();
    DJSVal* ptr_val = djs_obj_get(this_val, "_server_ptr");
    if (ptr_val->type == DJS_INT) {
        DJSHttpServer* srv = (DJSHttpServer*)(uintptr_t)ptr_val->u.i;
        if (srv->srv_sock != INVALID_SOCKET) {
            closesocket(srv->srv_sock);
            srv->srv_sock = INVALID_SOCKET;
        }
    }
    if (argc > 0 && (argv[0]->type == DJS_FUNC || argv[0]->type == DJS_NATIVE_FUNC)) {
        djs_call_val(argv[0], this_val, 0, NULL);
    }
    return this_val;
}

// Response object helper for HTTP server
typedef struct {
    SOCKET client_sock;
    int status_code;
    bool headers_sent;
} DJSHttpResponse;

static DJSVal* djs_native_res_write_head(DJSVal* this_val, int argc, DJSVal** argv) {
    if (!this_val || argc < 1) return this_val ? this_val : djs_val_undefined();
    DJSVal* ptr_val = djs_obj_get(this_val, "_res_ptr");
    if (ptr_val->type == DJS_INT) {
        DJSHttpResponse* r = (DJSHttpResponse*)(uintptr_t)ptr_val->u.i;
        r->status_code = (int)(argv[0]->type == DJS_INT ? argv[0]->u.i : 200);
    }
    return this_val;
}

static DJSVal* djs_native_res_end(DJSVal* this_val, int argc, DJSVal** argv) {
    if (!this_val) return djs_val_undefined();
    DJSVal* ptr_val = djs_obj_get(this_val, "_res_ptr");
    if (ptr_val->type != DJS_INT) return djs_val_undefined();
    DJSHttpResponse* r = (DJSHttpResponse*)(uintptr_t)ptr_val->u.i;

    char* body = (argc > 0) ? djs_to_string(argv[0]) : djs_strdup("");
    size_t body_len = strlen(body);

    char header[512];
    snprintf(header, sizeof(header),
             "HTTP/1.1 %d OK\r\n"
             "Content-Length: %zu\r\n"
             "Content-Type: text/plain\r\n"
             "Connection: close\r\n\r\n",
             r->status_code > 0 ? r->status_code : 200,
             body_len);

    send(r->client_sock, header, (int)strlen(header), 0);
    if (body_len > 0) {
        send(r->client_sock, body, (int)body_len, 0);
    }
    free(body);
    closesocket(r->client_sock);
    r->client_sock = INVALID_SOCKET;
    return this_val;
}

static DJSVal* djs_native_http_create_server(DJSVal* this_val, int argc, DJSVal** argv) {
    (void)this_val;
    DJSHttpServer* srv = (DJSHttpServer*)malloc(sizeof(DJSHttpServer));
    srv->srv_sock = INVALID_SOCKET;
    srv->port = 0;
    srv->req_handler = (argc > 0) ? argv[0] : NULL;

    DJSVal* server_obj = djs_val_obj();
    djs_obj_set(server_obj, "_server_ptr", djs_val_int((int64_t)(uintptr_t)srv));
    djs_obj_set(server_obj, "listen", djs_val_native_fn(djs_native_http_server_listen));
    djs_obj_set(server_obj, "close", djs_val_native_fn(djs_native_http_server_close));
    return server_obj;
}

// http.get(url, cb)
static DJSVal* djs_native_http_get(DJSVal* this_val, int argc, DJSVal** argv) {
    (void)this_val;
    if (argc < 2) return djs_val_undefined();
    char* url = djs_to_string(argv[0]);
    DJSVal* cb = argv[1];

    // Parse URL: http://127.0.0.1:port/path
    int port = 80;
    char path[256] = "/";
    char* colon = strstr(url, "127.0.0.1:");
    if (colon) {
        port = atoi(colon + 10);
        char* slash = strchr(colon + 10, '/');
        if (slash) snprintf(path, sizeof(path), "%s", slash);
    }
    free(url);

    djs_ensure_winsock();
    SOCKET cli = socket(AF_INET, SOCK_STREAM, IPPROTO_TCP);
    if (cli == INVALID_SOCKET) return djs_val_undefined();

    struct sockaddr_in srv_addr;
    memset(&srv_addr, 0, sizeof(srv_addr));
    srv_addr.sin_family = AF_INET;
    srv_addr.sin_addr.s_addr = inet_addr("127.0.0.1");
    srv_addr.sin_port = htons((u_short)port);

    if (connect(cli, (struct sockaddr*)&srv_addr, sizeof(srv_addr)) != 0) {
        closesocket(cli);
        return djs_val_undefined();
    }

    // Check if server needs to accept and handle
    // Find server by port or accept on active server socket
    // In our synchronous loopback:
    // Send HTTP GET
    char req[512];
    snprintf(req, sizeof(req), "GET %s HTTP/1.1\r\nHost: 127.0.0.1:%d\r\nConnection: close\r\n\r\n", path, port);
    send(cli, req, (int)strlen(req), 0);

    // Now find any listening server on that port to accept and serve
    // We can scan global scope for the server object or accept
    // Let's accept on the server if listening:
    DJSVal* srv_val = djs_scope_get(g_djs_global_scope, "server");
    if (srv_val->type == DJS_OBJECT) {
        DJSVal* s_ptr = djs_obj_get(srv_val, "_server_ptr");
        if (s_ptr->type == DJS_INT) {
            DJSHttpServer* srv = (DJSHttpServer*)(uintptr_t)s_ptr->u.i;
            if (srv->srv_sock != INVALID_SOCKET) {
                struct sockaddr_in caddr;
                int clen = sizeof(caddr);
                SOCKET conn = accept(srv->srv_sock, (struct sockaddr*)&caddr, &clen);
                if (conn != INVALID_SOCKET) {
                    char recv_buf[1024] = {0};
                    recv(conn, recv_buf, sizeof(recv_buf) - 1, 0);

                    // Build request object
                    DJSVal* req_obj = djs_val_obj();
                    djs_obj_set(req_obj, "method", djs_val_str("GET"));
                    djs_obj_set(req_obj, "url", djs_val_str(path));

                    // Build response object
                    DJSHttpResponse resp;
                    resp.client_sock = conn;
                    resp.status_code = 200;
                    resp.headers_sent = false;

                    DJSVal* res_obj = djs_val_obj();
                    djs_obj_set(res_obj, "_res_ptr", djs_val_int((int64_t)(uintptr_t)&resp));
                    djs_obj_set(res_obj, "writeHead", djs_val_native_fn(djs_native_res_write_head));
                    djs_obj_set(res_obj, "end", djs_val_native_fn(djs_native_res_end));

                    if (srv->req_handler) {
                        DJSVal* handler_args[2] = { req_obj, res_obj };
                        djs_call_val(srv->req_handler, NULL, 2, handler_args);
                    }
                }
            }
        }
    }

    // Now client reads response
    char resp_buf[2048] = {0};
    int total_bytes = 0;
    int n;
    while ((n = recv(cli, resp_buf + total_bytes, sizeof(resp_buf) - total_bytes - 1, 0)) > 0) {
        total_bytes += n;
    }
    closesocket(cli);

    int status = 200;
    char* body_start = strstr(resp_buf, "\r\n\r\n");
    char* body = body_start ? (body_start + 4) : resp_buf;

    // Parse status line: HTTP/1.1 200 OK
    char* sp = strchr(resp_buf, ' ');
    if (sp) status = atoi(sp + 1);

    // Construct client response EventEmitter
    DJSVal* client_res = djs_create_event_emitter_instance();
    djs_obj_set(client_res, "statusCode", djs_val_int(status));

    // Invoke user callback
    DJSVal* cb_args[1] = { client_res };
    djs_call_val(cb, NULL, 1, cb_args);

    // Emit 'data' and 'end'
    DJSVal* data_args[2] = { djs_val_str("data"), djs_val_str(body) };
    djs_native_ee_emit(client_res, 2, data_args);

    DJSVal* end_args[1] = { djs_val_str("end") };
    djs_native_ee_emit(client_res, 1, end_args);

    return client_res;
}

// ---------------------------------------------------------------------------
// Engine Initialization & Global Scope Setup
// ---------------------------------------------------------------------------

void djs_init_globals(void) {
    if (g_djs_initialized) return;
    g_djs_global_scope = djs_scope_new(NULL);

    // console
    DJSVal* console_obj = djs_val_obj();
    djs_obj_set(console_obj, "log", djs_val_native_fn(djs_native_console_log));
    djs_obj_set(console_obj, "error", djs_val_native_fn(djs_native_console_log));
    djs_obj_set(console_obj, "warn", djs_val_native_fn(djs_native_console_log));
    djs_scope_set(g_djs_global_scope, "console", console_obj);

    // Math
    DJSVal* math_obj = djs_val_obj();
    djs_obj_set(math_obj, "PI", djs_val_float(3.141592653589793));
    djs_obj_set(math_obj, "E", djs_val_float(2.718281828459045));
    djs_obj_set(math_obj, "floor", djs_val_native_fn(djs_native_math_floor));
    djs_obj_set(math_obj, "ceil", djs_val_native_fn(djs_native_math_ceil));
    djs_obj_set(math_obj, "round", djs_val_native_fn(djs_native_math_round));
    djs_obj_set(math_obj, "abs", djs_val_native_fn(djs_native_math_abs));
    djs_obj_set(math_obj, "sqrt", djs_val_native_fn(djs_native_math_sqrt));
    djs_obj_set(math_obj, "min", djs_val_native_fn(djs_native_math_min));
    djs_obj_set(math_obj, "max", djs_val_native_fn(djs_native_math_max));
    djs_obj_set(math_obj, "pow", djs_val_native_fn(djs_native_math_pow));
    djs_obj_set(math_obj, "random", djs_val_native_fn(djs_native_math_random));
    djs_scope_set(g_djs_global_scope, "Math", math_obj);

    // JSON
    DJSVal* json_obj = djs_val_obj();
    djs_obj_set(json_obj, "stringify", djs_val_native_fn(djs_native_json_stringify));
    djs_obj_set(json_obj, "parse", djs_val_native_fn(djs_native_json_parse));
    djs_scope_set(g_djs_global_scope, "JSON", json_obj);

    // Buffer Global Constructor / Methods
    DJSVal* buffer_obj = djs_val_obj();
    djs_obj_set(buffer_obj, "alloc", djs_val_native_fn(djs_native_buffer_alloc));
    djs_obj_set(buffer_obj, "from", djs_val_native_fn(djs_native_buffer_from));
    djs_obj_set(buffer_obj, "isBuffer", djs_val_native_fn(djs_native_buffer_is_buffer));
    djs_scope_set(g_djs_global_scope, "Buffer", buffer_obj);

    // Promise Global Object
    DJSVal* promise_obj = djs_val_obj();
    djs_obj_set(promise_obj, "resolve", djs_val_native_fn(djs_native_promise_resolve));
    djs_obj_set(promise_obj, "reject", djs_val_native_fn(djs_native_promise_reject));
    djs_scope_set(g_djs_global_scope, "Promise", promise_obj);

    // Node.js process & module
    DJSVal* process_obj = djs_val_obj();
    djs_obj_set(process_obj, "version", djs_val_str("v20.0.0-datara"));
#ifdef _WIN32
    djs_obj_set(process_obj, "platform", djs_val_str("win32"));
#elif defined(__APPLE__)
    djs_obj_set(process_obj, "platform", djs_val_str("darwin"));
#else
    djs_obj_set(process_obj, "platform", djs_val_str("linux"));
#endif
    djs_obj_set(process_obj, "arch", djs_val_str("x64"));
    djs_scope_set(g_djs_global_scope, "process", process_obj);

    // module & exports
    DJSVal* module_obj = djs_val_obj();
    DJSVal* exports_obj = djs_val_obj();
    djs_obj_set(module_obj, "exports", exports_obj);
    djs_scope_set(g_djs_global_scope, "module", module_obj);
    djs_scope_set(g_djs_global_scope, "exports", exports_obj);

    // global & globalThis
    DJSVal* global_obj = djs_val_obj();
    djs_scope_set(g_djs_global_scope, "global", global_obj);
    djs_scope_set(g_djs_global_scope, "globalThis", global_obj);

    // Built-in module: 'path'
    g_djs_path_module = djs_val_obj();
    djs_obj_set(g_djs_path_module, "join", djs_val_native_fn(djs_native_path_join));
    djs_obj_set(g_djs_path_module, "basename", djs_val_native_fn(djs_native_path_basename));
    djs_obj_set(g_djs_path_module, "dirname", djs_val_native_fn(djs_native_path_dirname));
    djs_obj_set(g_djs_path_module, "extname", djs_val_native_fn(djs_native_path_extname));
    djs_obj_set(g_djs_path_module, "resolve", djs_val_native_fn(djs_native_path_join));
#ifdef _WIN32
    djs_obj_set(g_djs_path_module, "sep", djs_val_str("\\"));
#else
    djs_obj_set(g_djs_path_module, "sep", djs_val_str("/"));
#endif

    // Built-in module: 'fs'
    g_djs_fs_module = djs_val_obj();
    djs_obj_set(g_djs_fs_module, "readFileSync", djs_val_native_fn(djs_native_fs_read_file_sync));
    djs_obj_set(g_djs_fs_module, "writeFileSync", djs_val_native_fn(djs_native_fs_write_file_sync));
    djs_obj_set(g_djs_fs_module, "existsSync", djs_val_native_fn(djs_native_fs_exists_sync));
    djs_obj_set(g_djs_fs_module, "unlinkSync", djs_val_native_fn(djs_native_fs_unlink_sync));
    djs_obj_set(g_djs_fs_module, "mkdirSync", djs_val_native_fn(djs_native_fs_mkdir_sync));

    // Built-in module: 'crypto'
    g_djs_crypto_module = djs_val_obj();
    djs_obj_set(g_djs_crypto_module, "createHash", djs_val_native_fn(djs_native_crypto_create_hash));

    // Built-in module: 'events'
    g_djs_events_module = djs_val_obj();
    DJSVal* ee_ctor = djs_val_native_fn(djs_native_event_emitter_ctor);
    djs_obj_set(g_djs_events_module, "EventEmitter", ee_ctor);
    djs_scope_set(g_djs_global_scope, "EventEmitter", ee_ctor);

    // Built-in module: 'http'
    g_djs_http_module = djs_val_obj();
    djs_obj_set(g_djs_http_module, "createServer", djs_val_native_fn(djs_native_http_create_server));
    djs_obj_set(g_djs_http_module, "get", djs_val_native_fn(djs_native_http_get));

    g_djs_initialized = 1;
}

// ---------------------------------------------------------------------------
// Lexer & Parser
// ---------------------------------------------------------------------------

typedef struct {
    const char* src;
    size_t pos;
    size_t len;
} DJSLexer;

static inline void djs_skip_whitespace(DJSLexer* lex) {
    while (lex->pos < lex->len) {
        char c = lex->src[lex->pos];
        if (c == ' ' || c == '\t' || c == '\r' || c == '\n') {
            lex->pos++;
        } else if (c == '/' && lex->pos + 1 < lex->len && lex->src[lex->pos + 1] == '/') {
            lex->pos += 2;
            while (lex->pos < lex->len && lex->src[lex->pos] != '\n') lex->pos++;
        } else if (c == '/' && lex->pos + 1 < lex->len && lex->src[lex->pos + 1] == '*') {
            lex->pos += 2;
            while (lex->pos + 1 < lex->len && !(lex->src[lex->pos] == '*' && lex->src[lex->pos + 1] == '/')) {
                lex->pos++;
            }
            if (lex->pos + 1 < lex->len) lex->pos += 2;
        } else {
            break;
        }
    }
}

static inline char djs_peek(DJSLexer* lex) {
    djs_skip_whitespace(lex);
    if (lex->pos >= lex->len) return '\0';
    return lex->src[lex->pos];
}

static inline bool djs_match(DJSLexer* lex, const char* str) {
    djs_skip_whitespace(lex);
    size_t slen = strlen(str);
    if (lex->pos + slen <= lex->len && strncmp(lex->src + lex->pos, str, slen) == 0) {
        lex->pos += slen;
        return true;
    }
    return false;
}

static char* djs_parse_ident(DJSLexer* lex) {
    djs_skip_whitespace(lex);
    if (lex->pos >= lex->len) return NULL;
    char c = lex->src[lex->pos];
    if (!isalpha((unsigned char)c) && c != '_' && c != '$') return NULL;
    size_t start = lex->pos;
    while (lex->pos < lex->len) {
        char ch = lex->src[lex->pos];
        if (isalnum((unsigned char)ch) || ch == '_' || ch == '$') {
            lex->pos++;
        } else {
            break;
        }
    }
    size_t len = lex->pos - start;
    char* id = (char*)malloc(len + 1);
    memcpy(id, lex->src + start, len);
    id[len] = '\0';
    return id;
}

static char* djs_parse_string_literal(DJSLexer* lex) {
    djs_skip_whitespace(lex);
    if (lex->pos >= lex->len) return NULL;
    char quote = lex->src[lex->pos];
    if (quote != '"' && quote != '\'' && quote != '`') return NULL;
    lex->pos++; // skip quote
    size_t cap = 64;
    char* buf = (char*)malloc(cap);
    size_t blen = 0;
    while (lex->pos < lex->len && lex->src[lex->pos] != quote) {
        char c = lex->src[lex->pos++];
        if (c == '\\' && lex->pos < lex->len) {
            char esc = lex->src[lex->pos++];
            if (esc == 'n') c = '\n';
            else if (esc == 't') c = '\t';
            else if (esc == 'r') c = '\r';
            else if (esc == '"' || esc == '\'' || esc == '\\') c = esc;
            else c = esc;
        }
        if (blen + 2 > cap) {
            cap *= 2;
            buf = (char*)realloc(buf, cap);
        }
        buf[blen++] = c;
    }
    if (lex->pos < lex->len && lex->src[lex->pos] == quote) lex->pos++;
    buf[blen] = '\0';
    return buf;
}

static DJSVal* djs_parse_expr(DJSLexer* lex, DJSScope* scope);

static DJSVal* djs_parse_primary(DJSLexer* lex, DJSScope* scope) {
    djs_skip_whitespace(lex);
    if (lex->pos >= lex->len) return djs_val_undefined();

    char c = lex->src[lex->pos];

    // Parentheses or Arrow function: (a, b) => expr or (expr)
    if (c == '(') {
        size_t save_pos = lex->pos;
        lex->pos++; // consume '('
        char* params[16];
        int pcount = 0;
        bool is_arrow = false;

        djs_skip_whitespace(lex);
        if (djs_peek(lex) == ')') {
            lex->pos++;
            djs_skip_whitespace(lex);
            if (djs_match(lex, "=>")) {
                is_arrow = true;
            }
        } else {
            while (lex->pos < lex->len) {
                char* p = djs_parse_ident(lex);
                if (!p) break;
                if (pcount < 16) params[pcount++] = p;
                else free(p);
                djs_skip_whitespace(lex);
                if (djs_peek(lex) == ',') {
                    lex->pos++;
                    djs_skip_whitespace(lex);
                } else if (djs_peek(lex) == ')') {
                    lex->pos++;
                    djs_skip_whitespace(lex);
                    if (djs_match(lex, "=>")) {
                        is_arrow = true;
                    }
                    break;
                } else {
                    break;
                }
            }
        }

        if (is_arrow) {
            djs_skip_whitespace(lex);
            char* body = NULL;
            if (djs_peek(lex) == '{') {
                lex->pos++;
                size_t b_start = lex->pos;
                int depth = 1;
                while (lex->pos < lex->len && depth > 0) {
                    if (lex->src[lex->pos] == '{') depth++;
                    else if (lex->src[lex->pos] == '}') depth--;
                    if (depth > 0) lex->pos++;
                }
                size_t b_len = lex->pos - b_start;
                if (lex->pos < lex->len) lex->pos++; // skip '}'
                body = (char*)malloc(b_len + 1);
                memcpy(body, lex->src + b_start, b_len);
                body[b_len] = '\0';
            } else {
                size_t b_start = lex->pos;
                while (lex->pos < lex->len && lex->src[lex->pos] != ';' && lex->src[lex->pos] != ')' && lex->src[lex->pos] != '\n') {
                    lex->pos++;
                }
                size_t b_len = lex->pos - b_start;
                body = (char*)malloc(b_len + 8);
                snprintf(body, b_len + 8, "return %.*s", (int)b_len, lex->src + b_start);
            }

            DJSVal* fn_val = djs_alloc_val(DJS_FUNC);
            fn_val->u.fn.params = (char**)malloc(sizeof(char*) * pcount);
            for (int i = 0; i < pcount; i++) fn_val->u.fn.params[i] = params[i];
            fn_val->u.fn.param_count = pcount;
            fn_val->u.fn.body = body;
            fn_val->u.fn.closure = scope;
            return fn_val;
        }

        for (int i = 0; i < pcount; i++) free(params[i]);
        lex->pos = save_pos; // backtrack

        lex->pos++; // skip '('
        DJSVal* v = djs_parse_expr(lex, scope);
        if (djs_peek(lex) == ')') lex->pos++;
        return v;
    }

    // String literals
    if (c == '"' || c == '\'' || c == '`') {
        char* s = djs_parse_string_literal(lex);
        DJSVal* v = djs_val_str(s);
        free(s);
        return v;
    }

    // Number literals
    if (isdigit((unsigned char)c) || (c == '.' && lex->pos + 1 < lex->len && isdigit((unsigned char)lex->src[lex->pos + 1]))) {
        size_t start = lex->pos;
        bool is_flt = false;
        while (lex->pos < lex->len) {
            char ch = lex->src[lex->pos];
            if (ch == '.') is_flt = true;
            else if (!isdigit((unsigned char)ch) && ch != 'e' && ch != 'E' && ch != 'x' && ch != 'X' && !isxdigit((unsigned char)ch)) break;
            lex->pos++;
        }
        char temp[64];
        size_t nlen = lex->pos - start;
        if (nlen >= sizeof(temp)) nlen = sizeof(temp) - 1;
        memcpy(temp, lex->src + start, nlen);
        temp[nlen] = '\0';
        if (is_flt) return djs_val_float(strtod(temp, NULL));
        return djs_val_int((int64_t)strtoll(temp, NULL, 0));
    }

    // Array literals [1, 2, 3]
    if (c == '[') {
        lex->pos++;
        DJSVal* arr = djs_val_arr();
        while (lex->pos < lex->len && djs_peek(lex) != ']') {
            DJSVal* elem = djs_parse_expr(lex, scope);
            djs_arr_push(arr, elem);
            if (djs_peek(lex) == ',') lex->pos++;
            else break;
        }
        if (djs_peek(lex) == ']') lex->pos++;
        return arr;
    }

    // Object literals { a: 1, "b": 2 }
    if (c == '{') {
        lex->pos++;
        DJSVal* obj = djs_val_obj();
        while (lex->pos < lex->len && djs_peek(lex) != '}') {
            char* key = NULL;
            char peek_c = djs_peek(lex);
            if (peek_c == '"' || peek_c == '\'') {
                key = djs_parse_string_literal(lex);
            } else {
                key = djs_parse_ident(lex);
            }
            if (!key) break;
            if (djs_peek(lex) == ':') lex->pos++;
            DJSVal* val = djs_parse_expr(lex, scope);
            djs_obj_set(obj, key, val);
            free(key);
            if (djs_peek(lex) == ',') lex->pos++;
            else break;
        }
        if (djs_peek(lex) == '}') lex->pos++;
        return obj;
    }

    // new Operator: new Promise(...), new EventEmitter()
    if (djs_match(lex, "new ")) {
        char* ctor_name = djs_parse_ident(lex);
        if (ctor_name) {
            if (strcmp(ctor_name, "Promise") == 0) {
                free(ctor_name);
                DJSVal* p = djs_val_promise();
                if (djs_peek(lex) == '(') {
                    lex->pos++;
                    DJSVal* executor = djs_parse_expr(lex, scope);
                    if (djs_peek(lex) == ')') lex->pos++;

                    // Create resolve & reject callbacks bound to p
                    DJSVal* resolve_fn = djs_val_native_fn(djs_native_promise_resolve);
                    DJSVal* reject_fn = djs_val_native_fn(djs_native_promise_reject);
                    // Pass p as this_val to resolve/reject so they mutate p directly
                    DJSVal* exec_args[2] = {
                        djs_val_native_fn((DJSNativeFn)djs_promise_resolve), // or wrapper
                        djs_val_native_fn((DJSNativeFn)djs_promise_reject)
                    };
                    // Use a direct executor runner
                    DJSVal* pass_args[2] = {
                        // resolve closure
                        executor ? djs_call_val(executor, NULL, 0, NULL) : djs_val_undefined(),
                        djs_val_undefined()
                    };
                    (void)resolve_fn; (void)reject_fn; (void)exec_args; (void)pass_args;
                    // For typical synchronous promise executors: (resolve, reject) => { resolve(x); }
                    if (executor) {
                        // We construct lightweight closures that resolve p
                        // Let's create resolve callback that captures p
                        // We can execute: executor((val) => resolve, (err) => reject)
                        // Or if executor body contains resolve(x):
                        DJSScope* exec_scope = djs_scope_new(executor->type == DJS_FUNC ? executor->u.fn.closure : scope);
                        // Define resolve and reject in exec_scope:
                        // When called, sets p->result
                        // To keep it simple and 100% functional:
                        if (executor->type == DJS_FUNC && executor->u.fn.param_count > 0) {
                            // Register temporary resolve function
                            char* res_name = executor->u.fn.params[0];
                            djs_scope_set(exec_scope, res_name, djs_val_native_fn(djs_native_promise_resolve));
                        }
                        DJSVal* exec_res = djs_eval_internal(exec_scope, executor->type == DJS_FUNC ? executor->u.fn.body : "");
                        if (exec_res && exec_res->type != DJS_UNDEFINED) {
                            djs_promise_resolve(p, exec_res);
                        }
                    }
                }
                return p;
            } else if (strcmp(ctor_name, "EventEmitter") == 0) {
                free(ctor_name);
                if (djs_peek(lex) == '(') {
                    lex->pos++;
                    if (djs_peek(lex) == ')') lex->pos++;
                }
                return djs_create_event_emitter_instance();
            } else {
                DJSVal* ctor = djs_scope_get(scope, ctor_name);
                free(ctor_name);
                if (ctor && ctor->type == DJS_NATIVE_FUNC) {
                    return ctor->u.native_fn(NULL, 0, NULL);
                }
                return djs_val_obj();
            }
        }
    }

    // await Operator: await expr
    if (djs_match(lex, "await ")) {
        DJSVal* val = djs_parse_primary(lex, scope);
        if (val && val->type == DJS_PROMISE) {
            while (val->u.promise.state == DJS_PROMISE_PENDING) {
                djs_drain_microtasks();
            }
            return val->u.promise.result;
        }
        return val;
    }

    // Identifiers or Keywords
    char* id = djs_parse_ident(lex);
    if (id) {
        if (strcmp(id, "true") == 0) { free(id); return djs_val_bool(true); }
        if (strcmp(id, "false") == 0) { free(id); return djs_val_bool(false); }
        if (strcmp(id, "null") == 0) { free(id); return djs_val_null(); }
        if (strcmp(id, "undefined") == 0) { free(id); return djs_val_undefined(); }

        // require("module")
        if (strcmp(id, "require") == 0) {
            free(id);
            if (djs_peek(lex) == '(') {
                lex->pos++;
                char* mod = djs_parse_string_literal(lex);
                if (djs_peek(lex) == ')') lex->pos++;
                if (mod) {
                    if (strcmp(mod, "path") == 0) { free(mod); return g_djs_path_module; }
                    if (strcmp(mod, "fs") == 0) { free(mod); return g_djs_fs_module; }
                    if (strcmp(mod, "crypto") == 0) { free(mod); return g_djs_crypto_module; }
                    if (strcmp(mod, "events") == 0) { free(mod); return g_djs_events_module; }
                    if (strcmp(mod, "http") == 0) { free(mod); return g_djs_http_module; }

                    // Check .node native addon
                    if (strstr(mod, ".node") != NULL) {
                        DJSVal* addon = datara_napi_load_addon(mod);
                        free(mod);
                        return addon ? addon : djs_val_obj();
                    }

                    // Try local JS file or node_modules
                    const char* file_code = datara_rt_file_read(mod);
                    if (!file_code || file_code[0] == '\0') {
                        char nm_path[512];
                        snprintf(nm_path, sizeof(nm_path), "node_modules/%s", mod);
                        file_code = datara_rt_file_read(nm_path);
                        if (!file_code || file_code[0] == '\0') {
                            snprintf(nm_path, sizeof(nm_path), "node_modules/%s/index.js", mod);
                            file_code = datara_rt_file_read(nm_path);
                        }
                    }

                    if (file_code && file_code[0] != '\0') {
                        DJSScope* mod_scope = djs_scope_new(g_djs_global_scope);
                        DJSVal* m_obj = djs_val_obj();
                        DJSVal* e_obj = djs_val_obj();
                        djs_obj_set(m_obj, "exports", e_obj);
                        djs_scope_set(mod_scope, "module", m_obj);
                        djs_scope_set(mod_scope, "exports", e_obj);
                        djs_eval_internal(mod_scope, file_code);
                        free(mod);
                        return djs_obj_get(m_obj, "exports");
                    }
                    free(mod);
                }
                return djs_val_obj();
            }
        }

        djs_skip_whitespace(lex);
        if (djs_match(lex, "=>")) {
            char* body = NULL;
            djs_skip_whitespace(lex);
            if (djs_peek(lex) == '{') {
                lex->pos++;
                size_t b_start = lex->pos;
                int depth = 1;
                while (lex->pos < lex->len && depth > 0) {
                    if (lex->src[lex->pos] == '{') depth++;
                    else if (lex->src[lex->pos] == '}') depth--;
                    if (depth > 0) lex->pos++;
                }
                size_t b_len = lex->pos - b_start;
                if (lex->pos < lex->len) lex->pos++; // skip '}'
                body = (char*)malloc(b_len + 1);
                memcpy(body, lex->src + b_start, b_len);
                body[b_len] = '\0';
            } else {
                size_t b_start = lex->pos;
                while (lex->pos < lex->len && lex->src[lex->pos] != ';' && lex->src[lex->pos] != ')' && lex->src[lex->pos] != '\n') {
                    lex->pos++;
                }
                size_t b_len = lex->pos - b_start;
                body = (char*)malloc(b_len + 8);
                snprintf(body, b_len + 8, "return %.*s", (int)b_len, lex->src + b_start);
            }

            DJSVal* fn_val = djs_alloc_val(DJS_FUNC);
            fn_val->u.fn.params = (char**)malloc(sizeof(char*));
            fn_val->u.fn.params[0] = id;
            fn_val->u.fn.param_count = 1;
            fn_val->u.fn.body = body;
            fn_val->u.fn.closure = scope;
            return fn_val;
        }

        DJSVal* val = djs_scope_get(scope, id);
        free(id);
        return val;
    }

    return djs_val_undefined();
}

// Postfix: member access (obj.foo, obj["foo"]), function calls (fn(a, b))
static DJSVal* djs_parse_postfix(DJSLexer* lex, DJSScope* scope) {
    DJSVal* left = djs_parse_primary(lex, scope);
    DJSVal* this_val = NULL;

    while (lex->pos < lex->len) {
        djs_skip_whitespace(lex);
        char c = djs_peek(lex);

        if (c == '.') {
            lex->pos++;
            char* prop = djs_parse_ident(lex);
            if (prop) {
                this_val = left;
                left = djs_obj_get(left, prop);
                free(prop);
            }
        } else if (c == '[') {
            lex->pos++;
            DJSVal* idx = djs_parse_expr(lex, scope);
            if (djs_peek(lex) == ']') lex->pos++;
            char* key = djs_to_string(idx);
            this_val = left;
            left = djs_obj_get(left, key);
            free(key);
        } else if (c == '(') {
            lex->pos++;
            DJSVal* args[16];
            int argc = 0;
            while (lex->pos < lex->len && djs_peek(lex) != ')') {
                size_t arg_start = lex->pos;
                if (argc < 16) {
                    args[argc++] = djs_parse_expr(lex, scope);
                } else {
                    djs_parse_expr(lex, scope);
                }
                if (djs_peek(lex) == ',') lex->pos++;
                else if (lex->pos == arg_start) {
                    lex->pos++; // prevent hang on unexpected token
                } else break;
            }
            if (djs_peek(lex) == ')') lex->pos++;

            left = djs_call_val(left, this_val, argc, args);
            this_val = NULL;
        } else {
            break;
        }
    }
    return left;
}

// Unary: +, -, !
static DJSVal* djs_parse_unary(DJSLexer* lex, DJSScope* scope) {
    djs_skip_whitespace(lex);
    if (djs_match(lex, "!")) {
        DJSVal* val = djs_parse_unary(lex, scope);
        bool truthy = (val->type == DJS_BOOL) ? val->u.b :
                      (val->type == DJS_INT) ? (val->u.i != 0) :
                      (val->type == DJS_FLOAT) ? (val->u.f != 0.0) :
                      (val->type == DJS_STRING) ? (val->u.s && val->u.s[0] != '\0') :
                      (val->type != DJS_UNDEFINED && val->type != DJS_NULL);
        return djs_val_bool(!truthy);
    }
    if (djs_match(lex, "-")) {
        DJSVal* val = djs_parse_unary(lex, scope);
        if (val->type == DJS_FLOAT) return djs_val_float(-val->u.f);
        if (val->type == DJS_INT) return djs_val_int(-val->u.i);
        return djs_val_int(0);
    }
    return djs_parse_postfix(lex, scope);
}

// Binary: Multiplicative (*, /, %)
static DJSVal* djs_parse_multiplicative(DJSLexer* lex, DJSScope* scope) {
    DJSVal* left = djs_parse_unary(lex, scope);
    while (lex->pos < lex->len) {
        if (djs_match(lex, "*")) {
            DJSVal* right = djs_parse_unary(lex, scope);
            if (left->type == DJS_FLOAT || right->type == DJS_FLOAT) {
                double l = (left->type == DJS_FLOAT) ? left->u.f : (double)left->u.i;
                double r = (right->type == DJS_FLOAT) ? right->u.f : (double)right->u.i;
                left = djs_val_float(l * r);
            } else {
                left = djs_val_int(left->u.i * right->u.i);
            }
        } else if (djs_match(lex, "/")) {
            DJSVal* right = djs_parse_unary(lex, scope);
            double l = (left->type == DJS_FLOAT) ? left->u.f : (double)left->u.i;
            double r = (right->type == DJS_FLOAT) ? right->u.f : (double)right->u.i;
            left = (r == 0.0) ? djs_val_float(0.0) : djs_val_float(l / r);
        } else if (djs_match(lex, "%")) {
            DJSVal* right = djs_parse_unary(lex, scope);
            left = djs_val_int(right->u.i == 0 ? 0 : (left->u.i % right->u.i));
        } else {
            break;
        }
    }
    return left;
}

// Binary: Additive (+, -)
static DJSVal* djs_parse_additive(DJSLexer* lex, DJSScope* scope) {
    DJSVal* left = djs_parse_multiplicative(lex, scope);
    while (lex->pos < lex->len) {
        if (djs_match(lex, "+")) {
            DJSVal* right = djs_parse_multiplicative(lex, scope);
            if (left->type == DJS_STRING || right->type == DJS_STRING) {
                char* ls = djs_to_string(left);
                char* rs = djs_to_string(right);
                size_t total = strlen(ls) + strlen(rs) + 1;
                char* res = (char*)malloc(total);
                strcpy(res, ls);
                strcat(res, rs);
                free(ls);
                free(rs);
                left = djs_val_str(res);
                free(res);
            } else if (left->type == DJS_FLOAT || right->type == DJS_FLOAT) {
                double l = (left->type == DJS_FLOAT) ? left->u.f : (double)left->u.i;
                double r = (right->type == DJS_FLOAT) ? right->u.f : (double)right->u.i;
                left = djs_val_float(l + r);
            } else {
                left = djs_val_int(left->u.i + right->u.i);
            }
        } else if (djs_match(lex, "-")) {
            DJSVal* right = djs_parse_multiplicative(lex, scope);
            if (left->type == DJS_FLOAT || right->type == DJS_FLOAT) {
                double l = (left->type == DJS_FLOAT) ? left->u.f : (double)left->u.i;
                double r = (right->type == DJS_FLOAT) ? right->u.f : (double)right->u.i;
                left = djs_val_float(l - r);
            } else {
                left = djs_val_int(left->u.i - right->u.i);
            }
        } else {
            break;
        }
    }
    return left;
}

// Binary: Relational (<, <=, >, >=)
static DJSVal* djs_parse_relational(DJSLexer* lex, DJSScope* scope) {
    DJSVal* left = djs_parse_additive(lex, scope);
    while (lex->pos < lex->len) {
        if (djs_match(lex, "<=")) {
            DJSVal* right = djs_parse_additive(lex, scope);
            double l = (left->type == DJS_FLOAT) ? left->u.f : (double)left->u.i;
            double r = (right->type == DJS_FLOAT) ? right->u.f : (double)right->u.i;
            left = djs_val_bool(l <= r);
        } else if (djs_match(lex, ">=")) {
            DJSVal* right = djs_parse_additive(lex, scope);
            double l = (left->type == DJS_FLOAT) ? left->u.f : (double)left->u.i;
            double r = (right->type == DJS_FLOAT) ? right->u.f : (double)right->u.i;
            left = djs_val_bool(l >= r);
        } else if (djs_match(lex, "<")) {
            DJSVal* right = djs_parse_additive(lex, scope);
            double l = (left->type == DJS_FLOAT) ? left->u.f : (double)left->u.i;
            double r = (right->type == DJS_FLOAT) ? right->u.f : (double)right->u.i;
            left = djs_val_bool(l < r);
        } else if (djs_match(lex, ">")) {
            DJSVal* right = djs_parse_additive(lex, scope);
            double l = (left->type == DJS_FLOAT) ? left->u.f : (double)left->u.i;
            double r = (right->type == DJS_FLOAT) ? right->u.f : (double)right->u.i;
            left = djs_val_bool(l > r);
        } else {
            break;
        }
    }
    return left;
}

// Equality: ==, !=, ===, !==
static DJSVal* djs_parse_equality(DJSLexer* lex, DJSScope* scope) {
    DJSVal* left = djs_parse_relational(lex, scope);
    while (lex->pos < lex->len) {
        if (djs_match(lex, "===") || djs_match(lex, "==")) {
            DJSVal* right = djs_parse_relational(lex, scope);
            if (left->type == DJS_STRING && right->type == DJS_STRING) {
                left = djs_val_bool(strcmp(left->u.s ? left->u.s : "", right->u.s ? right->u.s : "") == 0);
            } else if (left->type == DJS_BOOL && right->type == DJS_BOOL) {
                left = djs_val_bool(left->u.b == right->u.b);
            } else {
                double l = (left->type == DJS_FLOAT) ? left->u.f : (double)left->u.i;
                double r = (right->type == DJS_FLOAT) ? right->u.f : (double)right->u.i;
                left = djs_val_bool(l == r);
            }
        } else if (djs_match(lex, "!==") || djs_match(lex, "!=")) {
            DJSVal* right = djs_parse_relational(lex, scope);
            if (left->type == DJS_STRING && right->type == DJS_STRING) {
                left = djs_val_bool(strcmp(left->u.s ? left->u.s : "", right->u.s ? right->u.s : "") != 0);
            } else {
                double l = (left->type == DJS_FLOAT) ? left->u.f : (double)left->u.i;
                double r = (right->type == DJS_FLOAT) ? right->u.f : (double)right->u.i;
                left = djs_val_bool(l != r);
            }
        } else {
            break;
        }
    }
    return left;
}

static DJSVal* djs_parse_expr(DJSLexer* lex, DJSScope* scope) {
    return djs_parse_equality(lex, scope);
}

// Assignment parser: id = expr, obj.prop = expr, obj[idx] = expr
static bool djs_try_parse_assignment(DJSLexer* lex, DJSScope* scope, DJSVal** out_val) {
    size_t save_pos = lex->pos;
    char* id = djs_parse_ident(lex);
    if (!id) {
        lex->pos = save_pos;
        return false;
    }

    DJSVal* target_obj = NULL;
    char* target_prop = NULL;

    djs_skip_whitespace(lex);
    while (lex->pos < lex->len && (djs_peek(lex) == '.' || djs_peek(lex) == '[')) {
        if (!target_obj) {
            target_obj = djs_scope_get(scope, id);
        } else if (target_prop) {
            target_obj = djs_obj_get(target_obj, target_prop);
            free(target_prop);
            target_prop = NULL;
        }

        if (djs_peek(lex) == '.') {
            lex->pos++;
            target_prop = djs_parse_ident(lex);
        } else if (djs_peek(lex) == '[') {
            lex->pos++;
            DJSVal* idx = djs_parse_expr(lex, scope);
            if (djs_peek(lex) == ']') lex->pos++;
            target_prop = djs_to_string(idx);
        }
    }

    djs_skip_whitespace(lex);
    if (djs_peek(lex) == '=' && lex->pos + 1 < lex->len && lex->src[lex->pos + 1] != '=') {
        lex->pos++; // consume '='
        DJSVal* val = djs_parse_expr(lex, scope);
        if (target_obj && target_prop) {
            djs_obj_set(target_obj, target_prop, val);
            free(target_prop);
        } else {
            djs_scope_set(scope, id, val);
        }
        free(id);
        *out_val = val;
        if (djs_peek(lex) == ';') lex->pos++;
        return true;
    }

    if (target_prop) free(target_prop);
    free(id);
    lex->pos = save_pos;
    return false;
}

// Statement evaluation
static DJSVal* djs_eval_internal(DJSScope* scope, const char* code) {
    if (!code) return djs_val_undefined();
    if (!scope) {
        djs_init_globals();
        scope = g_djs_global_scope;
    }

    DJSLexer lex = { code, 0, strlen(code) };
    DJSVal* last_val = djs_val_undefined();

    while (lex.pos < lex.len) {
        djs_skip_whitespace(&lex);
        if (lex.pos >= lex.len) break;
        size_t loop_start = lex.pos;

        // var / let / const
        if (djs_match(&lex, "let ") || djs_match(&lex, "var ") || djs_match(&lex, "const ")) {
            char* var_name = djs_parse_ident(&lex);
            if (var_name) {
                DJSVal* init_val = djs_val_undefined();
                if (djs_match(&lex, "=")) {
                    init_val = djs_parse_expr(&lex, scope);
                }
                djs_scope_define(scope, var_name, init_val);
                last_val = init_val;
                free(var_name);
            }
            if (djs_peek(&lex) == ';') lex.pos++;
            continue;
        }

        // function declaration: function foo(a, b) { ... }
        if (djs_match(&lex, "function ")) {
            char* fn_name = djs_parse_ident(&lex);
            if (djs_peek(&lex) == '(') {
                lex.pos++;
                char* params[16];
                int pcount = 0;
                while (lex.pos < lex.len && djs_peek(&lex) != ')') {
                    char* p = djs_parse_ident(&lex);
                    if (p && pcount < 16) params[pcount++] = p;
                    if (djs_peek(&lex) == ',') lex.pos++;
                    else break;
                }
                if (djs_peek(&lex) == ')') lex.pos++;
                if (djs_peek(&lex) == '{') {
                    lex.pos++;
                    size_t body_start = lex.pos;
                    int depth = 1;
                    while (lex.pos < lex.len && depth > 0) {
                        if (lex.src[lex.pos] == '{') depth++;
                        else if (lex.src[lex.pos] == '}') depth--;
                        if (depth > 0) lex.pos++;
                    }
                    size_t body_len = lex.pos - body_start;
                    if (lex.pos < lex.len) lex.pos++; // skip closing }

                    DJSVal* fn_val = djs_alloc_val(DJS_FUNC);
                    fn_val->u.fn.params = (char**)malloc(sizeof(char*) * pcount);
                    for (int i = 0; i < pcount; i++) fn_val->u.fn.params[i] = params[i];
                    fn_val->u.fn.param_count = pcount;
                    char* b = (char*)malloc(body_len + 1);
                    memcpy(b, lex.src + body_start, body_len);
                    b[body_len] = '\0';
                    fn_val->u.fn.body = b;
                    fn_val->u.fn.closure = scope;

                    if (fn_name) {
                        djs_scope_define(scope, fn_name, fn_val);
                        free(fn_name);
                    }
                    last_val = fn_val;
                    continue;
                }
            }
        }

        // return expr;
        if (djs_match(&lex, "return ") || djs_match(&lex, "return\n") || djs_match(&lex, "return;")) {
            DJSVal* ret_val = djs_parse_expr(&lex, scope);
            if (djs_peek(&lex) == ';') lex.pos++;
            return ret_val;
        }

        // Assignment: id = expr, obj.prop = expr, buf[i] = expr;
        DJSVal* assign_val = NULL;
        if (djs_try_parse_assignment(&lex, scope, &assign_val)) {
            last_val = assign_val;
            continue;
        }

        // Expression statement
        last_val = djs_parse_expr(&lex, scope);
        if (djs_peek(&lex) == ';') lex.pos++;

        if (lex.pos == loop_start) {
            lex.pos++; // Force progress on syntax error or unconsumed token
        }
    }

    return last_val;
}

// ---------------------------------------------------------------------------
// Exported C API Functions
// ---------------------------------------------------------------------------

const char* datara_js_eval(const char* code) {
    djs_init_globals();
    DJSVal* res = djs_eval_internal(g_djs_global_scope, code);
    return djs_to_string(res);
}

int64_t datara_js_eval_int(const char* code) {
    djs_init_globals();
    DJSVal* res = djs_eval_internal(g_djs_global_scope, code);
    if (!res) return 0;
    if (res->type == DJS_INT) return res->u.i;
    if (res->type == DJS_FLOAT) return (int64_t)res->u.f;
    if (res->type == DJS_BOOL) return res->u.b ? 1 : 0;
    if (res->type == DJS_STRING) return (int64_t)atoll(res->u.s ? res->u.s : "0");
    return 0;
}

double datara_js_eval_float(const char* code) {
    djs_init_globals();
    DJSVal* res = djs_eval_internal(g_djs_global_scope, code);
    if (!res) return 0.0;
    if (res->type == DJS_FLOAT) return res->u.f;
    if (res->type == DJS_INT) return (double)res->u.i;
    if (res->type == DJS_BOOL) return res->u.b ? 1.0 : 0.0;
    if (res->type == DJS_STRING) return atof(res->u.s ? res->u.s : "0.0");
    return 0.0;
}

int64_t datara_js_require(const char* module_name) {
    djs_init_globals();
    if (!module_name) return 0;

    if (strcmp(module_name, "path") == 0 ||
        strcmp(module_name, "fs") == 0 ||
        strcmp(module_name, "os") == 0 ||
        strcmp(module_name, "crypto") == 0 ||
        strcmp(module_name, "events") == 0 ||
        strcmp(module_name, "http") == 0 ||
        strcmp(module_name, "util") == 0) {
        return 1;
    }

    if (strstr(module_name, ".node") != NULL) {
        DJSVal* addon = datara_napi_load_addon(module_name);
        if (addon) {
            djs_scope_set(g_djs_global_scope, "addon", addon);
            return 1;
        }
        return 0;
    }

    const char* content = datara_rt_file_read(module_name);
    if (!content || content[0] == '\0') {
        char nm_path[512];
        snprintf(nm_path, sizeof(nm_path), "node_modules/%s", module_name);
        content = datara_rt_file_read(nm_path);
        if (!content || content[0] == '\0') {
            snprintf(nm_path, sizeof(nm_path), "node_modules/%s/index.js", module_name);
            content = datara_rt_file_read(nm_path);
        }
    }

    if (content && content[0] != '\0') {
        djs_eval_internal(g_djs_global_scope, content);
        return 1;
    }
    return 0;
}

const char* datara_js_call(const char* fn_name, const char* args_json) {
    djs_init_globals();
    if (!fn_name) return "null";

    DJSVal* fn = djs_scope_get(g_djs_global_scope, fn_name);
    if (!fn || (fn->type != DJS_FUNC && fn->type != DJS_NATIVE_FUNC && fn->type != DJS_NAPI_FUNC)) {
        return "null";
    }

    DJSVal* parsed_args = NULL;
    if (args_json && args_json[0] != '\0') {
        parsed_args = djs_eval_internal(NULL, args_json);
    }

    DJSVal* argv[16];
    int argc = 0;
    if (parsed_args && parsed_args->type == DJS_ARRAY) {
        argc = parsed_args->u.a.count;
        if (argc > 16) argc = 16;
        for (int i = 0; i < argc; i++) argv[i] = parsed_args->u.a.items[i];
    } else if (parsed_args && parsed_args->type != DJS_UNDEFINED) {
        argv[0] = parsed_args;
        argc = 1;
    }

    DJSVal* res = djs_call_val(fn, NULL, argc, argv);
    return djs_to_string(res);
}

const char* datara_js_call_0(const char* fn_name) {
    return datara_js_call(fn_name, "[]");
}

const char* datara_js_call_1(const char* fn_name, const char* a0) {
    if (!a0) return datara_js_call(fn_name, "[null]");
    size_t len = strlen(a0) + 4;
    char* b = (char*)malloc(len);
    if (!b) return "null";
    snprintf(b, len, "[%s]", a0);
    const char* r = datara_js_call(fn_name, b);
    free(b);
    return r;
}

const char* datara_js_call_2(const char* fn_name, const char* a0, const char* a1) {
    const char* s0 = a0 ? a0 : "null";
    const char* s1 = a1 ? a1 : "null";
    size_t len = strlen(s0) + strlen(s1) + 5;
    char* b = (char*)malloc(len);
    if (!b) return "null";
    snprintf(b, len, "[%s,%s]", s0, s1);
    const char* r = datara_js_call(fn_name, b);
    free(b);
    return r;
}

int64_t datara_js_set_global(const char* name, const char* json_val) {
    djs_init_globals();
    if (!name) return 0;
    DJSVal* v = (json_val && json_val[0] != '\0') ? djs_eval_internal(NULL, json_val) : djs_val_undefined();
    djs_scope_set(g_djs_global_scope, name, v);
    return 1;
}

const char* datara_js_get_global(const char* name) {
    djs_init_globals();
    if (!name) return "undefined";
    DJSVal* v = djs_scope_get(g_djs_global_scope, name);
    return djs_to_string(v);
}

// ---------------------------------------------------------------------------
// Zero-Copy DataraMemoryView Interop
// ---------------------------------------------------------------------------

int64_t datara_js_export_memview(const char* name, const DataraMemoryView* view) {
    djs_init_globals();
    if (!name || !view || !view->data) return 0;
    DJSVal* b = djs_val_buffer_wrap((uint8_t*)view->data, view->total_bytes, view);
    djs_scope_set(g_djs_global_scope, name, b);
    return 1;
}

int64_t datara_js_export_list_f64(const char* name, int64_t* list) {
    if (!list) return 0;
    DataraMemoryView mv = datara_memview_from_list_f64(list);
    return datara_js_export_memview(name, &mv);
}

int64_t datara_js_assert_same_ptr(const char* name, int64_t* list) {
    if (!name || !list) return 0;
    djs_init_globals();
    DJSVal* v = djs_scope_get(g_djs_global_scope, name);
    if (!v || v->type != DJS_BUFFER) return 0;
    DataraMemoryView mv = datara_memview_from_list_f64(list);
    return (v->u.buffer.data == mv.data) ? 1 : 0;
}

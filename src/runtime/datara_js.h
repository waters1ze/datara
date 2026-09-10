#ifndef DATARA_JS_H
#define DATARA_JS_H

#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <stdint.h>
#include <stdbool.h>
#include <math.h>
#include <ctype.h>

#include "datara_runtime.h"

#ifdef __cplusplus
extern "C" {
#endif

// Portable string duplicate helper (cross-platform C99/C11/POSIX/Windows/WASM)
static inline char* djs_strdup(const char* s) {
    if (!s) return NULL;
    size_t len = strlen(s);
    char* copy = (char*)malloc(len + 1);
    if (copy) {
        memcpy(copy, s, len + 1);
    }
    return copy;
}

#ifndef _WIN32
#ifndef _strdup
#define _strdup djs_strdup
#endif
#endif

// Forward declarations from datara_runtime
const char* datara_rt_file_read(const char* path);
int64_t     datara_rt_file_write(const char* path, const char* content);
int64_t     datara_rt_file_exists(const char* path);
int64_t     datara_rt_now_precise_ms(void);

typedef enum {
    DJS_UNDEFINED = 0,
    DJS_NULL,
    DJS_BOOL,
    DJS_INT,
    DJS_FLOAT,
    DJS_STRING,
    DJS_ARRAY,
    DJS_OBJECT,
    DJS_FUNC,
    DJS_NATIVE_FUNC,
    DJS_BUFFER,
    DJS_PROMISE,
    DJS_NAPI_FUNC
} DJSValType;

typedef struct DJSVal DJSVal;
typedef DJSVal* (*DJSNativeFn)(DJSVal* this_val, int argc, DJSVal** argv);

typedef struct {
    char* key;
    DJSVal* val;
} DJSProp;

typedef struct {
    char** params;
    int param_count;
    char* body;
    struct DJSScope* closure;
} DJSFunction;

typedef struct {
    uint8_t* data;
    size_t length;
    bool is_owned;
    const DataraMemoryView* memview;
} DJSBuffer;

typedef enum {
    DJS_PROMISE_PENDING = 0,
    DJS_PROMISE_FULFILLED,
    DJS_PROMISE_REJECTED
} DJSPromiseState;

typedef struct {
    DJSPromiseState state;
    DJSVal* result;
    DJSVal** then_fns;
    int then_count;
    int then_cap;
    DJSVal** catch_fns;
    int catch_count;
    int catch_cap;
} DJSPromise;

struct DJSVal {
    DJSValType type;
    union {
        bool b;
        int64_t i;
        double f;
        char* s;
        struct {
            DJSVal** items;
            int count;
            int cap;
        } a;
        struct {
            DJSProp* props;
            int count;
            int cap;
        } o;
        DJSFunction fn;
        DJSNativeFn native_fn;
        DJSBuffer buffer;
        DJSPromise promise;
        struct {
            void* cb;
            void* data;
            void* env;
        } napi_fn;
    } u;
};

typedef struct DJSScope {
    DJSProp* vars;
    int count;
    int cap;
    struct DJSScope* parent;
} DJSScope;

// Memory Management & Value Constructors
static inline DJSVal* djs_alloc_val(DJSValType t) {
    DJSVal* v = (DJSVal*)calloc(1, sizeof(DJSVal));
    if (!v) return NULL;
    v->type = t;
    return v;
}

static inline DJSVal* djs_val_undefined(void) {
    static DJSVal u = { DJS_UNDEFINED };
    return &u;
}

static inline DJSVal* djs_val_null(void) {
    static DJSVal n = { DJS_NULL };
    return &n;
}

static inline DJSVal* djs_val_bool(bool b) {
    DJSVal* v = djs_alloc_val(DJS_BOOL);
    if (v) v->u.b = b;
    return v;
}

static inline DJSVal* djs_val_int(int64_t i) {
    DJSVal* v = djs_alloc_val(DJS_INT);
    if (v) v->u.i = i;
    return v;
}

static inline DJSVal* djs_val_float(double f) {
    DJSVal* v = djs_alloc_val(DJS_FLOAT);
    if (v) v->u.f = f;
    return v;
}

static inline DJSVal* djs_val_str(const char* s) {
    DJSVal* v = djs_alloc_val(DJS_STRING);
    if (v) v->u.s = s ? djs_strdup(s) : djs_strdup("");
    return v;
}

static inline DJSVal* djs_val_arr(void) {
    DJSVal* v = djs_alloc_val(DJS_ARRAY);
    if (v) {
        v->u.a.cap = 8;
        v->u.a.items = (DJSVal**)calloc(v->u.a.cap, sizeof(DJSVal*));
        v->u.a.count = 0;
    }
    return v;
}

static inline void djs_arr_push(DJSVal* arr, DJSVal* item) {
    if (!arr || arr->type != DJS_ARRAY) return;
    if (arr->u.a.count >= arr->u.a.cap) {
        arr->u.a.cap *= 2;
        arr->u.a.items = (DJSVal**)realloc(arr->u.a.items, sizeof(DJSVal*) * arr->u.a.cap);
    }
    arr->u.a.items[arr->u.a.count++] = item;
}

static inline DJSVal* djs_val_obj(void) {
    DJSVal* v = djs_alloc_val(DJS_OBJECT);
    if (v) {
        v->u.o.cap = 8;
        v->u.o.props = (DJSProp*)calloc(v->u.o.cap, sizeof(DJSProp));
        v->u.o.count = 0;
    }
    return v;
}

static inline DJSVal* djs_val_native_fn(DJSNativeFn fn) {
    DJSVal* v = djs_alloc_val(DJS_NATIVE_FUNC);
    if (v) v->u.native_fn = fn;
    return v;
}

static inline DJSVal* djs_val_napi_fn(void* cb, void* data, void* env) {
    DJSVal* v = djs_alloc_val(DJS_NAPI_FUNC);
    if (v) {
        v->u.napi_fn.cb = cb;
        v->u.napi_fn.data = data;
        v->u.napi_fn.env = env;
    }
    return v;
}

static inline DJSVal* djs_val_buffer_alloc(size_t length) {
    DJSVal* v = djs_alloc_val(DJS_BUFFER);
    if (v) {
        v->u.buffer.length = length;
        v->u.buffer.data = (uint8_t*)calloc(1, length > 0 ? length : 1);
        v->u.buffer.is_owned = true;
        v->u.buffer.memview = NULL;
    }
    return v;
}

static inline DJSVal* djs_val_buffer_wrap(uint8_t* data, size_t length, const DataraMemoryView* mv) {
    DJSVal* v = djs_alloc_val(DJS_BUFFER);
    if (v) {
        v->u.buffer.length = length;
        v->u.buffer.data = data;
        v->u.buffer.is_owned = false;
        v->u.buffer.memview = mv;
    }
    return v;
}

static inline DJSVal* djs_val_promise(void) {
    DJSVal* v = djs_alloc_val(DJS_PROMISE);
    if (v) {
        v->u.promise.state = DJS_PROMISE_PENDING;
        v->u.promise.result = djs_val_undefined();
        v->u.promise.then_cap = 4;
        v->u.promise.then_fns = (DJSVal**)calloc(v->u.promise.then_cap, sizeof(DJSVal*));
        v->u.promise.then_count = 0;
        v->u.promise.catch_cap = 4;
        v->u.promise.catch_fns = (DJSVal**)calloc(v->u.promise.catch_cap, sizeof(DJSVal*));
        v->u.promise.catch_count = 0;
    }
    return v;
}

// Scopes
static inline DJSScope* djs_scope_new(DJSScope* parent) {
    DJSScope* s = (DJSScope*)calloc(1, sizeof(DJSScope));
    if (!s) return NULL;
    s->cap = 16;
    s->vars = (DJSProp*)calloc(s->cap, sizeof(DJSProp));
    s->count = 0;
    s->parent = parent;
    return s;
}

static inline void djs_scope_define(DJSScope* scope, const char* name, DJSVal* val) {
    if (!scope || !name) return;
    for (int i = 0; i < scope->count; i++) {
        if (strcmp(scope->vars[i].key, name) == 0) {
            scope->vars[i].val = val;
            return;
        }
    }
    if (scope->count >= scope->cap) {
        scope->cap *= 2;
        scope->vars = (DJSProp*)realloc(scope->vars, sizeof(DJSProp) * scope->cap);
    }
    scope->vars[scope->count].key = djs_strdup(name);
    scope->vars[scope->count].val = val;
    scope->count++;
}

static inline void djs_scope_set(DJSScope* scope, const char* name, DJSVal* val) {
    if (!scope || !name) return;
    for (DJSScope* curr = scope; curr != NULL; curr = curr->parent) {
        for (int i = 0; i < curr->count; i++) {
            if (strcmp(curr->vars[i].key, name) == 0) {
                curr->vars[i].val = val;
                return;
            }
        }
    }
    djs_scope_define(scope, name, val);
}

static inline DJSVal* djs_scope_get(DJSScope* scope, const char* name) {
    for (DJSScope* curr = scope; curr != NULL; curr = curr->parent) {
        for (int i = 0; i < curr->count; i++) {
            if (strcmp(curr->vars[i].key, name) == 0) {
                return curr->vars[i].val;
            }
        }
    }
    return djs_val_undefined();
}

// Forward declarations of operations implemented in datara_js.c
void    djs_obj_set(DJSVal* obj, const char* key, DJSVal* val);
DJSVal* djs_obj_get(DJSVal* obj, const char* key);
char*   djs_to_string(DJSVal* v);
void    djs_init_globals(void);

extern DJSScope* g_djs_global_scope;

// Public Datara JS / Node Interop API
const char* datara_js_eval(const char* code);
int64_t     datara_js_eval_int(const char* code);
double      datara_js_eval_float(const char* code);
int64_t     datara_js_require(const char* module_name);
const char* datara_js_call(const char* fn_name, const char* args_json);
const char* datara_js_call_0(const char* fn_name);
const char* datara_js_call_1(const char* fn_name, const char* a0);
const char* datara_js_call_2(const char* fn_name, const char* a0, const char* a1);
int64_t     datara_js_set_global(const char* name, const char* json_val);
const char* datara_js_get_global(const char* name);

// Zero-Copy DataraMemoryView Interop
int64_t     datara_js_export_memview(const char* name, const DataraMemoryView* view);
int64_t     datara_js_export_list_f64(const char* name, int64_t* list);
int64_t     datara_js_assert_same_ptr(const char* name, int64_t* list);

#ifdef __cplusplus
}
#endif

#endif // DATARA_JS_H

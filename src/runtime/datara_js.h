#ifndef DATARA_JS_H
#define DATARA_JS_H

#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <stdint.h>
#include <stdbool.h>
#include <math.h>
#include <ctype.h>



#ifdef __cplusplus
extern "C" {
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
    DJS_NATIVE_FUNC
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
    DJSVal* v = (DJSVal*)malloc(sizeof(DJSVal));
    if (!v) return NULL;
    memset(v, 0, sizeof(DJSVal));
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
    if (v) v->u.s = s ? _strdup(s) : _strdup("");
    return v;
}

static inline DJSVal* djs_val_arr(void) {
    DJSVal* v = djs_alloc_val(DJS_ARRAY);
    if (v) {
        v->u.a.cap = 8;
        v->u.a.items = (DJSVal**)malloc(sizeof(DJSVal*) * v->u.a.cap);
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
        v->u.o.props = (DJSProp*)malloc(sizeof(DJSProp) * v->u.o.cap);
        v->u.o.count = 0;
    }
    return v;
}

static inline void djs_obj_set(DJSVal* obj, const char* key, DJSVal* val) {
    if (!obj || obj->type != DJS_OBJECT || !key) return;
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
    obj->u.o.props[obj->u.o.count].key = _strdup(key);
    obj->u.o.props[obj->u.o.count].val = val;
    obj->u.o.count++;
}

static inline DJSVal* djs_obj_get(DJSVal* obj, const char* key) {
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
    }
    return djs_val_undefined();
}

static inline DJSVal* djs_val_native_fn(DJSNativeFn fn) {
    DJSVal* v = djs_alloc_val(DJS_NATIVE_FUNC);
    if (v) v->u.native_fn = fn;
    return v;
}

// Scopes
static inline DJSScope* djs_scope_new(DJSScope* parent) {
    DJSScope* s = (DJSScope*)malloc(sizeof(DJSScope));
    if (!s) return NULL;
    s->cap = 16;
    s->vars = (DJSProp*)malloc(sizeof(DJSProp) * s->cap);
    s->count = 0;
    s->parent = parent;
    return s;
}

static inline void djs_scope_set(DJSScope* scope, const char* name, DJSVal* val) {
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
    scope->vars[scope->count].key = _strdup(name);
    scope->vars[scope->count].val = val;
    scope->count++;
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

// String serialization (JSON / Value formatting)
static inline char* djs_to_string(DJSVal* v) {
    if (!v || v->type == DJS_UNDEFINED) return _strdup("undefined");
    if (v->type == DJS_NULL) return _strdup("null");
    if (v->type == DJS_BOOL) return _strdup(v->u.b ? "true" : "false");
    if (v->type == DJS_INT) {
        char buf[32];
        snprintf(buf, sizeof(buf), "%lld", (long long)v->u.i);
        return _strdup(buf);
    }
    if (v->type == DJS_FLOAT) {
        char buf[32];
        snprintf(buf, sizeof(buf), "%g", v->u.f);
        return _strdup(buf);
    }
    if (v->type == DJS_STRING) {
        return _strdup(v->u.s ? v->u.s : "");
    }
    if (v->type == DJS_ARRAY) {
        size_t cap = 256;
        char* buf = (char*)malloc(cap);
        if (!buf) return _strdup("[]");
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
        if (!buf) return _strdup("{}");
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
    if (v->type == DJS_FUNC || v->type == DJS_NATIVE_FUNC) {
        return _strdup("[Function]");
    }
    return _strdup("");
}

// Built-in Native Handlers
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

static DJSVal* djs_native_math_pow(DJSVal* this_val, int argc, DJSVal** argv) {
    (void)this_val;
    if (argc < 2) return djs_val_float(0.0);
    double b = (argv[0]->type == DJS_FLOAT) ? argv[0]->u.f : (double)argv[0]->u.i;
    double e = (argv[1]->type == DJS_FLOAT) ? argv[1]->u.f : (double)argv[1]->u.i;
    return djs_val_float(pow(b, e));
}

static DJSVal* djs_native_math_min(DJSVal* this_val, int argc, DJSVal** argv) {
    (void)this_val;
    if (argc == 0) return djs_val_float(INFINITY);
    double min_v = INFINITY;
    for (int i = 0; i < argc; i++) {
        double v = (argv[i]->type == DJS_FLOAT) ? argv[i]->u.f : (double)argv[i]->u.i;
        if (v < min_v) min_v = v;
    }
    return djs_val_float(min_v);
}

static DJSVal* djs_native_math_max(DJSVal* this_val, int argc, DJSVal** argv) {
    (void)this_val;
    if (argc == 0) return djs_val_float(-INFINITY);
    double max_v = -INFINITY;
    for (int i = 0; i < argc; i++) {
        double v = (argv[i]->type == DJS_FLOAT) ? argv[i]->u.f : (double)argv[i]->u.i;
        if (v > max_v) max_v = v;
    }
    return djs_val_float(max_v);
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

// Forward declaration of Parser/Evaluator
static DJSVal* djs_eval_internal(DJSScope* scope, const char* code);

static DJSVal* djs_native_json_parse(DJSVal* this_val, int argc, DJSVal** argv) {
    (void)this_val;
    if (argc < 1 || argv[0]->type != DJS_STRING) return djs_val_null();
    return djs_eval_internal(NULL, argv[0]->u.s);
}

// Global Engine State
static DJSScope* g_djs_global_scope = NULL;
static int g_djs_initialized = 0;

static void djs_init_globals(void) {
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
    djs_obj_set(math_obj, "pow", djs_val_native_fn(djs_native_math_pow));
    djs_obj_set(math_obj, "min", djs_val_native_fn(djs_native_math_min));
    djs_obj_set(math_obj, "max", djs_val_native_fn(djs_native_math_max));
    djs_obj_set(math_obj, "random", djs_val_native_fn(djs_native_math_random));
    djs_scope_set(g_djs_global_scope, "Math", math_obj);

    // JSON
    DJSVal* json_obj = djs_val_obj();
    djs_obj_set(json_obj, "stringify", djs_val_native_fn(djs_native_json_stringify));
    djs_obj_set(json_obj, "parse", djs_val_native_fn(djs_native_json_parse));
    djs_scope_set(g_djs_global_scope, "JSON", json_obj);

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

    g_djs_initialized = 1;
}

// ---------------------------------------------------------------------------
// Lexer & Recursive-Descent Evaluator
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

static inline char djs_advance(DJSLexer* lex) {
    djs_skip_whitespace(lex);
    if (lex->pos >= lex->len) return '\0';
    return lex->src[lex->pos++];
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
    lex->pos++; // skip open quote
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
                // Expression body: e.g. "a + b" until ';' or EOF or ')' or '\n'
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

        // Not an arrow function: clean up parsed param idents and backtrack
        for (int i = 0; i < pcount; i++) free(params[i]);
        lex->pos = save_pos;

        // Normal parenthesized expression
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

    // Identifiers or Keywords
    char* id = djs_parse_ident(lex);
    if (id) {
        if (strcmp(id, "true") == 0) { free(id); return djs_val_bool(true); }
        if (strcmp(id, "false") == 0) { free(id); return djs_val_bool(false); }
        if (strcmp(id, "null") == 0) { free(id); return djs_val_null(); }
        if (strcmp(id, "undefined") == 0) { free(id); return djs_val_undefined(); }

        // Require statement in JS: require("path")
        if (strcmp(id, "require") == 0) {
            free(id);
            if (djs_peek(lex) == '(') {
                lex->pos++;
                char* mod = djs_parse_string_literal(lex);
                if (djs_peek(lex) == ')') lex->pos++;
                if (mod) {
                    // Check built-in modules
                    if (strcmp(mod, "path") == 0) {
                        free(mod);
                        DJSVal* p = djs_val_obj();
                        return p;
                    }
                    if (strcmp(mod, "fs") == 0) {
                        free(mod);
                        DJSVal* f = djs_val_obj();
                        return f;
                    }
                    // Try reading local file
                    const char* file_code = datara_rt_file_read(mod);
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

        DJSVal* val = djs_scope_get(scope, id);
        free(id);
        return val;
    }

    return djs_val_undefined();
}

// Postfix: member access (obj.foo, obj["foo"]), function calls (fn(a, b))
static DJSVal* djs_parse_postfix(DJSLexer* lex, DJSScope* scope) {
    DJSVal* left = djs_parse_primary(lex, scope);

    while (lex->pos < lex->len) {
        djs_skip_whitespace(lex);
        char c = djs_peek(lex);

        if (c == '.') {
            lex->pos++;
            char* prop = djs_parse_ident(lex);
            if (prop) {
                left = djs_obj_get(left, prop);
                free(prop);
            }
        } else if (c == '[') {
            lex->pos++;
            DJSVal* idx = djs_parse_expr(lex, scope);
            if (djs_peek(lex) == ']') lex->pos++;
            char* key = djs_to_string(idx);
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

            if (left->type == DJS_NATIVE_FUNC && left->u.native_fn) {
                left = left->u.native_fn(NULL, argc, args);
            } else if (left->type == DJS_FUNC && left->u.fn.body) {
                DJSScope* fn_scope = djs_scope_new(left->u.fn.closure ? left->u.fn.closure : g_djs_global_scope);
                for (int i = 0; i < left->u.fn.param_count && i < argc; i++) {
                    djs_scope_set(fn_scope, left->u.fn.params[i], args[i]);
                }
                left = djs_eval_internal(fn_scope, left->u.fn.body);
            } else {
                left = djs_val_undefined();
            }
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
                djs_scope_set(scope, var_name, init_val);
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
                        djs_scope_set(scope, fn_name, fn_val);
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

        // Assignment: ident = expr;
        size_t save_pos = lex.pos;
        char* ident = djs_parse_ident(&lex);
        if (ident && djs_match(&lex, "=") && djs_peek(&lex) != '=') {
            DJSVal* val = djs_parse_expr(&lex, scope);
            djs_scope_set(scope, ident, val);
            free(ident);
            last_val = val;
            if (djs_peek(&lex) == ';') lex.pos++;
            continue;
        }
        if (ident) free(ident);
        lex.pos = save_pos; // backtrack

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

    // Check built-in node modules
    if (strcmp(module_name, "path") == 0 ||
        strcmp(module_name, "fs") == 0 ||
        strcmp(module_name, "os") == 0 ||
        strcmp(module_name, "crypto") == 0 ||
        strcmp(module_name, "util") == 0) {
        return 1;
    }

    // Try reading file directly or in node_modules
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
    if (!fn || (fn->type != DJS_FUNC && fn->type != DJS_NATIVE_FUNC)) {
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

    DJSVal* res = djs_val_undefined();
    if (fn->type == DJS_NATIVE_FUNC && fn->u.native_fn) {
        res = fn->u.native_fn(NULL, argc, argv);
    } else if (fn->type == DJS_FUNC && fn->u.fn.body) {
        DJSScope* s = djs_scope_new(fn->u.fn.closure ? fn->u.fn.closure : g_djs_global_scope);
        for (int i = 0; i < fn->u.fn.param_count && i < argc; i++) {
            djs_scope_set(s, fn->u.fn.params[i], argv[i]);
        }
        res = djs_eval_internal(s, fn->u.fn.body);
    }
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

#ifdef __cplusplus
}
#endif

#endif // DATARA_JS_H

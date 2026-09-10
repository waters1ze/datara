#include "datara_napi.h"
#include "datara_js.h"

#ifdef _WIN32
#include <windows.h>
#else
#include <dlfcn.h>
#endif

static char g_napi_last_err_buf[512] = {0};

// --- N-API Implementation Functions ---

static napi_status d_napi_create_string_utf8(napi_env env, const char* str, size_t length, napi_value* result) {
    if (!env || !str || !result) return napi_invalid_arg;
    DJSVal* s;
    if (length == NAPI_AUTO_LENGTH) {
        s = djs_val_str(str);
    } else {
        char* tmp = (char*)malloc(length + 1);
        if (!tmp) return napi_generic_failure;
        memcpy(tmp, str, length);
        tmp[length] = '\0';
        s = djs_val_str(tmp);
        free(tmp);
    }
    *result = (napi_value)s;
    return napi_ok;
}

static napi_status d_napi_get_value_string_utf8(napi_env env, napi_value value, char* buf, size_t bufsize, size_t* result) {
    if (!env || !value) return napi_invalid_arg;
    DJSVal* v = (DJSVal*)value;
    const char* s = djs_to_string(v);
    size_t len = strlen(s);
    if (buf && bufsize > 0) {
        size_t to_copy = len < bufsize - 1 ? len : bufsize - 1;
        memcpy(buf, s, to_copy);
        buf[to_copy] = '\0';
    }
    if (result) *result = len;
    return napi_ok;
}

static napi_status d_napi_create_int64(napi_env env, int64_t value, napi_value* result) {
    if (!env || !result) return napi_invalid_arg;
    *result = (napi_value)djs_val_int(value);
    return napi_ok;
}

static napi_status d_napi_get_value_int64(napi_env env, napi_value value, int64_t* result) {
    if (!env || !value || !result) return napi_invalid_arg;
    DJSVal* v = (DJSVal*)value;
    if (v->type == DJS_INT) *result = v->u.i;
    else if (v->type == DJS_FLOAT) *result = (int64_t)v->u.f;
    else if (v->type == DJS_BOOL) *result = v->u.b ? 1 : 0;
    else *result = 0;
    return napi_ok;
}

static napi_status d_napi_create_double(napi_env env, double value, napi_value* result) {
    if (!env || !result) return napi_invalid_arg;
    *result = (napi_value)djs_val_float(value);
    return napi_ok;
}

static napi_status d_napi_get_value_double(napi_env env, napi_value value, double* result) {
    if (!env || !value || !result) return napi_invalid_arg;
    DJSVal* v = (DJSVal*)value;
    if (v->type == DJS_FLOAT) *result = v->u.f;
    else if (v->type == DJS_INT) *result = (double)v->u.i;
    else if (v->type == DJS_BOOL) *result = v->u.b ? 1.0 : 0.0;
    else *result = 0.0;
    return napi_ok;
}

static napi_status d_napi_create_object(napi_env env, napi_value* result) {
    if (!env || !result) return napi_invalid_arg;
    *result = (napi_value)djs_val_obj();
    return napi_ok;
}

static napi_status d_napi_set_named_property(napi_env env, napi_value object, const char* utf8name, napi_value value) {
    if (!env || !object || !utf8name) return napi_invalid_arg;
    djs_obj_set((DJSVal*)object, utf8name, (DJSVal*)value);
    return napi_ok;
}

static napi_status d_napi_get_named_property(napi_env env, napi_value object, const char* utf8name, napi_value* result) {
    if (!env || !object || !utf8name || !result) return napi_invalid_arg;
    *result = (napi_value)djs_obj_get((DJSVal*)object, utf8name);
    return napi_ok;
}

static napi_status d_napi_create_function(napi_env env, const char* utf8name, size_t length, napi_callback cb, void* data, napi_value* result) {
    (void)utf8name; (void)length;
    if (!env || !cb || !result) return napi_invalid_arg;
    DJSVal* fn = djs_val_napi_fn((void*)cb, data, (void*)env);
    *result = (napi_value)fn;
    return napi_ok;
}

static napi_status d_napi_get_cb_info(napi_env env, napi_callback_info cbinfo, size_t* argc, napi_value* argv, napi_value* this_arg, void** data) {
    if (!env || !cbinfo) return napi_invalid_arg;
    struct napi_callback_info__* info = (struct napi_callback_info__*)cbinfo;

    if (argc && argv) {
        size_t copy_count = *argc < info->argc ? *argc : info->argc;
        for (size_t i = 0; i < copy_count; i++) {
            argv[i] = info->argv[i];
        }
        *argc = info->argc;
    } else if (argc) {
        *argc = info->argc;
    }

    if (this_arg) *this_arg = info->this_arg;
    if (data) *data = info->data;
    return napi_ok;
}

static napi_status d_napi_call_function(napi_env env, napi_value recv, napi_value func, size_t argc, const napi_value* argv, napi_value* result) {
    if (!env || !func) return napi_invalid_arg;
    DJSVal* fn = (DJSVal*)func;
    if (fn->type == DJS_NAPI_FUNC && fn->u.napi_fn.cb) {
        struct napi_callback_info__ info;
        info.argc = argc;
        info.argv = (napi_value*)argv;
        info.this_arg = recv;
        info.data = fn->u.napi_fn.data;
        napi_callback cb = (napi_callback)fn->u.napi_fn.cb;
        napi_env target_env = fn->u.napi_fn.env ? (napi_env)fn->u.napi_fn.env : env;
        napi_value r = cb(target_env, &info);
        if (result) *result = r;
        return napi_ok;
    } else if (fn->type == DJS_NATIVE_FUNC && fn->u.native_fn) {
        DJSVal* r = fn->u.native_fn((DJSVal*)recv, (int)argc, (DJSVal**)argv);
        if (result) *result = (napi_value)r;
        return napi_ok;
    }
    return napi_generic_failure;
}

static napi_status d_napi_create_buffer(napi_env env, size_t length, void** data, napi_value* result) {
    if (!env || !result) return napi_invalid_arg;
    DJSVal* b = djs_val_buffer_alloc(length);
    if (!b) return napi_generic_failure;
    if (data) *data = b->u.buffer.data;
    *result = (napi_value)b;
    return napi_ok;
}

static napi_status d_napi_get_buffer_info(napi_env env, napi_value value, void** data, size_t* length) {
    if (!env || !value) return napi_invalid_arg;
    DJSVal* b = (DJSVal*)value;
    if (b->type != DJS_BUFFER) return napi_invalid_arg;
    if (data) *data = b->u.buffer.data;
    if (length) *length = b->u.buffer.length;
    return napi_ok;
}

static napi_status d_napi_throw_error(napi_env env, const char* code, const char* msg) {
    if (msg) {
        snprintf(g_napi_last_err_buf, sizeof(g_napi_last_err_buf), "[%s] %s", code ? code : "Error", msg);
    }
    return napi_ok;
}

static napi_table g_napi_api_table = {
    d_napi_create_string_utf8,
    d_napi_get_value_string_utf8,
    d_napi_create_int64,
    d_napi_get_value_int64,
    d_napi_create_double,
    d_napi_get_value_double,
    d_napi_create_object,
    d_napi_set_named_property,
    d_napi_get_named_property,
    d_napi_create_function,
    d_napi_get_cb_info,
    d_napi_call_function,
    d_napi_create_buffer,
    d_napi_get_buffer_info,
    d_napi_throw_error
};

// Global canonical env
static struct napi_env__ g_canonical_napi_env = {
    &g_napi_api_table,
    NULL,
    NULL
};

DJSVal* datara_napi_load_addon(const char* path) {
    if (!path) return djs_val_null();

#ifdef _WIN32
    HMODULE hMod = LoadLibraryA(path);
    if (!hMod) {
        // Try prepending ./ or searching relative
        char alt_path[512];
        snprintf(alt_path, sizeof(alt_path), ".\\%s", path);
        hMod = LoadLibraryA(alt_path);
    }
    if (!hMod) {
        snprintf(g_napi_last_err_buf, sizeof(g_napi_last_err_buf), "Failed to load addon '%s': Win32 error %lu", path, GetLastError());
        return djs_val_null();
    }

    typedef napi_value (*NapiInitFn)(napi_env env, napi_value exports);
    NapiInitFn init_fn = (NapiInitFn)GetProcAddress(hMod, "napi_register_module_v1");
    if (!init_fn) {
        init_fn = (NapiInitFn)GetProcAddress(hMod, "node_api_module_get_config");
    }
    if (!init_fn) {
        snprintf(g_napi_last_err_buf, sizeof(g_napi_last_err_buf), "Addon '%s' does not export napi_register_module_v1", path);
        return djs_val_null();
    }

    djs_init_globals();
    g_canonical_napi_env.js_scope = g_djs_global_scope;

    DJSVal* exports = djs_val_obj();
    napi_value res = init_fn(&g_canonical_napi_env, (napi_value)exports);
    if (res) {
        return (DJSVal*)res;
    }
    return exports;

#else
    void* handle = dlopen(path, RTLD_LAZY | RTLD_LOCAL);
    if (!handle) {
        snprintf(g_napi_last_err_buf, sizeof(g_napi_last_err_buf), "Failed to load addon '%s': %s", path, dlerror());
        return djs_val_null();
    }

    typedef napi_value (*NapiInitFn)(napi_env env, napi_value exports);
    NapiInitFn init_fn = (NapiInitFn)dlsym(handle, "napi_register_module_v1");
    if (!init_fn) {
        snprintf(g_napi_last_err_buf, sizeof(g_napi_last_err_buf), "Addon '%s' missing napi_register_module_v1", path);
        return djs_val_null();
    }

    djs_init_globals();
    g_canonical_napi_env.js_scope = g_djs_global_scope;

    DJSVal* exports = djs_val_obj();
    napi_value res = init_fn(&g_canonical_napi_env, (napi_value)exports);
    if (res) return (DJSVal*)res;
    return exports;
#endif
}

int32_t datara_napi_is_available(void) {
    return 1;
}

const char* datara_napi_last_error(void) {
    return g_napi_last_err_buf;
}

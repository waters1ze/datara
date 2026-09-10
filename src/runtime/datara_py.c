#if defined(__APPLE__)
#ifndef _DARWIN_C_SOURCE
#define _DARWIN_C_SOURCE 1
#endif
#else
#ifndef _GNU_SOURCE
#define _GNU_SOURCE 1
#endif
#ifndef _DEFAULT_SOURCE
#define _DEFAULT_SOURCE 1
#endif
#ifndef _POSIX_C_SOURCE
#define _POSIX_C_SOURCE 200809L
#endif
#endif

#include <stdio.h>
#include <stdlib.h>
#include <stdint.h>
#include <string.h>

#ifdef _WIN32
#ifndef WIN32_LEAN_AND_MEAN
#define WIN32_LEAN_AND_MEAN
#endif
#include <windows.h>
#else
#include <dlfcn.h>
#include <pthread.h>
#include <unistd.h>
#endif

#include "datara_runtime.h"
#include "datara_py.h"

// ============================================================================
// CPython PEP 384 Stable ABI Definitions (No Python.h dependency)
// ============================================================================

typedef void PyObject;

typedef struct {
    void*   buf;
    void*   obj;
    int64_t len;
    int64_t itemsize;
    int32_t readonly;
    int32_t ndim;
    char*   format;
    int64_t* shape;
    int64_t* strides;
    int64_t* suboffsets;
    void*   internal;
} Py_buffer_view;

typedef struct {
    // Lifecycle & GIL
    void        (*Py_Initialize)(void);
    int32_t     (*Py_IsInitialized)(void);
    void        (*Py_Finalize)(void);
    int32_t     (*PyGILState_Ensure)(void);
    void        (*PyGILState_Release)(int32_t);

    // Reference Counting
    void        (*Py_IncRef)(PyObject*);
    void        (*Py_DecRef)(PyObject*);

    // Modules & Attributes
    PyObject*   (*PyImport_ImportModule)(const char*);
    PyObject*   (*PyObject_GetAttrString)(PyObject*, const char*);
    int32_t     (*PyObject_SetAttrString)(PyObject*, const char*, PyObject*);
    int32_t     (*PyObject_HasAttrString)(PyObject*, const char*);

    // Calling
    PyObject*   (*PyObject_Call)(PyObject*, PyObject*, PyObject*);
    PyObject*   (*PyObject_CallObject)(PyObject*, PyObject*);
    PyObject*   (*PyObject_CallFunctionObjArgs)(PyObject*, ...);

    // Primitive Types
    PyObject*   (*PyObject_Str)(PyObject*);
    PyObject*   (*PyObject_Repr)(PyObject*);
    const char* (*PyUnicode_AsUTF8AndSize)(PyObject*, int64_t*);
    PyObject*   (*PyUnicode_FromString)(const char*);
    PyObject*   (*PyUnicode_FromStringAndSize)(const char*, int64_t);
    PyObject*   (*PyLong_FromLongLong)(int64_t);
    int64_t     (*PyLong_AsLongLong)(PyObject*);
    PyObject*   (*PyFloat_FromDouble)(double);
    double      (*PyFloat_AsDouble)(PyObject*);
    PyObject*   (*PyBool_FromLong)(long);
    int32_t     (*PyObject_IsTrue)(PyObject*);

    // Collections
    PyObject*   (*PyDict_New)(void);
    PyObject*   (*PyDict_GetItemString)(PyObject*, const char*);
    int32_t     (*PyDict_SetItemString)(PyObject*, const char*, PyObject*);
    PyObject*   (*PyDict_Copy)(PyObject*);
    PyObject*   (*PyTuple_New)(int64_t);
    int32_t     (*PyTuple_SetItem)(PyObject*, int64_t, PyObject*);
    PyObject*   (*PyList_New)(int64_t);
    int64_t     (*PyList_Size)(PyObject*);
    PyObject*   (*PyList_GetItem)(PyObject*, int64_t);
    int32_t     (*PyList_SetItem)(PyObject*, int64_t, PyObject*);
    int32_t     (*PyList_Append)(PyObject*, PyObject*);

    // Buffer & Zero-Copy MemoryView
    PyObject*   (*PyMemoryView_FromMemory)(char*, int64_t, int32_t);
    int32_t     (*PyObject_GetBuffer)(PyObject*, void*, int32_t);
    void        (*PyBuffer_Release)(void*);

    // Diagnostics & Traceback
    PyObject*   (*PyErr_Occurred)(void);
    void        (*PyErr_Fetch)(PyObject**, PyObject**, PyObject**);
    void        (*PyErr_NormalizeException)(PyObject**, PyObject**, PyObject**);
    void        (*PyErr_Clear)(void);
} DataraPyShim;

static DataraPyShim g_py;
static int32_t      g_py_loaded = 0;
static void*        g_py_lib_handle = NULL;
static char         g_py_last_error[4096] = {0};

#ifdef _WIN32
static SRWLOCK      g_py_lock = SRWLOCK_INIT;
#define PY_LOCK()   AcquireSRWLockExclusive(&g_py_lock)
#define PY_UNLOCK() ReleaseSRWLockExclusive(&g_py_lock)
#else
static pthread_mutex_t g_py_lock = PTHREAD_MUTEX_INITIALIZER;
#define PY_LOCK()   pthread_mutex_lock(&g_py_lock)
#define PY_UNLOCK() pthread_mutex_unlock(&g_py_lock)
#endif

// ============================================================================
// Internal Library Probing & Dynamic Loading
// ============================================================================

static void* probe_and_load_library(void) {
    const char* py_env_dll = getenv("PYTHON_DLL");
    if (py_env_dll && py_env_dll[0]) {
        if (strcmp(py_env_dll, "0") == 0 || strcmp(py_env_dll, "none") == 0 || strcmp(py_env_dll, "disabled") == 0) {
            return NULL;
        }
#ifdef _WIN32
        return (void*)LoadLibraryA(py_env_dll);
#else
        return dlopen(py_env_dll, RTLD_NOW | RTLD_GLOBAL);
#endif
    }

#ifdef _WIN32
    // 1. Try already loaded module in current process
    HMODULE h = GetModuleHandleA("python3.dll");
    if (h) return (void*)h;

    // 2. Try direct LoadLibraryA with standard name
    h = LoadLibraryA("python3.dll");
    if (h) return (void*)h;

    // 3. Probe versioned DLL names (Python 3.14 down to 3.8)
    static const char* s_versioned_dlls[] = {
        "python314.dll", "python313.dll", "python312.dll",
        "python311.dll", "python310.dll", "python39.dll", "python38.dll"
    };
    for (size_t i = 0; i < sizeof(s_versioned_dlls)/sizeof(s_versioned_dlls[0]); i++) {
        h = LoadLibraryA(s_versioned_dlls[i]);
        if (h) return (void*)h;
    }

    // 4. Check standard Python installation directories
    static const char* s_standard_paths[] = {
        "C:\\Python314\\python3.dll", "C:\\Python314\\python314.dll",
        "C:\\Python313\\python3.dll", "C:\\Python313\\python313.dll",
        "C:\\Python312\\python3.dll", "C:\\Python312\\python312.dll",
        "C:\\Python311\\python3.dll", "C:\\Python311\\python311.dll",
        "C:\\Python310\\python3.dll", "C:\\Python310\\python310.dll"
    };
    for (size_t i = 0; i < sizeof(s_standard_paths)/sizeof(s_standard_paths[0]); i++) {
        h = LoadLibraryA(s_standard_paths[i]);
        if (h) return (void*)h;
    }

    // 6. Check LocalAppData Programs Python directory
    const char* local_app = getenv("LOCALAPPDATA");
    if (local_app) {
        char buf[512];
        static const char* s_local_subdirs[] = {
            "Python314", "Python313", "Python312", "Python311", "Python310"
        };
        for (size_t i = 0; i < sizeof(s_local_subdirs)/sizeof(s_local_subdirs[0]); i++) {
            snprintf(buf, sizeof(buf), "%s\\Programs\\Python\\%s\\python3.dll", local_app, s_local_subdirs[i]);
            h = LoadLibraryA(buf);
            if (h) return (void*)h;
            snprintf(buf, sizeof(buf), "%s\\Programs\\Python\\%s\\%s.dll", local_app, s_local_subdirs[i], s_local_subdirs[i]);
            h = LoadLibraryA(buf);
            if (h) return (void*)h;
        }
    }

    return NULL;
#else
    static const char* s_posix_libs[] = {
        // macOS Homebrew and Framework locations
        "/opt/homebrew/lib/libpython3.13.dylib",
        "/opt/homebrew/lib/libpython3.12.dylib",
        "/opt/homebrew/lib/libpython3.11.dylib",
        "/opt/homebrew/lib/libpython3.dylib",
        "/opt/homebrew/Frameworks/Python.framework/Versions/Current/Python",
        "/opt/homebrew/Frameworks/Python.framework/Versions/3.13/Python",
        "/opt/homebrew/Frameworks/Python.framework/Versions/3.12/Python",
        "/opt/homebrew/Frameworks/Python.framework/Versions/3.11/Python",
        "/Library/Frameworks/Python.framework/Versions/Current/Python",
        "/Library/Frameworks/Python.framework/Versions/3.13/Python",
        "/Library/Frameworks/Python.framework/Versions/3.12/Python",
        "/Library/Frameworks/Python.framework/Versions/3.11/Python",
        "/usr/local/lib/libpython3.dylib",
        "/usr/local/lib/libpython3.12.dylib",
        // POSIX / Linux shared libraries
        "libpython3.so",
        "libpython3.dylib",
        "libpython3.14.so", "libpython3.14.dylib",
        "libpython3.13.so", "libpython3.13.dylib",
        "libpython3.12.so", "libpython3.12.dylib",
        "libpython3.11.so", "libpython3.11.dylib",
        "libpython3.10.so", "libpython3.10.dylib",
        "libpython3.9.so",  "libpython3.9.dylib",
        "libpython3.8.so",  "libpython3.8.dylib"
    };
    for (size_t i = 0; i < sizeof(s_posix_libs)/sizeof(s_posix_libs[0]); i++) {
        void* h = dlopen(s_posix_libs[i], RTLD_NOW | RTLD_GLOBAL);
        if (h) return h;
    }
    // Dynamic query fallback: ask python3 CLI for its actual shared library path
    FILE* fp = popen("python3 -c \"import sysconfig, os; d = sysconfig.get_config_var('LIBDIR') or ''; l = sysconfig.get_config_var('LDLIBRARY') or ''; print(os.path.join(d, l) if d and l else '')\" 2>/dev/null", "r");
    if (fp) {
        char py_path[512];
        if (fgets(py_path, sizeof(py_path), fp)) {
            char* nl = strchr(py_path, '\n');
            if (nl) *nl = '\0';
            char* cr = strchr(py_path, '\r');
            if (cr) *cr = '\0';
            if (py_path[0] != '\0') {
                void* h = dlopen(py_path, RTLD_NOW | RTLD_GLOBAL);
                pclose(fp);
                if (h) return h;
            }
        }
        pclose(fp);
    }
    return NULL;
#endif
}

static void* get_proc_address(void* lib, const char* name) {
#ifdef _WIN32
    return (void*)GetProcAddress((HMODULE)lib, name);
#else
    return dlsym(lib, name);
#endif
}

// ============================================================================
// Traceback & Error Capture
// ============================================================================

static void capture_current_python_error(void) {
    if (!g_py.PyErr_Occurred) return;
    if (!g_py.PyErr_Occurred()) {
        g_py_last_error[0] = '\0';
        return;
    }

    PyObject *ptype = NULL, *pval = NULL, *ptb = NULL;
    g_py.PyErr_Fetch(&ptype, &pval, &ptb);
    if (ptype && g_py.PyErr_NormalizeException) {
        g_py.PyErr_NormalizeException(&ptype, &pval, &ptb);
    }

    g_py_last_error[0] = '\0';

    // Try formatting through Python's traceback module
    if (ptype && g_py.PyImport_ImportModule) {
        PyObject* tb_mod = g_py.PyImport_ImportModule("traceback");
        if (tb_mod) {
            PyObject* fmt_fn = g_py.PyObject_GetAttrString(tb_mod, "format_exception");
            if (fmt_fn && g_py.PyObject_CallFunctionObjArgs) {
                PyObject* tb_list = g_py.PyObject_CallFunctionObjArgs(fmt_fn, ptype, pval, ptb, NULL);
                if (tb_list && g_py.PyList_Size && g_py.PyList_GetItem) {
                    int64_t count = g_py.PyList_Size(tb_list);
                    size_t offset = 0;
                    for (int64_t i = 0; i < count && offset < sizeof(g_py_last_error) - 1; i++) {
                        PyObject* item = g_py.PyList_GetItem(tb_list, i);
                        if (item && g_py.PyUnicode_AsUTF8AndSize) {
                            int64_t item_sz = 0;
                            const char* s = g_py.PyUnicode_AsUTF8AndSize(item, &item_sz);
                            if (s && item_sz > 0) {
                                size_t to_copy = (size_t)item_sz;
                                if (offset + to_copy >= sizeof(g_py_last_error) - 1) {
                                    to_copy = sizeof(g_py_last_error) - 1 - offset;
                                }
                                memcpy(g_py_last_error + offset, s, to_copy);
                                offset += to_copy;
                                g_py_last_error[offset] = '\0';
                            }
                        }
                    }
                    g_py.Py_DecRef(tb_list);
                }
                g_py.Py_DecRef(fmt_fn);
            }
            g_py.Py_DecRef(tb_mod);
        }
    }

    // Fallback: format exception value directly if traceback failed
    if (g_py_last_error[0] == '\0' && pval && g_py.PyObject_Str) {
        PyObject* str_val = g_py.PyObject_Str(pval);
        if (str_val && g_py.PyUnicode_AsUTF8AndSize) {
            int64_t sz = 0;
            const char* s = g_py.PyUnicode_AsUTF8AndSize(str_val, &sz);
            if (s) {
                snprintf(g_py_last_error, sizeof(g_py_last_error), "Python Error: %s", s);
            }
            g_py.Py_DecRef(str_val);
        }
    }

    if (g_py_last_error[0] == '\0') {
        snprintf(g_py_last_error, sizeof(g_py_last_error), "Python Error: Unknown runtime exception occurred");
    }

    if (ptype) g_py.Py_DecRef(ptype);
    if (pval) g_py.Py_DecRef(pval);
    if (ptb) g_py.Py_DecRef(ptb);
    g_py.PyErr_Clear();
}

// ============================================================================
// Public Interface Implementation
// ============================================================================

int32_t datara_py_init(void) {
    PY_LOCK();
    if (g_py_loaded) {
        PY_UNLOCK();
        return 0;
    }

    void* lib = probe_and_load_library();
    if (!lib) {
        snprintf(g_py_last_error, sizeof(g_py_last_error),
            "Failed to load CPython library (python3.dll / libpython3.so). Ensure Python 3.8+ is installed (download from https://www.python.org or run 'winget install Python.Python.3.12') and python3.dll is in PATH.");
        PY_UNLOCK();
        return -1;
    }
    g_py_lib_handle = lib;

    #define BIND_SYM(name) do { \
        g_py.name = get_proc_address(lib, #name); \
        if (!g_py.name) { \
            snprintf(g_py_last_error, sizeof(g_py_last_error), "Missing essential CPython symbol: %s", #name); \
            PY_UNLOCK(); \
            return -2; \
        } \
    } while (0)

    #define BIND_OPT(name) do { \
        g_py.name = get_proc_address(lib, #name); \
    } while (0)

    // Essential Lifecycle & GIL
    BIND_SYM(Py_Initialize);
    BIND_SYM(Py_IsInitialized);
    BIND_OPT(Py_Finalize);
    BIND_SYM(PyGILState_Ensure);
    BIND_SYM(PyGILState_Release);

    // Reference Counting
    BIND_SYM(Py_IncRef);
    BIND_SYM(Py_DecRef);

    // Module & Object
    BIND_SYM(PyImport_ImportModule);
    BIND_SYM(PyObject_GetAttrString);
    BIND_OPT(PyObject_SetAttrString);
    BIND_OPT(PyObject_HasAttrString);
    BIND_SYM(PyObject_Call);
    BIND_SYM(PyObject_CallObject);
    BIND_SYM(PyObject_CallFunctionObjArgs);

    // Primitives
    BIND_SYM(PyObject_Str);
    BIND_OPT(PyObject_Repr);
    BIND_SYM(PyUnicode_AsUTF8AndSize);
    BIND_SYM(PyUnicode_FromString);
    BIND_OPT(PyUnicode_FromStringAndSize);
    BIND_SYM(PyLong_FromLongLong);
    BIND_SYM(PyLong_AsLongLong);
    BIND_SYM(PyFloat_FromDouble);
    BIND_SYM(PyFloat_AsDouble);
    BIND_OPT(PyBool_FromLong);
    BIND_OPT(PyObject_IsTrue);

    // Collections
    BIND_SYM(PyDict_New);
    BIND_SYM(PyDict_GetItemString);
    BIND_SYM(PyDict_SetItemString);
    BIND_OPT(PyDict_Copy);
    BIND_SYM(PyTuple_New);
    BIND_SYM(PyTuple_SetItem);
    BIND_SYM(PyList_New);
    BIND_SYM(PyList_Size);
    BIND_SYM(PyList_GetItem);
    BIND_SYM(PyList_SetItem);
    BIND_SYM(PyList_Append);

    // Buffer & MemoryView
    BIND_SYM(PyMemoryView_FromMemory);
    BIND_SYM(PyObject_GetBuffer);
    BIND_SYM(PyBuffer_Release);

    // Diagnostics
    BIND_SYM(PyErr_Occurred);
    BIND_SYM(PyErr_Fetch);
    BIND_SYM(PyErr_NormalizeException);
    BIND_SYM(PyErr_Clear);

    #undef BIND_SYM
    #undef BIND_OPT

    // Initialize Python runtime if not already initialized
    if (!g_py.Py_IsInitialized()) {
        g_py.Py_Initialize();
    }

    g_py_loaded = 1;
    g_py_last_error[0] = '\0';
    PY_UNLOCK();
    return 0;
}

int32_t datara_py_is_available(void) {
    if (!g_py_loaded) {
        if (datara_py_init() != 0) {
            return 0;
        }
    }
    return g_py_loaded;
}

const char* datara_py_last_error(void) {
    return g_py_last_error;
}

void datara_py_clear_error(void) {
    g_py_last_error[0] = '\0';
}

static void py_handle_destructor(void* ptr) {
    if (!ptr || !g_py_loaded) return;
    int32_t gstate = g_py.PyGILState_Ensure();
    g_py.Py_DecRef((PyObject*)ptr);
    g_py.PyGILState_Release(gstate);
}

int64_t datara_py_exec(const char* code) {
    if (!datara_py_is_available()) return -1;
    if (!code) return 0;

    int32_t gstate = g_py.PyGILState_Ensure();

    PyObject* builtins = g_py.PyImport_ImportModule("builtins");
    if (!builtins) {
        capture_current_python_error();
        g_py.PyGILState_Release(gstate);
        return -1;
    }

    PyObject* exec_fn = g_py.PyObject_GetAttrString(builtins, "exec");
    PyObject* main_mod = g_py.PyImport_ImportModule("__main__");
    PyObject* main_dict = main_mod ? g_py.PyObject_GetAttrString(main_mod, "__dict__") : NULL;
    PyObject* code_str = g_py.PyUnicode_FromString(code);

    int64_t status = 0;
    if (exec_fn && main_dict && code_str) {
        PyObject* res = g_py.PyObject_CallFunctionObjArgs(exec_fn, code_str, main_dict, main_dict, NULL);
        if (!res || g_py.PyErr_Occurred()) {
            capture_current_python_error();
            status = -1;
        } else {
            g_py.Py_DecRef(res);
            g_py_last_error[0] = '\0';
        }
    } else {
        capture_current_python_error();
        status = -1;
    }

    if (code_str) g_py.Py_DecRef(code_str);
    if (main_dict) g_py.Py_DecRef(main_dict);
    if (main_mod) g_py.Py_DecRef(main_mod);
    if (exec_fn) g_py.Py_DecRef(exec_fn);
    if (builtins) g_py.Py_DecRef(builtins);

    g_py.PyGILState_Release(gstate);
    return status;
}

const char* datara_py_eval(const char* code) {
    if (!datara_py_is_available()) return "";
    if (!code) return "";

    int32_t gstate = g_py.PyGILState_Ensure();

    PyObject* builtins = g_py.PyImport_ImportModule("builtins");
    if (!builtins) {
        capture_current_python_error();
        g_py.PyGILState_Release(gstate);
        return "";
    }

    PyObject* eval_fn = g_py.PyObject_GetAttrString(builtins, "eval");
    PyObject* main_mod = g_py.PyImport_ImportModule("__main__");
    PyObject* main_dict = main_mod ? g_py.PyObject_GetAttrString(main_mod, "__dict__") : NULL;
    PyObject* code_str = g_py.PyUnicode_FromString(code);

    char* ret_buf = "";

    if (eval_fn && main_dict && code_str) {
        PyObject* res = g_py.PyObject_CallFunctionObjArgs(eval_fn, code_str, main_dict, main_dict, NULL);
        if (!res || g_py.PyErr_Occurred()) {
            capture_current_python_error();
        } else {
            PyObject* s_obj = g_py.PyObject_Str(res);
            if (s_obj) {
                int64_t len = 0;
                const char* utf8 = g_py.PyUnicode_AsUTF8AndSize(s_obj, &len);
                if (utf8) {
                    ret_buf = (char*)datara_rt_arena_alloc(len + 1);
                    if (ret_buf) {
                        memcpy(ret_buf, utf8, len);
                        ret_buf[len] = '\0';
                    } else {
                        ret_buf = "";
                    }
                }
                g_py.Py_DecRef(s_obj);
            }
            g_py.Py_DecRef(res);
            g_py_last_error[0] = '\0';
        }
    } else {
        capture_current_python_error();
    }

    if (code_str) g_py.Py_DecRef(code_str);
    if (main_dict) g_py.Py_DecRef(main_dict);
    if (main_mod) g_py.Py_DecRef(main_mod);
    if (eval_fn) g_py.Py_DecRef(eval_fn);
    if (builtins) g_py.Py_DecRef(builtins);

    g_py.PyGILState_Release(gstate);
    return ret_buf;
}

const char* datara_py_eval_safe(const char* code) {
    return datara_py_eval(code);
}

int64_t datara_py_eval_int(const char* code) {
    if (!datara_py_is_available()) return 0;
    if (!code) return 0;

    int32_t gstate = g_py.PyGILState_Ensure();

    PyObject* builtins = g_py.PyImport_ImportModule("builtins");
    if (!builtins) {
        capture_current_python_error();
        g_py.PyGILState_Release(gstate);
        return 0;
    }

    PyObject* eval_fn = g_py.PyObject_GetAttrString(builtins, "eval");
    PyObject* main_mod = g_py.PyImport_ImportModule("__main__");
    PyObject* main_dict = main_mod ? g_py.PyObject_GetAttrString(main_mod, "__dict__") : NULL;
    PyObject* code_str = g_py.PyUnicode_FromString(code);

    int64_t ret_val = 0;

    if (eval_fn && main_dict && code_str) {
        PyObject* res = g_py.PyObject_CallFunctionObjArgs(eval_fn, code_str, main_dict, main_dict, NULL);
        if (!res || g_py.PyErr_Occurred()) {
            capture_current_python_error();
        } else {
            ret_val = g_py.PyLong_AsLongLong(res);
            if (g_py.PyErr_Occurred()) {
                capture_current_python_error();
                ret_val = 0;
            } else {
                g_py_last_error[0] = '\0';
            }
            g_py.Py_DecRef(res);
        }
    } else {
        capture_current_python_error();
    }

    if (code_str) g_py.Py_DecRef(code_str);
    if (main_dict) g_py.Py_DecRef(main_dict);
    if (main_mod) g_py.Py_DecRef(main_mod);
    if (eval_fn) g_py.Py_DecRef(eval_fn);
    if (builtins) g_py.Py_DecRef(builtins);

    g_py.PyGILState_Release(gstate);
    return ret_val;
}

double datara_py_eval_float(const char* code) {
    if (!datara_py_is_available()) return 0.0;
    if (!code) return 0.0;

    int32_t gstate = g_py.PyGILState_Ensure();

    PyObject* builtins = g_py.PyImport_ImportModule("builtins");
    if (!builtins) {
        capture_current_python_error();
        g_py.PyGILState_Release(gstate);
        return 0.0;
    }

    PyObject* eval_fn = g_py.PyObject_GetAttrString(builtins, "eval");
    PyObject* main_mod = g_py.PyImport_ImportModule("__main__");
    PyObject* main_dict = main_mod ? g_py.PyObject_GetAttrString(main_mod, "__dict__") : NULL;
    PyObject* code_str = g_py.PyUnicode_FromString(code);

    double ret_val = 0.0;

    if (eval_fn && main_dict && code_str) {
        PyObject* res = g_py.PyObject_CallFunctionObjArgs(eval_fn, code_str, main_dict, main_dict, NULL);
        if (!res || g_py.PyErr_Occurred()) {
            capture_current_python_error();
        } else {
            ret_val = g_py.PyFloat_AsDouble(res);
            if (g_py.PyErr_Occurred()) {
                capture_current_python_error();
                ret_val = 0.0;
            } else {
                g_py_last_error[0] = '\0';
            }
            g_py.Py_DecRef(res);
        }
    } else {
        capture_current_python_error();
    }

    if (code_str) g_py.Py_DecRef(code_str);
    if (main_dict) g_py.Py_DecRef(main_dict);
    if (main_mod) g_py.Py_DecRef(main_mod);
    if (eval_fn) g_py.Py_DecRef(eval_fn);
    if (builtins) g_py.Py_DecRef(builtins);

    g_py.PyGILState_Release(gstate);
    return ret_val;
}

int64_t datara_py_import(const char* module_name) {
    if (!datara_py_is_available()) return 0;
    if (!module_name) return 0;

    int32_t gstate = g_py.PyGILState_Ensure();

    PyObject* mod = g_py.PyImport_ImportModule(module_name);
    if (!mod || g_py.PyErr_Occurred()) {
        capture_current_python_error();
        g_py.PyGILState_Release(gstate);
        return 0;
    }

    g_py_last_error[0] = '\0';
    uint32_t handle = datara_rt_handle_alloc(mod, "PyObject", py_handle_destructor);
    g_py.PyGILState_Release(gstate);
    return (int64_t)handle;
}

const char* datara_py_call(const char* fn_name, const char* args_json) {
    if (!datara_py_is_available()) return "";
    if (!fn_name) return "";

    int32_t gstate = g_py.PyGILState_Ensure();

    PyObject* target_fn = NULL;
    const char* dot = strchr(fn_name, '.');
    if (dot) {
        size_t mod_len = (size_t)(dot - fn_name);
        char mod_name[256];
        if (mod_len < sizeof(mod_name)) {
            memcpy(mod_name, fn_name, mod_len);
            mod_name[mod_len] = '\0';
            PyObject* mod = g_py.PyImport_ImportModule(mod_name);
            if (mod) {
                target_fn = g_py.PyObject_GetAttrString(mod, dot + 1);
                g_py.Py_DecRef(mod);
            }
        }
    } else {
        PyObject* main_mod = g_py.PyImport_ImportModule("__main__");
        if (main_mod) {
            target_fn = g_py.PyObject_GetAttrString(main_mod, fn_name);
            g_py.Py_DecRef(main_mod);
        }
        if (!target_fn) {
            PyObject* builtins = g_py.PyImport_ImportModule("builtins");
            if (builtins) {
                target_fn = g_py.PyObject_GetAttrString(builtins, fn_name);
                g_py.Py_DecRef(builtins);
            }
        }
    }

    if (!target_fn) {
        capture_current_python_error();
        if (g_py_last_error[0] == '\0') {
            snprintf(g_py_last_error, sizeof(g_py_last_error), "Python function '%s' not found", fn_name);
        }
        g_py.PyGILState_Release(gstate);
        return "";
    }

    PyObject* args_tuple = NULL;
    if (args_json && args_json[0] == '[') {
        PyObject* json_mod = g_py.PyImport_ImportModule("json");
        if (json_mod) {
            PyObject* loads_fn = g_py.PyObject_GetAttrString(json_mod, "loads");
            PyObject* json_str = g_py.PyUnicode_FromString(args_json);
            if (loads_fn && json_str) {
                PyObject* py_list = g_py.PyObject_CallFunctionObjArgs(loads_fn, json_str, NULL);
                if (py_list && g_py.PyList_Size) {
                    int64_t count = g_py.PyList_Size(py_list);
                    args_tuple = g_py.PyTuple_New(count);
                    for (int64_t i = 0; i < count; i++) {
                        PyObject* elem = g_py.PyList_GetItem(py_list, i);
                        if (elem) {
                            g_py.Py_IncRef(elem);
                            g_py.PyTuple_SetItem(args_tuple, i, elem);
                        }
                    }
                    g_py.Py_DecRef(py_list);
                }
            }
            if (json_str) g_py.Py_DecRef(json_str);
            if (loads_fn) g_py.Py_DecRef(loads_fn);
            g_py.Py_DecRef(json_mod);
        }
    }

    if (!args_tuple) {
        args_tuple = g_py.PyTuple_New(0);
    }

    char* ret_buf = "";
    PyObject* call_res = g_py.PyObject_Call(target_fn, args_tuple, NULL);
    if (!call_res || g_py.PyErr_Occurred()) {
        capture_current_python_error();
    } else {
        PyObject* s = g_py.PyObject_Str(call_res);
        if (s) {
            int64_t len = 0;
            const char* utf8 = g_py.PyUnicode_AsUTF8AndSize(s, &len);
            if (utf8) {
                ret_buf = (char*)datara_rt_arena_alloc(len + 1);
                if (ret_buf) {
                    memcpy(ret_buf, utf8, len);
                    ret_buf[len] = '\0';
                }
            }
            g_py.Py_DecRef(s);
        }
        g_py.Py_DecRef(call_res);
        g_py_last_error[0] = '\0';
    }

    if (args_tuple) g_py.Py_DecRef(args_tuple);
    g_py.Py_DecRef(target_fn);

    g_py.PyGILState_Release(gstate);
    return ret_buf;
}

const char* datara_py_call_1_str(const char* fn_name, const char* arg0) {
    char args_buf[1024];
    // Form simple JSON array [ "arg0" ]
    snprintf(args_buf, sizeof(args_buf), "[\"%s\"]", arg0 ? arg0 : "");
    return datara_py_call(fn_name, args_buf);
}

double datara_py_call_1_float(const char* fn_name, double arg0) {
    char args_buf[128];
    snprintf(args_buf, sizeof(args_buf), "[%f]", arg0);
    const char* s = datara_py_call(fn_name, args_buf);
    return s && s[0] ? atof(s) : 0.0;
}

// ============================================================================
// Zero-Copy DataraMemoryView Interop (Requirement 7)
// ============================================================================

int32_t datara_py_export_memview(const char* var_name, const DataraMemoryView* view) {
    if (!datara_py_is_available()) return -1;
    if (!var_name || !view || !view->data || view->total_bytes <= 0) return -1;

    int32_t gstate = g_py.PyGILState_Ensure();

    // 1. Create a zero-copy PyMemoryView directly referencing the Datara buffer
    // PyBUF_WRITE = 0x200 allows read/write access
    PyObject* raw_mv = g_py.PyMemoryView_FromMemory((char*)view->data, (int64_t)view->total_bytes, 0x200);
    if (!raw_mv || g_py.PyErr_Occurred()) {
        capture_current_python_error();
        g_py.PyGILState_Release(gstate);
        return -1;
    }

    PyObject* main_mod = g_py.PyImport_ImportModule("__main__");
    if (!main_mod) {
        capture_current_python_error();
        g_py.Py_DecRef(raw_mv);
        g_py.PyGILState_Release(gstate);
        return -1;
    }

    PyObject* main_dict = g_py.PyObject_GetAttrString(main_mod, "__dict__");
    if (!main_dict) {
        capture_current_python_error();
        g_py.Py_DecRef(main_mod);
        g_py.Py_DecRef(raw_mv);
        g_py.PyGILState_Release(gstate);
        return -1;
    }

    // Temporarily bind the raw memoryview
    g_py.PyDict_SetItemString(main_dict, "__dt_mv_temp__", raw_mv);

    // Format specifier for cast
    const char* fmt = "B";
    switch (view->element_type) {
        case DATARA_DTYPE_FLOAT64: fmt = "d"; break;
        case DATARA_DTYPE_FLOAT32: fmt = "f"; break;
        case DATARA_DTYPE_INT64:   fmt = "q"; break;
        case DATARA_DTYPE_INT32:   fmt = "i"; break;
        case DATARA_DTYPE_INT16:   fmt = "h"; break;
        case DATARA_DTYPE_UINT8:   fmt = "B"; break;
        case DATARA_DTYPE_INT8:    fmt = "b"; break;
        default:                   fmt = "B"; break;
    }

    // Cast memoryview to typed element array, and if numpy is available, wrap zero-copy
    char py_code[512];
    snprintf(py_code, sizeof(py_code),
        "try:\n"
        "    import numpy as _np\n"
        "    %s = _np.asarray(__dt_mv_temp__.cast('%s'))\n"
        "except Exception:\n"
        "    %s = __dt_mv_temp__.cast('%s')\n"
        "del __dt_mv_temp__\n",
        var_name, fmt, var_name, fmt);

    PyObject* builtins = g_py.PyImport_ImportModule("builtins");
    if (builtins) {
        PyObject* exec_fn = g_py.PyObject_GetAttrString(builtins, "exec");
        PyObject* code_str = g_py.PyUnicode_FromString(py_code);
        if (exec_fn && code_str) {
            PyObject* exec_res = g_py.PyObject_CallFunctionObjArgs(exec_fn, code_str, main_dict, main_dict, NULL);
            if (exec_res) {
                g_py.Py_DecRef(exec_res);
            } else {
                capture_current_python_error();
            }
        }
        if (code_str) g_py.Py_DecRef(code_str);
        if (exec_fn) g_py.Py_DecRef(exec_fn);
        g_py.Py_DecRef(builtins);
    }

    g_py.Py_DecRef(main_dict);
    g_py.Py_DecRef(main_mod);
    g_py.Py_DecRef(raw_mv);

    g_py.PyGILState_Release(gstate);
    return 0;
}

int64_t datara_py_export_list_f64(const char* var_name, int64_t* list) {
    if (!list) return -1;
    DataraMemoryView mv = datara_memview_from_list_f64(list);
    return (int64_t)datara_py_export_memview(var_name, &mv);
}

int64_t datara_py_assert_same_ptr(const char* var_name, int64_t* list) {
    if (!datara_py_is_available() || !var_name || !list) return 0;

    int32_t gstate = g_py.PyGILState_Ensure();

    PyObject* main_mod = g_py.PyImport_ImportModule("__main__");
    if (!main_mod) {
        g_py.PyGILState_Release(gstate);
        return 0;
    }

    PyObject* main_dict = g_py.PyObject_GetAttrString(main_mod, "__dict__");
    if (!main_dict) {
        g_py.Py_DecRef(main_mod);
        g_py.PyGILState_Release(gstate);
        return 0;
    }

    PyObject* obj = g_py.PyDict_GetItemString(main_dict, var_name);
    if (!obj) {
        g_py.Py_DecRef(main_dict);
        g_py.Py_DecRef(main_mod);
        g_py.PyGILState_Release(gstate);
        return 0;
    }

    Py_buffer_view view;
    memset(&view, 0, sizeof(view));
    int32_t res = g_py.PyObject_GetBuffer(obj, &view, 1 /* PyBUF_WRITABLE */);
    if (res != 0) {
        if (g_py.PyErr_Occurred()) g_py.PyErr_Clear();
        // Retry with basic PyBUF_SIMPLE = 0
        res = g_py.PyObject_GetBuffer(obj, &view, 0);
    }

    int64_t matches = 0;
    void* expected_data = (void*)&list[1];
    if (res == 0) {
        if (view.buf == expected_data) {
            matches = 1;
        }
        g_py.PyBuffer_Release(&view);
    }

    g_py.Py_DecRef(main_dict);
    g_py.Py_DecRef(main_mod);
    g_py.PyGILState_Release(gstate);
    return matches;
}

int32_t datara_py_test_zerocopy(void) {
    if (!datara_py_is_available()) return 0;

    double test_buf[4] = { 10.0, 20.0, 30.0, 40.0 };
    DataraMemoryView mv = datara_memview_1d(test_buf, 4, DATARA_DTYPE_FLOAT64);

    if (datara_py_export_memview("__zc_test__", &mv) != 0) {
        return 0;
    }

    // Verify pointer match in Python
    int32_t gstate = g_py.PyGILState_Ensure();
    PyObject* main_mod = g_py.PyImport_ImportModule("__main__");
    PyObject* main_dict = main_mod ? g_py.PyObject_GetAttrString(main_mod, "__dict__") : NULL;
    PyObject* py_obj = main_dict ? g_py.PyDict_GetItemString(main_dict, "__zc_test__") : NULL;

    Py_buffer_view buf_info;
    memset(&buf_info, 0, sizeof(buf_info));
    int32_t buf_res = py_obj ? g_py.PyObject_GetBuffer(py_obj, &buf_info, 1 /* PyBUF_WRITABLE */) : -1;
    if (buf_res != 0 && py_obj) {
        if (g_py.PyErr_Occurred()) g_py.PyErr_Clear();
        buf_res = g_py.PyObject_GetBuffer(py_obj, &buf_info, 0 /* PyBUF_SIMPLE */);
    }
    int32_t ptr_matched = (buf_res == 0 && buf_info.buf == (void*)test_buf);
    if (buf_res == 0) {
        g_py.PyBuffer_Release(&buf_info);
    }
    if (main_dict) g_py.Py_DecRef(main_dict);
    if (main_mod) g_py.Py_DecRef(main_mod);
    g_py.PyGILState_Release(gstate);

    if (!ptr_matched) {
        return 0;
    }

    // Mutate via Python execution
    datara_py_exec("__zc_test__[0] = 777.5\n__zc_test__[3] = 999.5\n");

    // Assert that the native C buffer observed the in-place mutation with zero copies
    if (test_buf[0] != 777.5 || test_buf[3] != 999.5) {
        return 0;
    }

    return 1;
}

// ============================================================================
// Self-Test Harness
// ============================================================================

int32_t datara_py_self_test(void) {
    if (datara_py_init() != 0) {
        return 101; // Failed to load/bind CPython
    }

    // 1. Basic Arithmetic Eval
    int64_t i_val = datara_py_eval_int("20 + 22");
    if (i_val != 42) return 102;

    double f_val = datara_py_eval_float("1.5 * 3.0");
    if (f_val < 4.49 || f_val > 4.51) return 103;

    const char* s_val = datara_py_eval("'hello ' + 'datara'");
    if (!s_val || strstr(s_val, "hello datara") == NULL) return 104;

    // 2. Python Function Call
    const char* sqrt_val = datara_py_call("math.sqrt", "[25.0]");
    if (!sqrt_val || atof(sqrt_val) != 5.0) return 105;

    // 3. Traceback & Error Capture
    const char* bad_val = datara_py_eval("10 / 0");
    if (bad_val && bad_val[0] != '\0') return 106; // Must return empty on error
    const char* err = datara_py_last_error();
    if (!err || strstr(err, "ZeroDivisionError") == NULL) return 107;
    datara_py_clear_error();

    // 4. Zero-Copy In-Place Buffer Mutation
    if (!datara_py_test_zerocopy()) return 108;

    return 0; // All self-tests passed
}

// ============================================================================
// Short `py_*` Aliases
// ============================================================================

int32_t     py_is_available(void) { return datara_py_is_available(); }
const char* py_last_error(void) { return datara_py_last_error(); }
void        py_clear_error(void) { datara_py_clear_error(); }
const char* py_eval(const char* code) { return datara_py_eval(code); }
const char* py_eval_safe(const char* code) { return datara_py_eval_safe(code); }
int64_t     py_eval_int(const char* code) { return datara_py_eval_int(code); }
double      py_eval_float(const char* code) { return datara_py_eval_float(code); }
int64_t     py_exec(const char* code) { return datara_py_exec(code); }
int64_t     py_import(const char* module_name) { return datara_py_import(module_name); }
const char* py_call(const char* fn_name, const char* args_json) { return datara_py_call(fn_name, args_json); }
const char* py_call_1_str(const char* fn_name, const char* arg0) { return datara_py_call_1_str(fn_name, arg0); }
double      py_call_1_float(const char* fn_name, double arg0) { return datara_py_call_1_float(fn_name, arg0); }


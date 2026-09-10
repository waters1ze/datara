#if defined(__APPLE__)
#ifndef _DARWIN_C_SOURCE
#define _DARWIN_C_SOURCE 1
#endif
#else
#ifndef _GNU_SOURCE
#define _GNU_SOURCE
#endif
#ifndef _DEFAULT_SOURCE
#define _DEFAULT_SOURCE
#endif
#ifndef _POSIX_C_SOURCE
#define _POSIX_C_SOURCE 200809L
#endif
#endif

#include <stdio.h>
#include <stdlib.h>
#include <stdint.h>
#include <string.h>
#include <math.h>
#include "datara_runtime.h"

#ifdef _WIN32
#ifndef WIN32_LEAN_AND_MEAN
#define WIN32_LEAN_AND_MEAN
#endif
#include <windows.h>
#include <winsock2.h>
#include <ws2tcpip.h>
#include <dbghelp.h>
#else
#include <sys/types.h>
#include <sys/socket.h>
#include <netinet/in.h>
#include <arpa/inet.h>
#include <netdb.h>
#include <unistd.h>
#include <fcntl.h>
#include <time.h>
#include <pthread.h>
#if defined(__APPLE__)
#include <sys/sysctl.h>
#endif
#if defined(__has_include)
#if __has_include(<execinfo.h>)
#include <execinfo.h>
#define DATARA_HAS_EXECINFO 1
#endif
#elif !defined(__musl__)
#include <execinfo.h>
#define DATARA_HAS_EXECINFO 1
#endif
#endif

#ifndef INADDR_NONE
#define INADDR_NONE ((unsigned long)0xffffffff)
#endif

uint32_t datara_rt_abi_version(void) {
    return DATARA_RT_ABI_VERSION;
}

void datara_rt_out_int(int64_t v) {
    datara_rt_print_int(v);
    datara_rt_print_newline();
}

void datara_rt_out_bool(int64_t v) {
    datara_rt_print_bool(v);
    datara_rt_print_newline();
}

// Returns a pointer to a static literal; never freed. The runtime's string
// concat also never frees its inputs, so this is safe in the same way.
const char* datara_rt_bool_to_str(int64_t v) {
    return v ? "true" : "false";
}

void datara_rt_out_float(double v) {
    datara_rt_print_float(v);
    datara_rt_print_newline();
}

const char* datara_rt_float_to_str(double v) {
    if (isnan(v)) {
        return signbit(v) ? "-NaN" : "NaN";
    }
    if (isinf(v)) {
        return v < 0 ? "-Infinity" : "Infinity";
    }
    char* buf = (char*)malloc(64);
    if (!buf) return "";
    if (v == floor(v) && fabs(v) < 1e15) {
        snprintf(buf, 64, "%.0f", v);
        return buf;
    }
    for (int prec = 4; prec < 17; prec++) {
        snprintf(buf, 64, "%.*g", prec, v);
        if (strtod(buf, NULL) == v) {
            return buf;
        }
    }
    snprintf(buf, 64, "%.17g", v);
    return buf;
}

void datara_rt_out_str(const char* s) {
    datara_rt_print_str(s != NULL ? s : "None");
    datara_rt_print_newline();
}

void datara_rt_err(const char* s) {
    fprintf(stderr, "%s\n", s != NULL ? s : "None");
}

void datara_rt_exit(int32_t code) {
    exit(code);
}

#define DATARA_SCRATCH_RING_SIZE (1024 * 1024)

#if defined(_MSC_VER)
#define DATARA_TLS __declspec(thread)
#else
#define DATARA_TLS __thread
#endif

static DATARA_TLS char tls_int_bufs[256][32];
static DATARA_TLS uint32_t tls_int_idx = 0;

static DATARA_TLS char* tls_scratch_ring = NULL;
static DATARA_TLS size_t tls_scratch_offset = 0;

static inline char* datara_scratch_alloc(size_t len) {
    if (!tls_scratch_ring) {
        tls_scratch_ring = (char*)malloc(DATARA_SCRATCH_RING_SIZE);
        if (!tls_scratch_ring) return (char*)malloc(len + 1);
    }
    size_t needed = (len + 1 + 15) & ~15;
    if (needed > (DATARA_SCRATCH_RING_SIZE / 4)) {
        return (char*)malloc(len + 1);
    }
    if (tls_scratch_offset + needed >= DATARA_SCRATCH_RING_SIZE) {
        // Ring exhausted: fall back to the heap instead of wrapping around,
        // which would silently overwrite strings that may still be live.
        // The fallback block is intentionally never freed (the runtime leaks
        // scratch strings by design) — that is safe, unlike corruption.
        return (char*)malloc(len + 1);
    }
    char* p = &tls_scratch_ring[tls_scratch_offset];
    tls_scratch_offset += needed;
    return p;
}

static inline void datara_fast_copy(char* dst, const char* src, size_t len) {
    if (len <= 8) {
        uint64_t v = 0;
        memcpy(&v, src, len);
        memcpy(dst, &v, len);
    } else if (len <= 16) {
        uint64_t v1, v2;
        memcpy(&v1, src, 8);
        memcpy(&v2, src + len - 8, 8);
        memcpy(dst, &v1, 8);
        memcpy(dst + len - 8, &v2, 8);
    } else {
        memcpy(dst, src, len);
    }
}

// Ultra-fast zero-malloc string concatenation via thread-local circular bump allocator
const char* datara_rt_str_concat(const char* a, const char* b) {
    if (!a || a[0] == '\0') return b ? b : "";
    if (!b || b[0] == '\0') return a ? a : "";

    size_t la = strlen(a);
    size_t lb = strlen(b);
    size_t total = la + lb;
    char* buf = datara_scratch_alloc(total);
    if (!buf) return "";
    datara_fast_copy(buf, a, la);
    datara_fast_copy(buf + la, b, lb);
    buf[total] = '\0';
    return buf;
}

static const char DIGITS_LUT[201] =
    "00010203040506070809"
    "10111213141516171819"
    "20212223242526272829"
    "30313233343536373839"
    "40414243444546474849"
    "50515253545556575859"
    "60616263646566676869"
    "70717273747576777879"
    "80818283848586878889"
    "90919293949596979899";

static inline size_t fast_i64toa(int64_t val, char* buf) {
    if (val >= 0 && val < 10000) {
        if (val < 10) {
            buf[0] = (char)('0' + val);
            buf[1] = '\0';
            return 1;
        }
        if (val < 100) {
            memcpy(buf, &DIGITS_LUT[val * 2], 2);
            buf[2] = '\0';
            return 2;
        }
        uint32_t v = (uint32_t)val;
        uint32_t q = v / 100;
        uint32_t r = v - (q * 100);
        if (q >= 10) {
            memcpy(buf, &DIGITS_LUT[q * 2], 2);
            memcpy(buf + 2, &DIGITS_LUT[r * 2], 2);
            buf[4] = '\0';
            return 4;
        } else {
            buf[0] = (char)('0' + q);
            memcpy(buf + 1, &DIGITS_LUT[r * 2], 2);
            buf[3] = '\0';
            return 3;
        }
    } else if (val >= 0 && val < 1000000) {
        uint32_t v = (uint32_t)val;
        uint32_t q1 = v / 10000;
        uint32_t rem = v - (q1 * 10000);
        uint32_t q2 = rem / 100;
        uint32_t r = rem - (q2 * 100);
        char* p = buf;
        if (q1 >= 10) {
            memcpy(p, &DIGITS_LUT[q1 * 2], 2);
            p += 2;
        } else {
            *p++ = (char)('0' + q1);
        }
        memcpy(p, &DIGITS_LUT[q2 * 2], 2);
        p += 2;
        memcpy(p, &DIGITS_LUT[r * 2], 2);
        p += 2;
        *p = '\0';
        return (size_t)(p - buf);
    }
    char temp[32];
    char* p = temp + 31;
    uint64_t uval;
    if (val < 0) {
        uval = (uint64_t)(-(val + 1)) + 1;
    } else {
        uval = (uint64_t)val;
    }
    while (uval >= 100) {
        uint64_t q = uval / 100;
        uint32_t r = (uint32_t)(uval - (q * 100));
        uval = q;
        p -= 2;
        memcpy(p, &DIGITS_LUT[r * 2], 2);
    }
    if (uval >= 10) {
        p -= 2;
        memcpy(p, &DIGITS_LUT[uval * 2], 2);
    } else {
        *--p = (char)('0' + uval);
    }
    size_t len = (size_t)((temp + 31) - p);
    char* dst = buf;
    if (val < 0) {
        *dst++ = '-';
        len++;
    }
    memcpy(dst, p, (size_t)((temp + 31) - p));
    buf[len] = '\0';
    return len;
}

const char* datara_rt_int_to_str(int64_t v) {
    char* buf = tls_int_bufs[tls_int_idx & 255];
    tls_int_idx++;
    size_t len = fast_i64toa(v, buf);
    buf[31] = (char)len;
    return buf;
}

static inline size_t datara_fast_strlen(const char* s) {
    if (!s) return 0;
    ptrdiff_t diff = s - &tls_int_bufs[0][0];
    if ((size_t)diff < sizeof(tls_int_bufs) && (((size_t)diff) & 31) == 0) {
        return (size_t)(uint8_t)s[31];
    }
    return strlen(s);
}

const char* datara_rt_str_concat_3(const char* a, const char* b, const char* c) {
    size_t la = datara_fast_strlen(a);
    size_t lb = datara_fast_strlen(b);
    size_t lc = datara_fast_strlen(c);
    size_t total = la + lb + lc;
    char* buf = datara_scratch_alloc(total);
    if (!buf) return "";
    char* p = buf;
    if (la) { datara_fast_copy(p, a, la); p += la; }
    if (lb) { datara_fast_copy(p, b, lb); p += lb; }
    if (lc) { datara_fast_copy(p, c, lc); p += lc; }
    *p = '\0';
    return buf;
}

const char* datara_rt_str_concat_4(const char* a, const char* b, const char* c, const char* d) {
    size_t la = datara_fast_strlen(a);
    size_t lb = datara_fast_strlen(b);
    size_t lc = datara_fast_strlen(c);
    size_t ld = datara_fast_strlen(d);
    size_t total = la + lb + lc + ld;
    char* buf = datara_scratch_alloc(total);
    if (!buf) return "";
    char* p = buf;
    if (la) { datara_fast_copy(p, a, la); p += la; }
    if (lb) { datara_fast_copy(p, b, lb); p += lb; }
    if (lc) { datara_fast_copy(p, c, lc); p += lc; }
    if (ld) { datara_fast_copy(p, d, ld); p += ld; }
    *p = '\0';
    return buf;
}

const char* datara_rt_str_concat_5(const char* a, const char* b, const char* c, const char* d, const char* e) {
    size_t la = datara_fast_strlen(a);
    size_t lb = datara_fast_strlen(b);
    size_t lc = datara_fast_strlen(c);
    size_t ld = datara_fast_strlen(d);
    size_t le = datara_fast_strlen(e);
    size_t total = la + lb + lc + ld + le;
    char* buf = datara_scratch_alloc(total);
    if (!buf) return "";
    char* p = buf;
    if (la) { datara_fast_copy(p, a, la); p += la; }
    if (lb) { datara_fast_copy(p, b, lb); p += lb; }
    if (lc) { datara_fast_copy(p, c, lc); p += lc; }
    if (ld) { datara_fast_copy(p, d, ld); p += ld; }
    if (le) { datara_fast_copy(p, e, le); p += le; }
    *p = '\0';
    return buf;
}

const char* datara_rt_format_str_i64_str_i64(const char* s1, int64_t n1, const char* s2, int64_t n2) {
    size_t l1 = s1 ? datara_fast_strlen(s1) : 0;
    size_t l2 = s2 ? datara_fast_strlen(s2) : 0;
    size_t max_needed = l1 + l2 + 50;
    char* buf = datara_scratch_alloc(max_needed);
    if (!buf) return "";
    char* p = buf;
    if (l1) {
        datara_fast_copy(p, s1, l1);
        p += l1;
    }
    p += fast_i64toa(n1, p);
    if (l2) {
        datara_fast_copy(p, s2, l2);
        p += l2;
    }
    p += fast_i64toa(n2, p);
    *p = '\0';
    return buf;
}

// 64-bit NaN-Boxing runtime support
// ============================================================================
// Datara Universal Polyglot Value (DataraValue) & 64-bit NaN-Boxing
// ============================================================================

DataraValue datara_val_from_float(double f) {
    DataraValue v;
    memcpy(&v, &f, sizeof(v));
    if ((v & DATARA_QNAN_MASK) == DATARA_QNAN_PREFIX && (v & DATARA_TAG_MASK) != 0) {
        v = DATARA_QNAN_PREFIX; // canonical float NaN
    }
    return v;
}

DataraValue datara_val_from_int(int64_t val) {
    uint64_t payload = (uint64_t)val & DATARA_PAYLOAD_MASK;
    return DATARA_QNAN_PREFIX | DATARA_TAG_INT | payload;
}

DataraValue datara_val_from_bool(int32_t b) {
    return DATARA_QNAN_PREFIX | DATARA_TAG_BOOL | (b ? 1ULL : 0ULL);
}

DataraValue datara_val_from_str(const char* s) {
    return DATARA_QNAN_PREFIX | DATARA_TAG_STR | (((uint64_t)(uintptr_t)s) & DATARA_PAYLOAD_MASK);
}

DataraValue datara_val_from_rawptr(void* ptr) {
    return DATARA_QNAN_PREFIX | DATARA_TAG_RAWPTR | (((uint64_t)(uintptr_t)ptr) & DATARA_PAYLOAD_MASK);
}

DataraValue datara_val_from_handle(uint32_t handle_id) {
    return DATARA_QNAN_PREFIX | DATARA_TAG_HANDLE | (uint64_t)handle_id;
}

DataraValue datara_val_null(void) {
    return DATARA_QNAN_PREFIX | DATARA_TAG_NULL;
}

DataraValue datara_val_undefined(void) {
    return DATARA_QNAN_PREFIX | DATARA_TAG_UNDEFINED;
}

int32_t datara_val_is_float(DataraValue v) {
    return ((v & DATARA_QNAN_MASK) != DATARA_QNAN_PREFIX) || ((v & DATARA_TAG_MASK) == 0);
}

int32_t datara_val_is_int(DataraValue v) {
    return ((v & DATARA_QNAN_MASK) == DATARA_QNAN_PREFIX) && ((v & DATARA_TAG_MASK) == DATARA_TAG_INT);
}

int32_t datara_val_is_bool(DataraValue v) {
    return ((v & DATARA_QNAN_MASK) == DATARA_QNAN_PREFIX) && ((v & DATARA_TAG_MASK) == DATARA_TAG_BOOL);
}

int32_t datara_val_is_str(DataraValue v) {
    return ((v & DATARA_QNAN_MASK) == DATARA_QNAN_PREFIX) && ((v & DATARA_TAG_MASK) == DATARA_TAG_STR);
}

int32_t datara_val_is_rawptr(DataraValue v) {
    return ((v & DATARA_QNAN_MASK) == DATARA_QNAN_PREFIX) && ((v & DATARA_TAG_MASK) == DATARA_TAG_RAWPTR);
}

int32_t datara_val_is_handle(DataraValue v) {
    return ((v & DATARA_QNAN_MASK) == DATARA_QNAN_PREFIX) && ((v & DATARA_TAG_MASK) == DATARA_TAG_HANDLE);
}

int32_t datara_val_is_null(DataraValue v) {
    return ((v & DATARA_QNAN_MASK) == DATARA_QNAN_PREFIX) && ((v & DATARA_TAG_MASK) == DATARA_TAG_NULL);
}

int32_t datara_val_is_undefined(DataraValue v) {
    return ((v & DATARA_QNAN_MASK) == DATARA_QNAN_PREFIX) && ((v & DATARA_TAG_MASK) == DATARA_TAG_UNDEFINED);
}

double datara_val_to_float(DataraValue v) {
    double d;
    memcpy(&d, &v, sizeof(d));
    return d;
}

int64_t datara_val_to_int(DataraValue v) {
    uint64_t payload = v & DATARA_PAYLOAD_MASK;
    if (payload & 0x0000800000000000ULL) {
        payload |= 0xFFFF000000000000ULL;
    }
    return (int64_t)payload;
}

int32_t datara_val_to_bool(DataraValue v) {
    return (v & 1ULL) ? 1 : 0;
}

const char* datara_val_to_str(DataraValue v) {
    return (const char*)(uintptr_t)(v & DATARA_PAYLOAD_MASK);
}

void* datara_val_to_rawptr(DataraValue v) {
    return (void*)(uintptr_t)(v & DATARA_PAYLOAD_MASK);
}

uint32_t datara_val_to_handle(DataraValue v) {
    return (uint32_t)(v & 0xFFFFFFFFULL);
}

// Legacy Nanbox backward compatibility
uint64_t datara_rt_nanbox_int(int64_t val) {
    return datara_val_from_int(val);
}

int64_t datara_rt_nanunbox_int(uint64_t box) {
    return datara_val_to_int(box);
}

uint64_t datara_rt_nanbox_bool(int64_t b) {
    return datara_val_from_bool(b ? 1 : 0);
}

int64_t datara_rt_nanunbox_bool(uint64_t box) {
    return (int64_t)datara_val_to_bool(box);
}

uint64_t datara_rt_nanbox_str(const char* s) {
    return datara_val_from_str(s);
}

const char* datara_rt_nanunbox_str(uint64_t box) {
    return datara_val_to_str(box);
}

void datara_rt_out_val(uint64_t box) {
    if (datara_val_is_float(box)) {
        datara_rt_out_float(datara_val_to_float(box));
    } else if (datara_val_is_int(box)) {
        datara_rt_out_int(datara_val_to_int(box));
    } else if (datara_val_is_bool(box)) {
        datara_rt_out_bool(datara_val_to_bool(box));
    } else if (datara_val_is_str(box)) {
        datara_rt_out_str(datara_val_to_str(box));
    } else if (datara_val_is_null(box) || datara_val_is_undefined(box)) {
        printf("None\n");
    } else if (datara_val_is_handle(box)) {
        uint32_t hid = datara_val_to_handle(box);
        const char* tag = datara_rt_handle_tag(hid);
        printf("<foreign handle #%u [%s]>\n", hid, tag ? tag : "unknown");
    } else {
        datara_rt_out_int((int64_t)box);
    }
}

// ============================================================================
// Foreign Handle Table Implementation (Thread-Safe, Mutex-Guarded, Auto-Growing)
// ============================================================================
typedef struct {
    void* ptr;
    const char* type_tag;
    void (*destructor)(void*);
    uint32_t generation;
    int32_t in_use;
} DataraHandleEntry;

static DataraHandleEntry* g_handles = NULL;
static size_t g_handles_cap = 0;
static size_t g_handles_count = 0;

#if defined(_WIN32)
static SRWLOCK g_handle_lock = SRWLOCK_INIT;
#define HANDLE_LOCK_ACQUIRE() AcquireSRWLockExclusive(&g_handle_lock)
#define HANDLE_LOCK_RELEASE() ReleaseSRWLockExclusive(&g_handle_lock)
#define HANDLE_LOCK_READ_ACQUIRE() AcquireSRWLockShared(&g_handle_lock)
#define HANDLE_LOCK_READ_RELEASE() ReleaseSRWLockShared(&g_handle_lock)
#else
static pthread_mutex_t g_handle_lock = PTHREAD_MUTEX_INITIALIZER;
#define HANDLE_LOCK_ACQUIRE() pthread_mutex_lock(&g_handle_lock)
#define HANDLE_LOCK_RELEASE() pthread_mutex_unlock(&g_handle_lock)
#define HANDLE_LOCK_READ_ACQUIRE() pthread_mutex_lock(&g_handle_lock)
#define HANDLE_LOCK_READ_RELEASE() pthread_mutex_unlock(&g_handle_lock)
#endif

uint32_t datara_rt_handle_alloc(void* ptr, const char* type_tag, void (*destructor)(void*)) {
    HANDLE_LOCK_ACQUIRE();
    if (!g_handles || g_handles_cap == 0) {
        size_t initial_cap = 64;
        g_handles = (DataraHandleEntry*)calloc(initial_cap, sizeof(DataraHandleEntry));
        if (!g_handles) {
            HANDLE_LOCK_RELEASE();
            return 0;
        }
        g_handles_cap = initial_cap;
    }

    // Slot 0 is reserved as null/invalid handle
    size_t found_idx = 0;
    for (size_t i = 1; i < g_handles_cap; ++i) {
        if (!g_handles[i].in_use) {
            found_idx = i;
            break;
        }
    }

    if (found_idx == 0) {
        size_t new_cap = g_handles_cap * 2;
        DataraHandleEntry* new_table = (DataraHandleEntry*)realloc(g_handles, new_cap * sizeof(DataraHandleEntry));
        if (!new_table) {
            HANDLE_LOCK_RELEASE();
            return 0;
        }
        memset(new_table + g_handles_cap, 0, (new_cap - g_handles_cap) * sizeof(DataraHandleEntry));
        found_idx = g_handles_cap;
        g_handles = new_table;
        g_handles_cap = new_cap;
    }

    g_handles[found_idx].ptr = ptr;
    g_handles[found_idx].type_tag = type_tag ? type_tag : "foreign";
    g_handles[found_idx].destructor = destructor;
    g_handles[found_idx].generation++;
    g_handles[found_idx].in_use = 1;
    g_handles_count++;

    HANDLE_LOCK_RELEASE();
    return (uint32_t)found_idx;
}

void* datara_rt_handle_get(uint32_t handle_id) {
    if (handle_id == 0) return NULL;
    HANDLE_LOCK_READ_ACQUIRE();
    void* res = NULL;
    if (g_handles && handle_id < g_handles_cap && g_handles[handle_id].in_use) {
        res = g_handles[handle_id].ptr;
    }
    HANDLE_LOCK_READ_RELEASE();
    return res;
}

const char* datara_rt_handle_tag(uint32_t handle_id) {
    if (handle_id == 0) return NULL;
    HANDLE_LOCK_READ_ACQUIRE();
    const char* res = NULL;
    if (g_handles && handle_id < g_handles_cap && g_handles[handle_id].in_use) {
        res = g_handles[handle_id].type_tag;
    }
    HANDLE_LOCK_READ_RELEASE();
    return res;
}

int32_t datara_rt_handle_free(uint32_t handle_id) {
    if (handle_id == 0) return 0;
    void* ptr = NULL;
    void (*dtor)(void*) = NULL;

    HANDLE_LOCK_ACQUIRE();
    if (g_handles && handle_id < g_handles_cap && g_handles[handle_id].in_use) {
        ptr = g_handles[handle_id].ptr;
        dtor = g_handles[handle_id].destructor;
        g_handles[handle_id].in_use = 0;
        g_handles[handle_id].ptr = NULL;
        g_handles[handle_id].type_tag = NULL;
        g_handles[handle_id].destructor = NULL;
        if (g_handles_count > 0) g_handles_count--;
        HANDLE_LOCK_RELEASE();
        if (dtor && ptr) {
            dtor(ptr);
        }
        return 1;
    }
    HANDLE_LOCK_RELEASE();
    return 0;
}

void datara_rt_handle_shutdown(void) {
    HANDLE_LOCK_ACQUIRE();
    if (g_handles) {
        for (size_t i = 1; i < g_handles_cap; ++i) {
            if (g_handles[i].in_use) {
                void* ptr = g_handles[i].ptr;
                void (*dtor)(void*) = g_handles[i].destructor;
                g_handles[i].in_use = 0;
                g_handles[i].ptr = NULL;
                if (dtor && ptr) {
                    dtor(ptr);
                }
            }
        }
        free(g_handles);
        g_handles = NULL;
        g_handles_cap = 0;
        g_handles_count = 0;
    }
    HANDLE_LOCK_RELEASE();
}

size_t datara_rt_handle_count(void) {
    HANDLE_LOCK_READ_ACQUIRE();
    size_t cnt = g_handles_count;
    HANDLE_LOCK_READ_RELEASE();
    return cnt;
}

// ============================================================================
// DataraMemoryView Implementation
// ============================================================================
size_t datara_dtype_itemsize(DataraDType dtype) {
    switch (dtype) {
        case DATARA_DTYPE_INT8:
        case DATARA_DTYPE_UINT8:
        case DATARA_DTYPE_BOOL:
            return 1;
        case DATARA_DTYPE_INT16:
        case DATARA_DTYPE_UINT16:
            return 2;
        case DATARA_DTYPE_INT32:
        case DATARA_DTYPE_UINT32:
        case DATARA_DTYPE_FLOAT32:
            return 4;
        case DATARA_DTYPE_INT64:
        case DATARA_DTYPE_UINT64:
        case DATARA_DTYPE_FLOAT64:
            return 8;
        default:
            return 8;
    }
}

DataraMemoryView datara_memview_1d(void* data, int64_t length, DataraDType dtype) {
    DataraMemoryView mv;
    memset(&mv, 0, sizeof(mv));
    mv.data = data;
    mv.element_type = dtype;
    size_t sz = datara_dtype_itemsize(dtype);
    mv.ndim = 1;
    mv.shape[0] = length >= 0 ? length : 0;
    mv.strides[0] = (int64_t)sz;
    mv.total_bytes = (size_t)mv.shape[0] * sz;
    return mv;
}

DataraMemoryView datara_memview_2d(void* data, int64_t rows, int64_t cols, DataraDType dtype) {
    DataraMemoryView mv;
    memset(&mv, 0, sizeof(mv));
    mv.data = data;
    mv.element_type = dtype;
    size_t sz = datara_dtype_itemsize(dtype);
    mv.ndim = 2;
    mv.shape[0] = rows >= 0 ? rows : 0;
    mv.shape[1] = cols >= 0 ? cols : 0;
    mv.strides[1] = (int64_t)sz;
    mv.strides[0] = (int64_t)(mv.shape[1] * sz);
    mv.total_bytes = (size_t)(mv.shape[0] * mv.shape[1]) * sz;
    return mv;
}

DataraMemoryView datara_memview_from_list_f64(int64_t* list) {
    if (!list) {
        return datara_memview_1d(NULL, 0, DATARA_DTYPE_FLOAT64);
    }
    int64_t count = list[0];
    if (count < 0) count = 0;
    void* data_ptr = (void*)(&list[1]);
    return datara_memview_1d(data_ptr, count, DATARA_DTYPE_FLOAT64);
}

DataraMemoryView datara_memview_from_vector(void* vec_data, int64_t len, DataraDType dtype) {
    return datara_memview_1d(vec_data, len, dtype);
}

int64_t datara_memview_get_f64(const DataraMemoryView* view, int64_t idx, double* out) {
    if (!view || !view->data || !out || idx < 0 || idx >= view->shape[0]) return 0;
    if (view->element_type == DATARA_DTYPE_FLOAT64) {
        const char* base = (const char*)view->data;
        const double* ptr = (const double*)(base + idx * view->strides[0]);
        *out = *ptr;
        return 1;
    } else if (view->element_type == DATARA_DTYPE_FLOAT32) {
        const char* base = (const char*)view->data;
        const float* ptr = (const float*)(base + idx * view->strides[0]);
        *out = (double)(*ptr);
        return 1;
    }
    return 0;
}

int64_t datara_memview_set_f64(DataraMemoryView* view, int64_t idx, double val) {
    if (!view || !view->data || idx < 0 || idx >= view->shape[0]) return 0;
    if (view->element_type == DATARA_DTYPE_FLOAT64) {
        char* base = (char*)view->data;
        double* ptr = (double*)(base + idx * view->strides[0]);
        *ptr = val;
        return 1;
    } else if (view->element_type == DATARA_DTYPE_FLOAT32) {
        char* base = (char*)view->data;
        float* ptr = (float*)(base + idx * view->strides[0]);
        *ptr = (float)val;
        return 1;
    }
    return 0;
}

int64_t datara_memview_get_i64(const DataraMemoryView* view, int64_t idx, int64_t* out) {
    if (!view || !view->data || !out || idx < 0 || idx >= view->shape[0]) return 0;
    const char* base = (const char*)view->data;
    const char* ptr = base + idx * view->strides[0];
    switch (view->element_type) {
        case DATARA_DTYPE_INT8:   *out = *(const int8_t*)ptr; return 1;
        case DATARA_DTYPE_UINT8:  *out = *(const uint8_t*)ptr; return 1;
        case DATARA_DTYPE_INT16:  *out = *(const int16_t*)ptr; return 1;
        case DATARA_DTYPE_UINT16: *out = *(const uint16_t*)ptr; return 1;
        case DATARA_DTYPE_INT32:  *out = *(const int32_t*)ptr; return 1;
        case DATARA_DTYPE_UINT32: *out = *(const uint32_t*)ptr; return 1;
        case DATARA_DTYPE_INT64:  *out = *(const int64_t*)ptr; return 1;
        case DATARA_DTYPE_UINT64: *out = (int64_t)*(const uint64_t*)ptr; return 1;
        default: return 0;
    }
}

int64_t datara_memview_set_i64(DataraMemoryView* view, int64_t idx, int64_t val) {
    if (!view || !view->data || idx < 0 || idx >= view->shape[0]) return 0;
    char* base = (char*)view->data;
    char* ptr = base + idx * view->strides[0];
    switch (view->element_type) {
        case DATARA_DTYPE_INT8:   *(int8_t*)ptr = (int8_t)val; return 1;
        case DATARA_DTYPE_UINT8:  *(uint8_t*)ptr = (uint8_t)val; return 1;
        case DATARA_DTYPE_INT16:  *(int16_t*)ptr = (int16_t)val; return 1;
        case DATARA_DTYPE_UINT16: *(uint16_t*)ptr = (uint16_t)val; return 1;
        case DATARA_DTYPE_INT32:  *(int32_t*)ptr = (int32_t)val; return 1;
        case DATARA_DTYPE_UINT32: *(uint32_t*)ptr = (uint32_t)val; return 1;
        case DATARA_DTYPE_INT64:  *(int64_t*)ptr = val; return 1;
        case DATARA_DTYPE_UINT64: *(uint64_t*)ptr = (uint64_t)val; return 1;
        default: return 0;
    }
}

// C FFI Flat Buffer Round-trip Test Helpers
int64_t datara_rt_test_list_f64_multiply(int64_t* list, double factor) {
    if (!list) return 0;
    DataraMemoryView mv = datara_memview_from_list_f64(list);
    for (int64_t i = 0; i < mv.shape[0]; ++i) {
        double v = 0.0;
        if (datara_memview_get_f64(&mv, i, &v)) {
            datara_memview_set_f64(&mv, i, v * factor);
        }
    }
    return mv.shape[0];
}

int64_t datara_rt_test_list_f64_sum(int64_t* list) {
    if (!list) return 0;
    DataraMemoryView mv = datara_memview_from_list_f64(list);
    double sum = 0.0;
    for (int64_t i = 0; i < mv.shape[0]; ++i) {
        double v = 0.0;
        if (datara_memview_get_f64(&mv, i, &v)) {
            sum += v;
        }
    }
    return (int64_t)sum;
}

static void test_custom_destructor(void* p) {
    int* counter = (int*)p;
    if (counter) (*counter)++;
}

int32_t datara_rt_polyglot_foundation_test(void) {
    // 1. Float round-trip
    DataraValue vf = datara_val_from_float(3.1415926535);
    if (!datara_val_is_float(vf)) return 0;
    if (fabs(datara_val_to_float(vf) - 3.1415926535) > 1e-9) return 0;

    // 2. Int round-trip
    DataraValue vi = datara_val_from_int(123456789LL);
    if (!datara_val_is_int(vi)) return 0;
    if (datara_val_to_int(vi) != 123456789LL) return 0;

    DataraValue vi_neg = datara_val_from_int(-42LL);
    if (!datara_val_is_int(vi_neg)) return 0;
    if (datara_val_to_int(vi_neg) != -42LL) return 0;

    // 3. Bool round-trip
    DataraValue vb_t = datara_val_from_bool(1);
    DataraValue vb_f = datara_val_from_bool(0);
    if (!datara_val_is_bool(vb_t) || datara_val_to_bool(vb_t) != 1) return 0;
    if (!datara_val_is_bool(vb_f) || datara_val_to_bool(vb_f) != 0) return 0;

    // 4. String pointer round-trip
    const char* test_str = "Datara Polyglot Engine";
    DataraValue vs = datara_val_from_str(test_str);
    if (!datara_val_is_str(vs)) return 0;
    if (strcmp(datara_val_to_str(vs), test_str) != 0) return 0;

    // 5. Raw pointer round-trip
    int dummy_target = 0x55AA;
    DataraValue vp = datara_val_from_rawptr(&dummy_target);
    if (!datara_val_is_rawptr(vp)) return 0;
    if (datara_val_to_rawptr(vp) != &dummy_target) return 0;

    // 6. Handle table & destructor test
    int dtor_called = 0;
    uint32_t h1 = datara_rt_handle_alloc(&dtor_called, "test_resource", test_custom_destructor);
    if (h1 == 0) return 0;
    if (datara_rt_handle_get(h1) != &dtor_called) return 0;
    if (strcmp(datara_rt_handle_tag(h1), "test_resource") != 0) return 0;
    if (!datara_rt_handle_free(h1)) return 0;
    if (dtor_called != 1) return 0;
    if (datara_rt_handle_get(h1) != NULL) return 0;

    // 7. MemoryView flat buffer roundtrip
    double raw_buf[4] = { 1.5, 2.5, 3.5, 4.5 };
    DataraMemoryView mv = datara_memview_1d(raw_buf, 4, DATARA_DTYPE_FLOAT64);
    if (mv.total_bytes != 4 * sizeof(double)) return 0;
    if (mv.ndim != 1 || mv.shape[0] != 4 || mv.strides[0] != sizeof(double)) return 0;
    double elem = 0.0;
    if (!datara_memview_get_f64(&mv, 2, &elem) || fabs(elem - 3.5) > 1e-9) return 0;
    if (!datara_memview_set_f64(&mv, 2, 99.0)) return 0;
    if (raw_buf[2] != 99.0) return 0;

    return 1;
}

// Ultra-Fast Zero-Allocation Streaming Terminal I/O Subsystem
#define DATARA_OUT_BUF_SIZE 65536
static DATARA_TLS char datara_out_buf[DATARA_OUT_BUF_SIZE];
static DATARA_TLS size_t datara_out_pos = 0;

static inline size_t datara_fast_i64toa(int64_t val, char* dst) {
    char temp[32];
    char* p = temp;
    uint64_t u = (uint64_t)val;
    size_t len = 0;
    if (val < 0) {
        dst[len++] = '-';
        u = ~u + 1;
    }
    do {
        *p++ = (char)('0' + (u % 10));
        u /= 10;
    } while (u > 0);
    while (p > temp) {
        dst[len++] = *--p;
    }
    return len;
}

void datara_rt_flush(void) {
    if (datara_out_pos > 0) {
#ifdef _WIN32
        HANDLE hOut = GetStdHandle(STD_OUTPUT_HANDLE);
        if (hOut != INVALID_HANDLE_VALUE && hOut != NULL) {
            DWORD written = 0;
            WriteFile(hOut, datara_out_buf, (DWORD)datara_out_pos, &written, NULL);
        } else {
            fwrite(datara_out_buf, 1, datara_out_pos, stdout);
            fflush(stdout);
        }
#else
        ssize_t ret = write(1, datara_out_buf, datara_out_pos);
        (void)ret;
#endif
        datara_out_pos = 0;
    }
}

static DATARA_TLS int g_capture_enabled = 0;
static DATARA_TLS char* g_capture_buf = NULL;
static DATARA_TLS size_t g_capture_len = 0;
static DATARA_TLS size_t g_capture_cap = 0;

void datara_rt_set_capture(int32_t enable) {
    g_capture_enabled = enable;
    if (enable) {
        datara_rt_clear_capture();
    }
}

void datara_rt_clear_capture(void) {
    if (g_capture_buf) {
        g_capture_buf[0] = '\0';
    }
    g_capture_len = 0;
}

const char* datara_rt_get_capture(void) {
    datara_rt_flush();
    return g_capture_buf ? g_capture_buf : "";
}

static void datara_capture_append(const char* s, size_t len) {
    if (!s || len == 0) return;
    if (g_capture_len + len + 1 > g_capture_cap) {
        size_t new_cap = (g_capture_cap == 0) ? 4096 : (g_capture_cap * 2 + len + 1);
        char* new_buf = (char*)realloc(g_capture_buf, new_cap);
        if (!new_buf) return;
        g_capture_buf = new_buf;
        g_capture_cap = new_cap;
    }
    memcpy(g_capture_buf + g_capture_len, s, len);
    g_capture_len += len;
    g_capture_buf[g_capture_len] = '\0';
}

static inline void datara_rt_buf_write(const char* s, size_t len) {
    if (!s || len == 0) return;
    if (g_capture_enabled) {
        datara_capture_append(s, len);
        return;
    }
    if (len >= DATARA_OUT_BUF_SIZE) {
        datara_rt_flush();
#ifdef _WIN32
        HANDLE hOut = GetStdHandle(STD_OUTPUT_HANDLE);
        if (hOut != INVALID_HANDLE_VALUE && hOut != NULL) {
            DWORD written = 0;
            WriteFile(hOut, s, (DWORD)len, &written, NULL);
            return;
        }
#else
        ssize_t ret = write(1, s, len);
        (void)ret;
        return;
#endif
        fwrite(s, 1, len, stdout);
        fflush(stdout);
        return;
    }
    if (datara_out_pos + len > DATARA_OUT_BUF_SIZE) {
        datara_rt_flush();
    }
    memcpy(datara_out_buf + datara_out_pos, s, len);
    datara_out_pos += len;
}

void datara_rt_print_str(const char* s) {
    if (!s) {
        datara_rt_buf_write("None", 4);
    } else {
        datara_rt_buf_write(s, strlen(s));
    }
}

void datara_rt_print_int(int64_t v) {
    char buf[32];
    size_t len = datara_fast_i64toa(v, buf);
    datara_rt_buf_write(buf, len);
}

void datara_rt_print_float(double v) {
    if (isnan(v)) {
        datara_rt_buf_write(signbit(v) ? "-NaN" : "NaN", signbit(v) ? 4 : 3);
        return;
    }
    if (isinf(v)) {
        datara_rt_buf_write(v < 0 ? "-Infinity" : "Infinity", v < 0 ? 9 : 8);
        return;
    }
    char buf[64];
    if (v == floor(v) && fabs(v) < 1e15) {
        int len = snprintf(buf, sizeof(buf), "%.0f", v);
        datara_rt_buf_write(buf, (size_t)len);
        return;
    }
    for (int prec = 4; prec < 17; prec++) {
        snprintf(buf, sizeof(buf), "%.*g", prec, v);
        if (strtod(buf, NULL) == v) {
            datara_rt_buf_write(buf, strlen(buf));
            return;
        }
    }
    snprintf(buf, sizeof(buf), "%.17g", v);
    datara_rt_buf_write(buf, strlen(buf));
}

void datara_rt_print_bool(int64_t v) {
    if (v) {
        datara_rt_buf_write("true", 4);
    } else {
        datara_rt_buf_write("false", 5);
    }
}

void datara_rt_print_space(void) {
    datara_rt_buf_write(" ", 1);
}

void datara_rt_print_newline(void) {
    datara_rt_buf_write("\n", 1);
    datara_rt_flush();
}

void datara_rt_print_list(void* list) {
    if (!list) {
        datara_rt_buf_write("[]", 2);
        return;
    }
    int64_t* arr = (int64_t*)list;
    int64_t len = arr[0];
    datara_rt_buf_write("[", 1);
    for (int64_t i = 0; i < len; i++) {
        if (i > 0) {
            datara_rt_buf_write(", ", 2);
        }
        datara_rt_print_int(arr[i + 1]);
    }
    datara_rt_buf_write("]", 1);
}

// Prelude built-in functions
void datara_rt_println(const char* s) {
    datara_rt_print_str(s);
    datara_rt_print_newline();
}

void datara_rt_print(const char* s) {
    datara_rt_print_str(s);
    datara_rt_flush();
}

void datara_rt_eprintln(const char* s) {
    datara_rt_flush();
    fputs(s ? s : "", stderr);
    fputc('\n', stderr);
    fflush(stderr);
}

void datara_rt_print_backtrace(void) {
    fprintf(stderr, "stack backtrace:\n");
#ifdef _WIN32
    void* stack[64];
    USHORT frames = CaptureStackBackTrace(1, 64, stack, NULL);
    if (frames == 0) {
        fprintf(stderr, "  (empty backtrace)\n");
        fflush(stderr);
        return;
    }

    HANDLE hProcess = GetCurrentProcess();
    HMODULE hDbgHelp = LoadLibraryA("dbghelp.dll");
    typedef BOOL (WINAPI *SymInitializeFn)(HANDLE, PCSTR, BOOL);
    typedef BOOL (WINAPI *SymFromAddrFn)(HANDLE, DWORD64, PDWORD64, void*);
    typedef BOOL (WINAPI *SymGetLineFromAddr64Fn)(HANDLE, DWORD64, PDWORD, void*);
    typedef BOOL (WINAPI *SymCleanupFn)(HANDLE);

    SymInitializeFn pSymInit = NULL;
    SymFromAddrFn pSymFromAddr = NULL;
    SymGetLineFromAddr64Fn pSymGetLine = NULL;
    SymCleanupFn pSymCleanup = NULL;

    if (hDbgHelp) {
        pSymInit = (SymInitializeFn)GetProcAddress(hDbgHelp, "SymInitialize");
        pSymFromAddr = (SymFromAddrFn)GetProcAddress(hDbgHelp, "SymFromAddr");
        pSymGetLine = (SymGetLineFromAddr64Fn)GetProcAddress(hDbgHelp, "SymGetLineFromAddr64");
        pSymCleanup = (SymCleanupFn)GetProcAddress(hDbgHelp, "SymCleanup");
        if (pSymInit) {
            pSymInit(hProcess, NULL, TRUE);
        }
    }

    char buffer[sizeof(SYMBOL_INFO) + 256 * sizeof(char)];
    PSYMBOL_INFO symbol = (PSYMBOL_INFO)buffer;
    symbol->SizeOfStruct = sizeof(SYMBOL_INFO);
    symbol->MaxNameLen = 255;

    IMAGEHLP_LINE64 line_info;
    memset(&line_info, 0, sizeof(IMAGEHLP_LINE64));
    line_info.SizeOfStruct = sizeof(IMAGEHLP_LINE64);

    for (USHORT i = 0; i < frames; i++) {
        DWORD64 address = (DWORD64)(uintptr_t)stack[i];
        const char* name = "???";
        DWORD displacement = 0;
        char loc[512] = {0};

        if (pSymFromAddr && pSymFromAddr(hProcess, address, 0, symbol)) {
            name = symbol->Name;
        }

        if (pSymGetLine && pSymGetLine(hProcess, address, &displacement, &line_info)) {
            snprintf(loc, sizeof(loc), "      at %s:%lu", line_info.FileName, (unsigned long)line_info.LineNumber);
        }

        if (loc[0] != '\0') {
            fprintf(stderr, "  %2u: %s\n%s (0x%llx)\n", (unsigned int)i, name, loc, (unsigned long long)address);
        } else {
            fprintf(stderr, "  %2u: %s (0x%llx)\n", (unsigned int)i, name, (unsigned long long)address);
        }
    }

    if (hDbgHelp) {
        if (pSymCleanup) pSymCleanup(hProcess);
        FreeLibrary(hDbgHelp);
    }
#else
#if defined(DATARA_HAS_EXECINFO)
    void* stack[64];
    int frames = backtrace(stack, 64);
    char** symbols = backtrace_symbols(stack, frames);
    if (symbols) {
        for (int i = 1; i < frames; i++) {
            fprintf(stderr, "  %2d: %s\n", i - 1, symbols[i]);
        }
        free(symbols);
    } else {
        for (int i = 1; i < frames; i++) {
            fprintf(stderr, "  %2d: 0x%lx\n", i - 1, (unsigned long)(uintptr_t)stack[i]);
        }
    }
#else
    fprintf(stderr, "  (native backtrace not supported on this platform)\n");
#endif
#endif
    fflush(stderr);
}

void datara_rt_panic(const char* s) {
    datara_rt_flush();
    fprintf(stderr, "panic: %s\n", s ? s : "explicit panic");
    datara_rt_print_backtrace();
    fflush(stderr);
    exit(1);
}

void datara_rt_assert(int64_t cond, const char* msg) {
    if (!cond) {
        datara_rt_panic(msg ? msg : "assertion failed");
    }
}

void datara_rt_overflow_panic(void) {
    datara_rt_panic("integer overflow");
}

void datara_rt_div_zero_panic(void) {
    datara_rt_panic("integer division by zero");
}

int64_t datara_rt_checked_add(int64_t a, int64_t b) {
    int64_t res;
#if defined(__GNUC__) || defined(__clang__)
    if (__builtin_add_overflow(a, b, &res)) {
        datara_rt_panic("integer overflow");
    }
#else
    if ((b > 0 && a > INT64_MAX - b) || (b < 0 && a < INT64_MIN - b)) {
        datara_rt_panic("integer overflow");
    }
    res = a + b;
#endif
    return res;
}

int64_t datara_rt_checked_sub(int64_t a, int64_t b) {
    int64_t res;
#if defined(__GNUC__) || defined(__clang__)
    if (__builtin_sub_overflow(a, b, &res)) {
        datara_rt_panic("integer overflow");
    }
#else
    if ((b < 0 && a > INT64_MAX + b) || (b > 0 && a < INT64_MIN + b)) {
        datara_rt_panic("integer overflow");
    }
    res = a - b;
#endif
    return res;
}

int64_t datara_rt_checked_mul(int64_t a, int64_t b) {
    int64_t res;
#if defined(__GNUC__) || defined(__clang__)
    if (__builtin_mul_overflow(a, b, &res)) {
        datara_rt_panic("integer overflow");
    }
#else
    if (a > 0) {
        if (b > 0) {
            if (a > (INT64_MAX / b)) datara_rt_panic("integer overflow");
        } else {
            if (b < (INT64_MIN / a)) datara_rt_panic("integer overflow");
        }
    } else {
        if (b > 0) {
            if (a < (INT64_MIN / b)) datara_rt_panic("integer overflow");
        } else {
            if (a != 0 && b < (INT64_MAX / a)) datara_rt_panic("integer overflow");
        }
    }
    res = a * b;
#endif
    return res;
}

int64_t datara_rt_checked_div(int64_t a, int64_t b) {
    if (b == 0) {
        datara_rt_panic("integer division by zero");
    }
    if (a == INT64_MIN && b == -1) {
        datara_rt_panic("integer overflow");
    }
    return a / b;
}

int64_t datara_rt_checked_rem(int64_t a, int64_t b) {
    if (b == 0) {
        datara_rt_panic("integer division by zero");
    }
    if (a == INT64_MIN && b == -1) {
        datara_rt_panic("integer overflow");
    }
    return a % b;
}

int64_t datara_rt_saturating_add(int64_t a, int64_t b) {
    int64_t res;
#if defined(__GNUC__) || defined(__clang__)
    if (__builtin_add_overflow(a, b, &res)) {
        return (a >= 0) ? INT64_MAX : INT64_MIN;
    }
    return res;
#else
    if (b > 0 && a > INT64_MAX - b) return INT64_MAX;
    if (b < 0 && a < INT64_MIN - b) return INT64_MIN;
    return a + b;
#endif
}

int64_t datara_rt_saturating_sub(int64_t a, int64_t b) {
    int64_t res;
#if defined(__GNUC__) || defined(__clang__)
    if (__builtin_sub_overflow(a, b, &res)) {
        return (a >= 0) ? INT64_MAX : INT64_MIN;
    }
    return res;
#else
    if (b < 0 && a > INT64_MAX + b) return INT64_MAX;
    if (b > 0 && a < INT64_MIN + b) return INT64_MIN;
    return a - b;
#endif
}

int64_t datara_rt_saturating_mul(int64_t a, int64_t b) {
    int64_t res;
#if defined(__GNUC__) || defined(__clang__)
    if (__builtin_mul_overflow(a, b, &res)) {
        return ((a ^ b) >= 0) ? INT64_MAX : INT64_MIN;
    }
    return res;
#else
    if (a > 0) {
        if (b > 0 && a > (INT64_MAX / b)) return INT64_MAX;
        if (b < 0 && b < (INT64_MIN / a)) return INT64_MIN;
    } else if (a < 0) {
        if (b > 0 && a < (INT64_MIN / b)) return INT64_MIN;
        if (b < 0 && b < (INT64_MAX / a)) return INT64_MAX;
    }
    return a * b;
#endif
}

int64_t datara_rt_wrapping_add(int64_t a, int64_t b) {
    return (int64_t)((uint64_t)a + (uint64_t)b);
}

int64_t datara_rt_wrapping_sub(int64_t a, int64_t b) {
    return (int64_t)((uint64_t)a - (uint64_t)b);
}

int64_t datara_rt_wrapping_mul(int64_t a, int64_t b) {
    return (int64_t)((uint64_t)a * (uint64_t)b);
}

int64_t datara_rt_len(const char* s) {
    return s ? (int64_t)strlen(s) : 0;
}

const char* datara_rt_input(const char* prompt) {
    datara_rt_flush();
    if (prompt && prompt[0] != '\0') {
        datara_rt_print_str(prompt);
        datara_rt_flush();
    }
    char* buf = datara_scratch_alloc(1024);
    if (!buf) return "";
    if (fgets(buf, 1024, stdin) == NULL) {
        buf[0] = '\0';
        return buf;
    }
    size_t len = strlen(buf);
    while (len > 0 && (buf[len - 1] == '\n' || buf[len - 1] == '\r')) {
        buf[--len] = '\0';
    }
    return buf;
}

int64_t datara_rt_input_int(const char* prompt) {
    const char* s = datara_rt_input(prompt);
    if (!s || s[0] == '\0') return 0;
    return (int64_t)strtoll(s, NULL, 10);
}

double datara_rt_input_float(const char* prompt) {
    const char* s = datara_rt_input(prompt);
    if (!s || s[0] == '\0') return 0.0;
    return strtod(s, NULL);
}

void datara_rt_out_dec64(int64_t val) {
    int64_t integer = val / 10000;
    int64_t frac = val % 10000;
    if (frac < 0) frac = -frac;
    printf("%lld.%04lld\n", integer, frac);
}

int64_t datara_rt_str_eq(const char* a, const char* b) {
    if (a == b) return 1;
    if (!a || !b) return 0;
    return strcmp(a, b) == 0 ? 1 : 0;
}

int64_t datara_rt_str_len(const char* s) {
    return s ? (int64_t)strlen(s) : 0;
}

int64_t datara_rt_byte_len(const char* s) {
    return s ? (int64_t)strlen(s) : 0;
}

int64_t datara_rt_list_get(int64_t* list, int64_t idx) {
    if (!list) return 0;
    int64_t count = list[0];
    if (idx < 0 || idx >= count) return 0;
    return list[idx + 1];
}

int64_t datara_rt_list_get_unchecked(int64_t* list, int64_t idx) {
    return list[idx + 1];
}

int64_t datara_rt_list_len(int64_t* list) {
    return list ? list[0] : 0;
}

int64_t* datara_rt_list_set(int64_t* list, int64_t idx, int64_t v) {
    if (!list) return NULL;
    int64_t count = list[0];
    if (idx < 0 || idx >= count) return list;
    list[idx + 1] = v;
    return list;
}

int64_t* datara_rt_list_set_unchecked(int64_t* list, int64_t idx, int64_t v) {
    list[idx + 1] = v;
    return list;
}

typedef struct {
    int64_t capacity;
    int64_t magic;
} DataraListHeader;

#define DATARA_LIST_MAGIC 0x4441544C49535430ULL

int64_t* datara_rt_list_create_capacity(int64_t cap) {
    if (cap < 8) cap = 8;
    if (cap > (int64_t)((SIZE_MAX - sizeof(DataraListHeader)) / sizeof(int64_t) - 1)) return NULL;
    DataraListHeader* hdr = (DataraListHeader*)malloc(sizeof(DataraListHeader) + (size_t)(cap + 1) * sizeof(int64_t));
    if (!hdr) return NULL;
    hdr->capacity = cap;
    hdr->magic = DATARA_LIST_MAGIC;
    int64_t* list = (int64_t*)(hdr + 1);
    list[0] = 0;
    return list;
}

int64_t* datara_rt_list_create(int64_t cap) {
    return datara_rt_list_create_capacity(cap);
}

int64_t* datara_rt_list_append(int64_t* list, int64_t v) {
    if (!list) {
        int64_t* arr = datara_rt_list_create_capacity(8);
        if (!arr) return NULL;
        arr[0] = 1;
        arr[1] = v;
        return arr;
    }
    int64_t count = list[0];
    if (count < 0) return list;
    DataraListHeader* hdr = ((DataraListHeader*)list) - 1;
    if (hdr->magic == DATARA_LIST_MAGIC && hdr->capacity >= count) {
        if (count + 1 > hdr->capacity) {
            if (hdr->capacity > (int64_t)((SIZE_MAX - sizeof(DataraListHeader)) / (2 * sizeof(int64_t)) - 1)) return list;
            int64_t new_cap = hdr->capacity * 2;
            if (new_cap < 8) new_cap = 8;
            DataraListHeader* new_hdr = (DataraListHeader*)realloc(hdr, sizeof(DataraListHeader) + (size_t)(new_cap + 1) * sizeof(int64_t));
            if (!new_hdr) return list;
            hdr = new_hdr;
            hdr->capacity = new_cap;
            list = (int64_t*)(hdr + 1);
        }
        list[0] = count + 1;
        list[count + 1] = v;
        return list;
    }

    if (count > (int64_t)((SIZE_MAX - sizeof(DataraListHeader)) / (2 * sizeof(int64_t)) - 2)) return list;
    int64_t new_cap = (count + 1) * 2;
    if (new_cap < 8) new_cap = 8;
    DataraListHeader* new_hdr = (DataraListHeader*)malloc(sizeof(DataraListHeader) + (size_t)(new_cap + 1) * sizeof(int64_t));
    if (!new_hdr) return list;
    new_hdr->capacity = new_cap;
    int64_t* new_list = (int64_t*)(new_hdr + 1);
    new_list[0] = count + 1;
    for (int64_t i = 1; i <= count; i++) {
        new_list[i] = list[i];
    }
    new_list[count + 1] = v;
    return new_list;
}

int64_t datara_rt_list_pop(int64_t* list) {
    if (!list || list[0] <= 0) return 0;
    int64_t count = list[0];
    int64_t val = list[count];
    list[0] = count - 1;
    return val;
}

int64_t* datara_rt_slice(int64_t* list, int64_t start, int64_t end) {
    if (!list) return NULL;
    int64_t len = list[0];
    if (len < 0) return NULL;
    if (start < 0) start = 0;
    if (end > len) end = len;
    if (start >= end) {
        return datara_rt_list_create_capacity(0);
    }
    int64_t count = end - start;
    if (count > (int64_t)((SIZE_MAX - sizeof(DataraListHeader)) / sizeof(int64_t) - 1)) return NULL;
    DataraListHeader* hdr = (DataraListHeader*)malloc(sizeof(DataraListHeader) + (size_t)(count + 1) * sizeof(int64_t));
    if (!hdr) return NULL;
    hdr->capacity = count;
    hdr->magic = DATARA_LIST_MAGIC;
    int64_t* res = (int64_t*)(hdr + 1);
    res[0] = count;
    for (int64_t i = 0; i < count; i++) {
        res[i + 1] = list[start + i + 1];
    }
    return res;
}

int64_t* datara_rt_list_create_repeat(int64_t elem, int64_t count) {
    if (count < 0) count = 0;
    if (count > (int64_t)((SIZE_MAX - sizeof(DataraListHeader)) / sizeof(int64_t) - 1)) return NULL;
    DataraListHeader* hdr = (DataraListHeader*)malloc(sizeof(DataraListHeader) + (size_t)(count + 1) * sizeof(int64_t));
    if (!hdr) return NULL;
    hdr->capacity = count;
    hdr->magic = DATARA_LIST_MAGIC;
    int64_t* arr = (int64_t*)(hdr + 1);
    arr[0] = count;
    for (int64_t i = 0; i < count; i++) {
        arr[i + 1] = elem;
    }
    return arr;
}

static int64_t* datara_rt_list_create_from(const int64_t* vals, int64_t count) {
    int64_t cap = count < 8 ? 8 : count;
    DataraListHeader* hdr = (DataraListHeader*)malloc(sizeof(DataraListHeader) + (size_t)(cap + 1) * sizeof(int64_t));
    if (!hdr) return NULL;
    hdr->capacity = cap;
    hdr->magic = DATARA_LIST_MAGIC;
    int64_t* arr = (int64_t*)(hdr + 1);
    arr[0] = count;
    for (int64_t i = 0; i < count; i++) {
        arr[i + 1] = vals[i];
    }
    for (int64_t i = count; i < cap; i++) {
        arr[i + 1] = 0;
    }
    return arr;
}

int64_t* datara_rt_list_create_1(int64_t a) {
    return datara_rt_list_create_from(&a, 1);
}

int64_t* datara_rt_list_create_2(int64_t a, int64_t b) {
    int64_t vals[2] = { a, b };
    return datara_rt_list_create_from(vals, 2);
}

int64_t* datara_rt_list_create_3(int64_t a, int64_t b, int64_t c) {
    int64_t vals[3] = { a, b, c };
    return datara_rt_list_create_from(vals, 3);
}

int64_t* datara_rt_list_create_4(int64_t a, int64_t b, int64_t c, int64_t d) {
    int64_t vals[4] = { a, b, c, d };
    return datara_rt_list_create_from(vals, 4);
}

int64_t* datara_rt_list_create_5(int64_t a, int64_t b, int64_t c, int64_t d, int64_t e) {
    int64_t vals[5] = { a, b, c, d, e };
    return datara_rt_list_create_from(vals, 5);
}

typedef struct {
    int64_t capacity;
    int64_t magic;
} DataraMapHeader;

#define DATARA_MAP_MAGIC 0x4441544D41503130ULL

void* datara_rt_map_create(void) {
    size_t init_cap = 8;
    size_t total_bytes = sizeof(DataraMapHeader) + (1 + init_cap * 2) * sizeof(int64_t);
    DataraMapHeader* hdr = (DataraMapHeader*)malloc(total_bytes);
    if (!hdr) return NULL;
    hdr->capacity = init_cap;
    hdr->magic = DATARA_MAP_MAGIC;
    int64_t* map = (int64_t*)(hdr + 1);
    map[0] = 0;
    return map;
}

int64_t* datara_rt_map_create_2(const char* k0, int64_t v0, const char* k1, int64_t v1) {
    size_t init_cap = 8;
    size_t total_bytes = sizeof(DataraMapHeader) + (1 + init_cap * 2) * sizeof(int64_t);
    DataraMapHeader* hdr = (DataraMapHeader*)malloc(total_bytes);
    if (!hdr) return NULL;
    hdr->capacity = init_cap;
    hdr->magic = DATARA_MAP_MAGIC;
    int64_t* map = (int64_t*)(hdr + 1);
    map[0] = 2;
    map[1] = (int64_t)k0;
    map[2] = v0;
    map[3] = (int64_t)k1;
    map[4] = v1;
    return map;
}

static int64_t* datara_rt_map_create_from(const char** keys, const int64_t* vals, int64_t n) {
    size_t init_cap = 8;
    if ((size_t)n > init_cap) init_cap = (size_t)n;
    size_t total_bytes = sizeof(DataraMapHeader) + (1 + init_cap * 2) * sizeof(int64_t);
    DataraMapHeader* hdr = (DataraMapHeader*)malloc(total_bytes);
    if (!hdr) return NULL;
    hdr->capacity = init_cap;
    hdr->magic = DATARA_MAP_MAGIC;
    int64_t* map = (int64_t*)(hdr + 1);
    map[0] = n;
    for (int64_t i = 0; i < n; i++) {
        map[1 + i * 2] = (int64_t)keys[i];
        map[2 + i * 2] = vals[i];
    }
    return map;
}

void* datara_rt_map_create_1(const char* k0, int64_t v0) {
    const char* keys[1] = { k0 };
    const int64_t vals[1] = { v0 };
    return datara_rt_map_create_from(keys, vals, 1);
}

int64_t* datara_rt_map_create_3(const char* k0, int64_t v0, const char* k1, int64_t v1,
                                const char* k2, int64_t v2) {
    const char* keys[3] = { k0, k1, k2 };
    const int64_t vals[3] = { v0, v1, v2 };
    return datara_rt_map_create_from(keys, vals, 3);
}

int64_t* datara_rt_map_create_4(const char* k0, int64_t v0, const char* k1, int64_t v1,
                                const char* k2, int64_t v2, const char* k3, int64_t v3) {
    const char* keys[4] = { k0, k1, k2, k3 };
    const int64_t vals[4] = { v0, v1, v2, v3 };
    return datara_rt_map_create_from(keys, vals, 4);
}

int64_t* datara_rt_map_create_5(const char* k0, int64_t v0, const char* k1, int64_t v1,
                                const char* k2, int64_t v2, const char* k3, int64_t v3,
                                const char* k4, int64_t v4) {
    const char* keys[5] = { k0, k1, k2, k3, k4 };
    const int64_t vals[5] = { v0, v1, v2, v3, v4 };
    return datara_rt_map_create_from(keys, vals, 5);
}

int64_t datara_rt_map_get(int64_t* map, const char* key) {
    if (!map || !key) return 0;
    int64_t count = map[0];
    for (int64_t i = 0; i < count; i++) {
        const char* k = (const char*)map[1 + i * 2];
        if (k && strcmp(k, key) == 0) {
            return map[2 + i * 2];
        }
    }
    return 0;
}

static inline DataraMapHeader* datara_rt_map_get_header(int64_t* map) {
    if (!map) return NULL;
    DataraMapHeader* hdr = ((DataraMapHeader*)map) - 1;
    if (hdr->magic == DATARA_MAP_MAGIC) return hdr;
    return NULL;
}

int64_t* datara_rt_map_insert(int64_t* map, const char* key, int64_t val) {
    if (!map) {
        size_t init_cap = 8;
        size_t total_bytes = sizeof(DataraMapHeader) + (1 + init_cap * 2) * sizeof(int64_t);
        DataraMapHeader* hdr = (DataraMapHeader*)malloc(total_bytes);
        if (!hdr) return NULL;
        hdr->capacity = init_cap;
        hdr->magic = DATARA_MAP_MAGIC;
        int64_t* m = (int64_t*)(hdr + 1);
        m[0] = 1;
        m[1] = (int64_t)key;
        m[2] = val;
        return m;
    }
    int64_t count = map[0];
    for (int64_t i = 0; i < count; i++) {
        const char* k = (const char*)map[1 + i * 2];
        if (k && key && strcmp(k, key) == 0) {
            map[2 + i * 2] = val;
            return map;
        }
    }
    DataraMapHeader* hdr = datara_rt_map_get_header(map);
    if (hdr) {
        if (count < hdr->capacity) {
            map[1 + count * 2] = (int64_t)key;
            map[2 + count * 2] = val;
            map[0] = count + 1;
            return map;
        }
        int64_t new_cap = hdr->capacity * 2;
        size_t total_bytes = sizeof(DataraMapHeader) + (1 + new_cap * 2) * sizeof(int64_t);
        DataraMapHeader* new_hdr = (DataraMapHeader*)realloc(hdr, total_bytes);
        if (!new_hdr) return map;
        new_hdr->capacity = new_cap;
        int64_t* new_map = (int64_t*)(new_hdr + 1);
        new_map[1 + count * 2] = (int64_t)key;
        new_map[2 + count * 2] = val;
        new_map[0] = count + 1;
        return new_map;
    } else {
        size_t init_cap = (count + 1) < 8 ? 8 : (count + 1) * 2;
        size_t total_bytes = sizeof(DataraMapHeader) + (1 + init_cap * 2) * sizeof(int64_t);
        DataraMapHeader* new_hdr = (DataraMapHeader*)malloc(total_bytes);
        if (!new_hdr) return map;
        new_hdr->capacity = init_cap;
        new_hdr->magic = DATARA_MAP_MAGIC;
        int64_t* new_map = (int64_t*)(new_hdr + 1);
        new_map[0] = count + 1;
        for (int64_t i = 0; i < count; i++) {
            new_map[1 + i * 2] = map[1 + i * 2];
            new_map[2 + i * 2] = map[2 + i * 2];
        }
        new_map[1 + count * 2] = (int64_t)key;
        new_map[2 + count * 2] = val;
        free(map);
        return new_map;
    }
}

void datara_rt_map_free(void* map) {
    if (!map) return;
    DataraMapHeader* hdr = datara_rt_map_get_header((int64_t*)map);
    if (hdr) {
        free(hdr);
    } else {
        free(map);
    }
}

const char* datara_rt_range_str(int64_t start, int64_t end) {
    char* buf = (char*)malloc(48);
    if (!buf) return "";
    size_t l1 = fast_i64toa(start, buf);
    buf[l1] = '.';
    buf[l1 + 1] = '.';
    fast_i64toa(end, buf + l1 + 2);
    return buf;
}

#ifdef _WIN32
#include <windows.h>
int64_t now_ms(void) {
    static LARGE_INTEGER freq = {0};
    if (freq.QuadPart == 0) {
        QueryPerformanceFrequency(&freq);
    }
    LARGE_INTEGER count;
    QueryPerformanceCounter(&count);
    return (int64_t)((count.QuadPart * 1000) / freq.QuadPart);
}
int64_t datara_rt_now_ms(void) {
    return now_ms();
}
int64_t datara_rt_now_unix_ms(void) {
    FILETIME ft;
    GetSystemTimeAsFileTime(&ft);
    ULARGE_INTEGER uli;
    uli.LowPart = ft.dwLowDateTime;
    uli.HighPart = ft.dwHighDateTime;
    return (int64_t)((uli.QuadPart - 116444736000000000ULL) / 10000ULL);
}
int64_t datara_rt_now_precise_ms(void) {
    static LARGE_INTEGER freq = {0};
    if (freq.QuadPart == 0) {
        QueryPerformanceFrequency(&freq);
    }
    LARGE_INTEGER count;
    QueryPerformanceCounter(&count);
    return (int64_t)((count.QuadPart * 1000000) / freq.QuadPart);
}
int64_t now_ns(void) {
    static LARGE_INTEGER freq = {0};
    if (freq.QuadPart == 0) {
        QueryPerformanceFrequency(&freq);
    }
    LARGE_INTEGER count;
    QueryPerformanceCounter(&count);
    int64_t sec = count.QuadPart / freq.QuadPart;
    int64_t rem = count.QuadPart % freq.QuadPart;
    return sec * 1000000000LL + (rem * 1000000000LL) / freq.QuadPart;
}
int64_t datara_rt_now_ns(void) {
    return now_ns();
}
#else
#include <time.h>
int64_t now_ms(void) {
    struct timespec ts;
    clock_gettime(CLOCK_MONOTONIC, &ts);
    return (int64_t)(ts.tv_sec * 1000 + ts.tv_nsec / 1000000);
}
int64_t datara_rt_now_ms(void) {
    return now_ms();
}
int64_t datara_rt_now_unix_ms(void) {
    struct timespec ts;
    clock_gettime(CLOCK_REALTIME, &ts);
    return (int64_t)(ts.tv_sec * 1000 + ts.tv_nsec / 1000000);
}
int64_t datara_rt_now_precise_ms(void) {
    struct timespec ts;
    clock_gettime(CLOCK_MONOTONIC, &ts);
    return (int64_t)ts.tv_sec * 1000000LL + (int64_t)(ts.tv_nsec / 1000LL);
}
int64_t now_ns(void) {
    struct timespec ts;
    clock_gettime(CLOCK_MONOTONIC, &ts);
    return (int64_t)ts.tv_sec * 1000000000LL + ts.tv_nsec;
}
int64_t datara_rt_now_ns(void) {
    return now_ns();
}
#endif

int64_t datara_rt_file_write(const char* path, const char* content) {
    if (!path || !content) return 0;
    FILE* f = fopen(path, "wb");
    if (!f) return 0;
    size_t len = strlen(content);
    size_t written = fwrite(content, 1, len, f);
    fclose(f);
    return written == len ? 1 : 0;
}

int64_t datara_rt_file_append(const char* path, const char* content) {
    if (!path || !content) return 0;
    FILE* f = fopen(path, "ab");
    if (!f) return 0;
    size_t len = strlen(content);
    size_t written = fwrite(content, 1, len, f);
    fclose(f);
    return written == len ? 1 : 0;
}

const char* datara_rt_file_read(const char* path) {
    if (!path) return "";
    FILE* f = fopen(path, "rb");
    if (!f) return "";
    fseek(f, 0, SEEK_END);
    long sz = ftell(f);
    if (sz < 0 || sz > 1024L * 1024 * 1024) { fclose(f); return ""; }
    fseek(f, 0, SEEK_SET);
    char* buf = (char*)malloc((size_t)sz + 1);
    if (!buf) { fclose(f); return ""; }
    size_t read_bytes = fread(buf, 1, (size_t)sz, f);
    buf[read_bytes] = '\0';
    fclose(f);
    return buf;
}

int64_t datara_rt_file_exists(const char* path) {
    if (!path) return 0;
    FILE* f = fopen(path, "rb");
    if (f) {
        fclose(f);
        return 1;
    }
    return 0;
}

void* datara_rt_sys_caps_create(void) {
    return (void*)0xCAFE;
}

void* datara_rt_files_grant_readonly(void* prov, const char* path) {
    (void)prov;
    return (void*)path;
}

void* datara_rt_files_grant_readwrite(void* prov, const char* path) {
    (void)prov;
    return (void*)path;
}

void* datara_rt_net_grant_connect(void* prov, const char* host, int64_t port) {
    (void)prov;
    (void)host;
    (void)port;
    return (void*)0xCAFE;
}

void* datara_rt_file_open(void* token, const char* path) {
    (void)token;
    return (void*)path;
}

const char* datara_rt_file_read_all(void* handle) {
    if (!handle) return "";
    return datara_rt_file_read((const char*)handle);
}

int64_t datara_rt_file_close(void* handle) {
    (void)handle;
    return 0;
}

void datara_rt_sleep(int64_t ms) {
    if (ms <= 0) return;
#ifdef _WIN32
    Sleep((DWORD)ms);
#else
    struct timespec ts;
    ts.tv_sec = ms / 1000;
    ts.tv_nsec = (ms % 1000) * 1000000;
    nanosleep(&ts, NULL);
#endif
}

const char* datara_rt_env_get(const char* key) {
    if (!key) return "";
    const char* val = getenv(key);
    return val ? val : "";
}

const char* datara_rt_path_join(const char* a, const char* b) {
    if (!a && !b) return "";
    if (!a || strlen(a) == 0) return b ? b : "";
    if (!b || strlen(b) == 0) return a ? a : "";
    size_t la = strlen(a);
    size_t lb = strlen(b);
    int needs_sep = (a[la - 1] != '/' && a[la - 1] != '\\' && b[0] != '/' && b[0] != '\\');
    size_t total = la + (needs_sep ? 1 : 0) + lb + 1;
    char* buf = datara_scratch_alloc(total);
    if (!buf) return "";
    memcpy(buf, a, la);
    size_t pos = la;
    if (needs_sep) {
#ifdef _WIN32
        buf[pos++] = '\\';
#else
        buf[pos++] = '/';
#endif
    }
    memcpy(buf + pos, b, lb);
    buf[pos + lb] = '\0';
    return buf;
}

static int g_argc = 0;
static char** g_argv = NULL;

// Set to 1 when datara_rt_random_bytes falls back to the clock-seeded LCG
// instead of the OS CSPRNG; exposed via datara_rt_rng_is_insecure().
static int g_datara_rng_insecure = 0;

void datara_rt_set_args(int argc, char** argv) {
    g_argc = argc;
    g_argv = argv;
}

int64_t datara_rt_args_count(void) {
    return (int64_t)g_argc;
}

const char* datara_rt_args_get(int64_t idx) {
    if (idx < 0 || idx >= g_argc || !g_argv) return "";
    return g_argv[idx];
}

int64_t datara_rt_str_contains(const char* s, const char* sub) {
    if (!s || !sub) return 0;
    return strstr(s, sub) != NULL ? 1 : 0;
}

int64_t datara_rt_str_starts_with(const char* s, const char* prefix) {
    if (!s || !prefix) return 0;
    size_t len_s = strlen(s);
    size_t len_p = strlen(prefix);
    if (len_s < len_p) return 0;
    return strncmp(s, prefix, len_p) == 0 ? 1 : 0;
}

int64_t datara_rt_str_ends_with(const char* s, const char* suffix) {
    if (!s || !suffix) return 0;
    size_t len_s = strlen(s);
    size_t len_suf = strlen(suffix);
    if (len_s < len_suf) return 0;
    return strcmp(s + len_s - len_suf, suffix) == 0 ? 1 : 0;
}

int64_t datara_rt_str_index_of(const char* s, const char* sub) {
    if (!s || !sub) return -1;
    const char* p = strstr(s, sub);
    if (!p) return -1;
    return (int64_t)(p - s);
}

const char* datara_rt_str_trim(const char* s) {
    if (!s) return "";
    while (*s == ' ' || *s == '\t' || *s == '\r' || *s == '\n') {
        s++;
    }
    if (*s == '\0') return "";
    size_t len = strlen(s);
    while (len > 0 && (s[len - 1] == ' ' || s[len - 1] == '\t' || s[len - 1] == '\r' || s[len - 1] == '\n')) {
        len--;
    }
    char* buf = (char*)malloc(len + 1);
    if (!buf) return "";
    memcpy(buf, s, len);
    buf[len] = '\0';
    return buf;
}

int64_t datara_rt_str_to_int(const char* s) {
    if (!s) return 0;
    return (int64_t)atoll(s);
}

double datara_rt_str_to_float(const char* s) {
    if (!s) return 0.0;
    return atof(s);
}

const char* datara_rt_str_substring(const char* s, int64_t start, int64_t len) {
    if (!s || start < 0 || len <= 0) return "";
    int64_t total_len = (int64_t)strlen(s);
    if (start >= total_len) return "";
    if (start + len > total_len) {
        len = total_len - start;
    }
    char* buf = datara_scratch_alloc((size_t)len);
    if (!buf) return "";
    memcpy(buf, s + start, (size_t)len);
    buf[len] = '\0';
    return buf;
}

int64_t datara_rt_str_chars(const char* s) {
    if (!s) return 0;
    int64_t count = 0;
    const unsigned char* p = (const unsigned char*)s;
    while (*p) {
        if ((*p & 0xC0) != 0x80) {
            count++;
        }
        p++;
    }
    return count;
}

int32_t datara_rt_validate_utf8(const char* s) {
    if (!s) return 1;
    const unsigned char* bytes = (const unsigned char*)s;
    size_t i = 0;
    size_t len = strlen(s);
    while (i < len) {
        unsigned char b0 = bytes[i];
        if (b0 <= 0x7F) {
            i += 1;
        } else if (b0 >= 0xC2 && b0 <= 0xDF) {
            if (i + 1 >= len) return 0;
            unsigned char b1 = bytes[i + 1];
            if ((b1 & 0xC0) != 0x80) return 0;
            i += 2;
        } else if (b0 == 0xE0) {
            if (i + 2 >= len) return 0;
            unsigned char b1 = bytes[i + 1];
            unsigned char b2 = bytes[i + 2];
            if (b1 < 0xA0 || b1 > 0xBF || (b2 & 0xC0) != 0x80) return 0;
            i += 3;
        } else if (b0 >= 0xE1 && b0 <= 0xEC) {
            if (i + 2 >= len) return 0;
            unsigned char b1 = bytes[i + 1];
            unsigned char b2 = bytes[i + 2];
            if ((b1 & 0xC0) != 0x80 || (b2 & 0xC0) != 0x80) return 0;
            i += 3;
        } else if (b0 == 0xED) {
            if (i + 2 >= len) return 0;
            unsigned char b1 = bytes[i + 1];
            unsigned char b2 = bytes[i + 2];
            if (b1 < 0x80 || b1 > 0x9F || (b2 & 0xC0) != 0x80) return 0;
            i += 3;
        } else if (b0 >= 0xEE && b0 <= 0xEF) {
            if (i + 2 >= len) return 0;
            unsigned char b1 = bytes[i + 1];
            unsigned char b2 = bytes[i + 2];
            if ((b1 & 0xC0) != 0x80 || (b2 & 0xC0) != 0x80) return 0;
            i += 3;
        } else if (b0 == 0xF0) {
            if (i + 3 >= len) return 0;
            unsigned char b1 = bytes[i + 1];
            unsigned char b2 = bytes[i + 2];
            unsigned char b3 = bytes[i + 3];
            if (b1 < 0x90 || b1 > 0xBF || (b2 & 0xC0) != 0x80 || (b3 & 0xC0) != 0x80) return 0;
            i += 4;
        } else if (b0 >= 0xF1 && b0 <= 0xF3) {
            if (i + 3 >= len) return 0;
            unsigned char b1 = bytes[i + 1];
            unsigned char b2 = bytes[i + 2];
            unsigned char b3 = bytes[i + 3];
            if ((b1 & 0xC0) != 0x80 || (b2 & 0xC0) != 0x80 || (b3 & 0xC0) != 0x80) return 0;
            i += 4;
        } else if (b0 == 0xF4) {
            if (i + 3 >= len) return 0;
            unsigned char b1 = bytes[i + 1];
            unsigned char b2 = bytes[i + 2];
            unsigned char b3 = bytes[i + 3];
            if (b1 < 0x80 || b1 > 0x8F || (b2 & 0xC0) != 0x80 || (b3 & 0xC0) != 0x80) return 0;
            i += 4;
        } else {
            return 0;
        }
    }
    return 1;
}

int64_t datara_rt_char_len(const char* s) {
    return datara_rt_str_chars(s);
}

const char* datara_rt_str_sanitize_utf8(const char* s) {
    if (!s) return "";
    if (datara_rt_validate_utf8(s)) return s;
    size_t len = strlen(s);
    char* buf = datara_scratch_alloc(len * 3 + 1);
    if (!buf) return "";
    size_t out_idx = 0;
    const unsigned char* p = (const unsigned char*)s;
    size_t i = 0;
    while (i < len) {
        unsigned char b0 = p[i];
        size_t seq_len = 0;
        if (b0 <= 0x7F) seq_len = 1;
        else if (b0 >= 0xC2 && b0 <= 0xDF && i + 1 < len && (p[i+1] & 0xC0) == 0x80) seq_len = 2;
        else if (b0 == 0xE0 && i + 2 < len && p[i+1] >= 0xA0 && p[i+1] <= 0xBF && (p[i+2] & 0xC0) == 0x80) seq_len = 3;
        else if (b0 >= 0xE1 && b0 <= 0xEC && i + 2 < len && (p[i+1] & 0xC0) == 0x80 && (p[i+2] & 0xC0) == 0x80) seq_len = 3;
        else if (b0 == 0xED && i + 2 < len && p[i+1] >= 0x80 && p[i+1] <= 0x9F && (p[i+2] & 0xC0) == 0x80) seq_len = 3;
        else if (b0 >= 0xEE && b0 <= 0xEF && i + 2 < len && (p[i+1] & 0xC0) == 0x80 && (p[i+2] & 0xC0) == 0x80) seq_len = 3;
        else if (b0 == 0xF0 && i + 3 < len && p[i+1] >= 0x90 && p[i+1] <= 0xBF && (p[i+2] & 0xC0) == 0x80 && (p[i+3] & 0xC0) == 0x80) seq_len = 4;
        else if (b0 >= 0xF1 && b0 <= 0xF3 && i + 3 < len && (p[i+1] & 0xC0) == 0x80 && (p[i+2] & 0xC0) == 0x80 && (p[i+3] & 0xC0) == 0x80) seq_len = 4;
        else if (b0 == 0xF4 && i + 3 < len && p[i+1] >= 0x80 && p[i+1] <= 0x8F && (p[i+2] & 0xC0) == 0x80 && (p[i+3] & 0xC0) == 0x80) seq_len = 4;

        if (seq_len > 0) {
            for (size_t k = 0; k < seq_len; k++) {
                buf[out_idx++] = (char)p[i + k];
            }
            i += seq_len;
        } else {
            buf[out_idx++] = (char)0xEF;
            buf[out_idx++] = (char)0xBF;
            buf[out_idx++] = (char)0xBD;
            i += 1;
        }
    }
    buf[out_idx] = '\0';
    return buf;
}

const char* datara_rt_str_next_scalar(const char* s, int64_t* inout_offset) {
    if (!s || !inout_offset || *inout_offset < 0) return "";
    size_t slen = strlen(s);
    if ((size_t)*inout_offset >= slen) return "";

    const unsigned char* p = (const unsigned char*)(s + *inout_offset);
    size_t char_len = 1;
    if (*p < 0x80) char_len = 1;
    else if ((*p & 0xE0) == 0xC0) char_len = 2;
    else if ((*p & 0xF0) == 0xE0) char_len = 3;
    else if ((*p & 0xF8) == 0xF0) char_len = 4;

    if ((size_t)*inout_offset + char_len > slen) {
        char_len = slen - (size_t)*inout_offset;
    }

    char* buf = datara_scratch_alloc(char_len);
    if (!buf) return "";
    memcpy(buf, p, char_len);
    buf[char_len] = '\0';
    *inout_offset += (int64_t)char_len;
    return buf;
}

const char* datara_rt_str_scalar_at(const char* s, int64_t offset) {
    if (!s || offset < 0) return "";
    size_t slen = strlen(s);
    if ((size_t)offset >= slen) return "";

    const unsigned char* p = (const unsigned char*)(s + offset);
    size_t char_len = 1;
    if (*p < 0x80) char_len = 1;
    else if ((*p & 0xE0) == 0xC0) char_len = 2;
    else if ((*p & 0xF0) == 0xE0) char_len = 3;
    else if ((*p & 0xF8) == 0xF0) char_len = 4;

    if ((size_t)offset + char_len > slen) {
        char_len = slen - (size_t)offset;
    }

    char* buf = datara_scratch_alloc(char_len);
    if (!buf) return "";
    memcpy(buf, p, char_len);
    buf[char_len] = '\0';
    return buf;
}

int64_t datara_rt_str_next_offset(const char* s, int64_t current_offset) {
    if (!s || current_offset < 0) return 0;
    size_t slen = strlen(s);
    if ((size_t)current_offset >= slen) return (int64_t)slen;

    const unsigned char* p = (const unsigned char*)(s + current_offset);
    size_t char_len = 1;
    if (*p < 0x80) char_len = 1;
    else if ((*p & 0xE0) == 0xC0) char_len = 2;
    else if ((*p & 0xF0) == 0xE0) char_len = 3;
    else if ((*p & 0xF8) == 0xF0) char_len = 4;

    if ((size_t)current_offset + char_len > slen) {
        char_len = slen - (size_t)current_offset;
    }
    return current_offset + (int64_t)char_len;
}

int64_t datara_rt_str_byte_at(const char* s, int64_t idx) {
    if (!s || idx < 0) return -1;
    size_t slen = strlen(s);
    if ((size_t)idx >= slen) return -1;
    return (int64_t)((const unsigned char*)s)[idx];
}

const char* datara_rt_str_char_at(const char* s, int64_t idx) {
    if (!s || idx < 0) return "";
    size_t slen = strlen(s);
    const unsigned char* p = (const unsigned char*)s;
    int64_t cur_char = 0;
    size_t byte_pos = 0;

    while (byte_pos < slen) {
        size_t char_len = 1;
        unsigned char b = p[byte_pos];
        if (b < 0x80) char_len = 1;
        else if ((b & 0xE0) == 0xC0) char_len = 2;
        else if ((b & 0xF0) == 0xE0) char_len = 3;
        else if ((b & 0xF8) == 0xF0) char_len = 4;

        if (byte_pos + char_len > slen) {
            char_len = slen - byte_pos;
        }

        if (cur_char == idx) {
            char* buf = datara_scratch_alloc(char_len);
            if (!buf) return "";
            memcpy(buf, s + byte_pos, char_len);
            buf[char_len] = '\0';
            return buf;
        }

        cur_char++;
        byte_pos += char_len;
    }
    return "";
}

const char* datara_rt_str_repeat(const char* s, int64_t count) {
    if (!s || count <= 0) return "";
    size_t slen = strlen(s);
    if (slen == 0) return "";
    if (count != 0 && slen > SIZE_MAX / (size_t)count) return NULL;
    size_t total = slen * (size_t)count;
    char* buf = datara_scratch_alloc(total);
    if (!buf) return "";
    char* p = buf;
    for (int64_t i = 0; i < count; i++) {
        memcpy(p, s, slen);
        p += slen;
    }
    *p = '\0';
    return buf;
}

const char* datara_rt_str_pad_left(const char* s, int64_t total_len, const char* pad) {
    if (!s) s = "";
    if (!pad || pad[0] == '\0') pad = " ";
    int64_t slen = (int64_t)strlen(s);
    if (slen >= total_len) return s;
    int64_t pad_needed = total_len - slen;
    size_t pad_len = strlen(pad);
    char* buf = datara_scratch_alloc((size_t)total_len);
    if (!buf) return s;
    char* p = buf;
    int64_t rem = pad_needed;
    while (rem > 0) {
        size_t take = (size_t)rem < pad_len ? (size_t)rem : pad_len;
        memcpy(p, pad, take);
        p += take;
        rem -= (int64_t)take;
    }
    memcpy(p, s, (size_t)slen);
    p += slen;
    *p = '\0';
    return buf;
}

const char* datara_rt_str_pad_right(const char* s, int64_t total_len, const char* pad) {
    if (!s) s = "";
    if (!pad || pad[0] == '\0') pad = " ";
    int64_t slen = (int64_t)strlen(s);
    if (slen >= total_len) return s;
    int64_t pad_needed = total_len - slen;
    size_t pad_len = strlen(pad);
    char* buf = datara_scratch_alloc((size_t)total_len);
    if (!buf) return s;
    char* p = buf;
    memcpy(p, s, (size_t)slen);
    p += slen;
    int64_t rem = pad_needed;
    while (rem > 0) {
        size_t take = (size_t)rem < pad_len ? (size_t)rem : pad_len;
        memcpy(p, pad, take);
        p += take;
        rem -= (int64_t)take;
    }
    *p = '\0';
    return buf;
}

const char* datara_rt_str_replace(const char* s, const char* target, const char* replacement) {
    if (!s) return "";
    if (!target || target[0] == '\0') return s;
    if (!replacement) replacement = "";
    size_t slen = strlen(s);
    size_t tlen = strlen(target);
    size_t rlen = strlen(replacement);

    size_t count = 0;
    const char* p = s;
    while ((p = strstr(p, target)) != NULL) {
        count++;
        p += tlen;
    }
    if (count == 0) return s;

    size_t new_len = slen + count * (rlen > tlen ? (rlen - tlen) : 0);
    char* buf = datara_scratch_alloc(new_len);
    if (!buf) return s;

    char* dst = buf;
    p = s;
    const char* match;
    while ((match = strstr(p, target)) != NULL) {
        size_t seg = (size_t)(match - p);
        memcpy(dst, p, seg);
        dst += seg;
        memcpy(dst, replacement, rlen);
        dst += rlen;
        p = match + tlen;
    }
    size_t rest = strlen(p);
    memcpy(dst, p, rest);
    dst[rest] = '\0';
    return buf;
}

const char* datara_rt_str_to_upper(const char* s) {
    if (!s) return "";
    size_t len = strlen(s);
    char* buf = datara_scratch_alloc(len);
    if (!buf) return "";
    for (size_t i = 0; i < len; i++) {
        char c = s[i];
        if (c >= 'a' && c <= 'z') c = (char)(c - ('a' - 'A'));
        buf[i] = c;
    }
    buf[len] = '\0';
    return buf;
}

const char* datara_rt_str_to_lower(const char* s) {
    if (!s) return "";
    size_t len = strlen(s);
    char* buf = datara_scratch_alloc(len);
    if (!buf) return "";
    for (size_t i = 0; i < len; i++) {
        char c = s[i];
        if (c >= 'A' && c <= 'Z') c = (char)(c + ('a' - 'A'));
        buf[i] = c;
    }
    buf[len] = '\0';
    return buf;
}

int64_t* datara_rt_str_split(const char* s, const char* delim) {
    if (!s) return datara_rt_list_create(0);
    if (!delim || delim[0] == '\0') {
        int64_t* list = datara_rt_list_create_capacity(1);
        list = datara_rt_list_append(list, (int64_t)s);
        return list;
    }
    size_t delim_len = strlen(delim);
    int64_t* list = datara_rt_list_create_capacity(8);
    const char* cur = s;
    const char* found;
    while ((found = strstr(cur, delim)) != NULL) {
        size_t part_len = (size_t)(found - cur);
        char* part = datara_scratch_alloc(part_len);
        if (part) {
            memcpy(part, cur, part_len);
            part[part_len] = '\0';
            list = datara_rt_list_append(list, (int64_t)part);
        }
        cur = found + delim_len;
    }
    size_t rem_len = strlen(cur);
    char* rem = datara_scratch_alloc(rem_len);
    if (rem) {
        memcpy(rem, cur, rem_len);
        rem[rem_len] = '\0';
        list = datara_rt_list_append(list, (int64_t)rem);
    }
    return list;
}

const char* datara_rt_str_join(const int64_t* list, const char* delim) {
    if (!list) return "";
    int64_t count = datara_rt_list_len((int64_t*)list);
    if (count == 0) return "";
    if (!delim) delim = "";
    size_t delim_len = strlen(delim);

    size_t total_len = 0;
    for (int64_t i = 0; i < count; i++) {
        const char* item = (const char*)datara_rt_list_get((int64_t*)list, i);
        if (item) {
            total_len += strlen(item);
        }
        if (i + 1 < count) {
            total_len += delim_len;
        }
    }

    char* res = datara_scratch_alloc(total_len);
    if (!res) return "";
    char* p = res;
    for (int64_t i = 0; i < count; i++) {
        const char* item = (const char*)datara_rt_list_get((int64_t*)list, i);
        if (item) {
            size_t ilen = strlen(item);
            memcpy(p, item, ilen);
            p += ilen;
        }
        if (i + 1 < count && delim_len > 0) {
            memcpy(p, delim, delim_len);
            p += delim_len;
        }
    }
    *p = '\0';
    return res;
}

const char* datara_rt_format_percent(double val, int64_t decimals) {
    if (decimals < 0) decimals = 0;
    if (decimals > 10) decimals = 10;
    char* buf = datara_scratch_alloc(32);
    if (!buf) return "";
    snprintf(buf, 32, "%.*f%%", (int)decimals, val * 100.0);
    return buf;
}

const char* datara_rt_format_int_with_commas(int64_t n) {
    char temp[32];
    snprintf(temp, sizeof(temp), "%lld", (long long)n);
    size_t len = strlen(temp);
    size_t digits_start = (temp[0] == '-') ? 1 : 0;
    size_t num_digits = len - digits_start;
    size_t commas = (num_digits > 0) ? (num_digits - 1) / 3 : 0;

    char* buf = datara_scratch_alloc(len + commas);
    if (!buf) return "";

    char* dst = buf;
    if (digits_start) {
        *dst++ = '-';
    }

    size_t first_group = num_digits % 3;
    if (first_group == 0) first_group = 3;

    const char* src = temp + digits_start;
    memcpy(dst, src, first_group);
    dst += first_group;
    src += first_group;

    while (*src) {
        *dst++ = ',';
        memcpy(dst, src, 3);
        dst += 3;
        src += 3;
    }
    *dst = '\0';
    return buf;
}

// ---------------------------------------------------------------------------
// Network Sockets (TCP/UDP)
// ---------------------------------------------------------------------------

static int g_wsa_initialized = 0;
static void datara_rt_ensure_sockets(void) {
#ifdef _WIN32
    if (!g_wsa_initialized) {
        WSADATA wsa;
        WSAStartup(MAKEWORD(2, 2), &wsa);
        g_wsa_initialized = 1;
    }
#endif
}

int64_t datara_rt_socket_create(int64_t is_tcp) {
    datara_rt_ensure_sockets();
    int type = is_tcp ? SOCK_STREAM : SOCK_DGRAM;
#ifdef _WIN32
    SOCKET s = socket(AF_INET, type, 0);
    if (s == INVALID_SOCKET) return -1;
    return (int64_t)s;
#else
    int s = socket(AF_INET, type, 0);
    if (s < 0) return -1;
    return (int64_t)s;
#endif
}

int64_t datara_rt_socket_bind(int64_t sock, const char* host, int64_t port) {
    if (sock < 0) return -1;
    struct sockaddr_in addr;
    memset(&addr, 0, sizeof(addr));
    addr.sin_family = AF_INET;
    addr.sin_port = htons((uint16_t)port);
    if (!host || strlen(host) == 0 || strcmp(host, "0.0.0.0") == 0) {
        addr.sin_addr.s_addr = INADDR_ANY;
    } else {
        addr.sin_addr.s_addr = inet_addr(host);
    }
    int opt = 1;
#ifdef _WIN32
    setsockopt((SOCKET)sock, SOL_SOCKET, SO_REUSEADDR, (const char*)&opt, sizeof(opt));
    if (bind((SOCKET)sock, (struct sockaddr*)&addr, sizeof(addr)) == SOCKET_ERROR) {
        return -1;
    }
#else
    setsockopt((int)sock, SOL_SOCKET, SO_REUSEADDR, &opt, sizeof(opt));
    if (bind((int)sock, (struct sockaddr*)&addr, sizeof(addr)) < 0) {
        return -1;
    }
#endif
    return 0;
}

int64_t datara_rt_socket_listen(int64_t sock, int64_t backlog) {
    if (sock < 0) return -1;
    int b = backlog > 0 ? (int)backlog : 128;
#ifdef _WIN32
    if (listen((SOCKET)sock, b) == SOCKET_ERROR) return -1;
#else
    if (listen((int)sock, b) < 0) return -1;
#endif
    return 0;
}

int64_t datara_rt_socket_accept(int64_t sock) {
    if (sock < 0) return -1;
#ifdef _WIN32
    SOCKET client = accept((SOCKET)sock, NULL, NULL);
    if (client == INVALID_SOCKET) return -1;
    return (int64_t)client;
#else
    int client = accept((int)sock, NULL, NULL);
    if (client < 0) return -1;
    return (int64_t)client;
#endif
}

int64_t datara_rt_socket_connect(int64_t sock, const char* host, int64_t port) {
    if (sock < 0 || !host) return -1;
    char port_str[16];
    snprintf(port_str, sizeof(port_str), "%u", (unsigned int)port);
    struct addrinfo hints, *res = NULL;
    memset(&hints, 0, sizeof(hints));
    hints.ai_family = AF_UNSPEC;
    hints.ai_socktype = SOCK_STREAM;
    if (getaddrinfo(host, port_str, &hints, &res) != 0 || !res) {
        return -1;
    }
    int connect_res = -1;
    for (struct addrinfo* p = res; p != NULL; p = p->ai_next) {
#ifdef _WIN32
        if (connect((SOCKET)sock, p->ai_addr, (int)p->ai_addrlen) != SOCKET_ERROR) {
            connect_res = 0;
            break;
        }
#else
        if (connect((int)sock, p->ai_addr, p->ai_addrlen) == 0) {
            connect_res = 0;
            break;
        }
#endif
    }
    freeaddrinfo(res);
    return connect_res;
}

int64_t datara_rt_socket_send(int64_t sock, const char* data) {
    if (sock < 0 || !data) return -1;
    int len = (int)strlen(data);
#ifdef _WIN32
    int sent = send((SOCKET)sock, data, len, 0);
    return sent == SOCKET_ERROR ? -1 : (int64_t)sent;
#else
    ssize_t sent = send((int)sock, data, len, 0);
    return sent < 0 ? -1 : (int64_t)sent;
#endif
}

const char* datara_rt_socket_recv(int64_t sock, int64_t max_bytes) {
    if (sock < 0) return "";
    int cap = max_bytes > 0 ? (int)max_bytes : 4096;
    char* buf = (char*)malloc(cap + 1);
    if (!buf) return "";
#ifdef _WIN32
    int n = recv((SOCKET)sock, buf, cap, 0);
#else
    ssize_t n = recv((int)sock, buf, cap, 0);
#endif
    if (n <= 0) {
        free(buf);
        return "";
    }
    char* scratch = datara_scratch_alloc((size_t)n);
    if (scratch) {
        memcpy(scratch, buf, n);
        scratch[n] = '\0';
        free(buf);
        return scratch;
    }
    buf[n] = '\0';
    return buf;
}

void datara_rt_socket_close(int64_t sock) {
    if (sock < 0) return;
#ifdef _WIN32
    closesocket((SOCKET)sock);
#else
    close((int)sock);
#endif
}

// ---------------------------------------------------------------------------
// HTTP (blocking HTTP/1.1 GET over the socket primitives above)
// ---------------------------------------------------------------------------

/* Upper bound on a downloaded body, mirroring the runtime's other
 * bounded-resource conventions (scratch ring, socket recv caps). */
#define DATARA_HTTP_MAX_BODY (16 * 1024 * 1024)

static const char* datara_http_error(const char* msg) {
    size_t prefix_len = strlen("HTTP_ERROR: ");
    size_t msg_len = msg ? strlen(msg) : 0;
    char* buf = datara_scratch_alloc(prefix_len + msg_len);
    if (!buf) return "HTTP_ERROR: out of memory";
    memcpy(buf, "HTTP_ERROR: ", prefix_len);
    memcpy(buf + prefix_len, msg, msg_len);
    buf[prefix_len + msg_len] = '\0';
    return buf;
}

/* Case-insensitive comparison of n bytes. */
static int datara_http_ci_eq(const char* a, const char* b, size_t n) {
    for (size_t i = 0; i < n; i++) {
        char ca = a[i], cb = b[i];
        if (ca >= 'A' && ca <= 'Z') ca = (char)(ca + 32);
        if (cb >= 'A' && cb <= 'Z') cb = (char)(cb + 32);
        if (ca != cb) return 0;
    }
    return 1;
}

/* Case-insensitive search for `needle` inside the bounded region
 * [hay, hay+hay_len). Returns a pointer to the match or NULL. */
static const char* datara_http_ci_find(const char* hay, size_t hay_len,
                                       const char* needle) {
    size_t nlen = strlen(needle);
    if (nlen == 0 || hay_len < nlen) return NULL;
    for (size_t i = 0; i + nlen <= hay_len; i++) {
        if (datara_http_ci_eq(hay + i, needle, nlen)) return hay + i;
    }
    return NULL;
}

/* Find a header line by name (case-insensitive) inside the header block
 * [hdr, hdr+hdr_len). On success returns a pointer to the value (after the
 * colon, leading whitespace skipped) and sets *vlen to its length;
 * returns NULL when the header is absent. */
static const char* datara_http_header_value(const char* hdr, size_t hdr_len,
                                            const char* name, size_t* vlen) {
    size_t nlen = strlen(name);
    size_t i = 0;
    while (i < hdr_len) {
        size_t eol = i;
        while (eol < hdr_len && hdr[eol] != '\n') eol++;
        size_t line_len = eol - i;
        if (line_len > 0 && hdr[i + line_len - 1] == '\r') line_len--;
        if (line_len > nlen + 1 && hdr[i + nlen] == ':'
            && datara_http_ci_eq(hdr + i, name, nlen)) {
            size_t v = i + nlen + 1;
            while (v < i + line_len && (hdr[v] == ' ' || hdr[v] == '\t')) v++;
            *vlen = (i + line_len) - v;
            return hdr + v;
        }
        i = eol + 1;
    }
    return NULL;
}

/* Decode an HTTP/1.1 chunked body in place: reads hex chunk-size lines
 * (ignoring chunk extensions after ';'), copies each chunk's bytes down,
 * skips the trailing CRLF, and stops at the 0-size terminating chunk
 * (trailers are ignored). Returns the decoded length, or (size_t)-1 on
 * malformed input / when the total exceeds DATARA_HTTP_MAX_BODY. */
static size_t datara_http_decode_chunked(char* body, size_t body_len) {
    size_t rd = 0, wr = 0;
    for (;;) {
        size_t line_end = rd;
        while (line_end < body_len && body[line_end] != '\n') line_end++;
        if (line_end >= body_len) return (size_t)-1;
        size_t line_len = line_end - rd;
        if (line_len > 0 && body[line_end - 1] == '\r') line_len--;
        size_t size = 0;
        size_t k = 0;
        while (k < line_len) {
            char c = body[rd + k];
            int d;
            if (c >= '0' && c <= '9') d = c - '0';
            else if (c >= 'a' && c <= 'f') d = c - 'a' + 10;
            else if (c >= 'A' && c <= 'F') d = c - 'A' + 10;
            else break; /* chunk extensions (';...') or trailing junk */
            size = size * 16 + (size_t)d;
            if (size > DATARA_HTTP_MAX_BODY) return (size_t)-1;
            k++;
        }
        if (k == 0) return (size_t)-1; /* no hex digits at all */
        rd = line_end + 1;
        if (size == 0) break; /* final chunk; ignore trailers */
        if (size > body_len - rd) return (size_t)-1;
        if (wr + size > DATARA_HTTP_MAX_BODY) return (size_t)-1;
        memmove(body + wr, body + rd, size);
        wr += size;
        rd += size;
        /* Skip the CRLF (tolerate a bare LF) after the chunk data. */
        if (rd < body_len && body[rd] == '\r') rd++;
        if (rd < body_len && body[rd] == '\n') {
            rd++;
        } else if (size > 0 && rd < body_len && body[rd - 1] != '\n') {
            return (size_t)-1;
        }
    }
    return wr;
}

const char* datara_rt_http_get(const char* url) {
    if (!url || !url[0]) {
        return datara_http_error("empty url");
    }
    datara_rt_ensure_sockets();

    // Parse http://host[:port]/path — https is explicitly unsupported.
    if (strncmp(url, "http://", 7) != 0) {
        if (strncmp(url, "https://", 8) == 0) {
            return datara_http_error("https not supported");
        }
        return datara_http_error("invalid url (expected http://...)");
    }
    const char* host_start = url + 7;
    const char* path_start = strchr(host_start, '/');
    const char* path = path_start ? path_start : "/";
    size_t host_len = path_start ? (size_t)(path_start - host_start) : strlen(host_start);
    if (host_len == 0 || host_len >= 256) {
        return datara_http_error("invalid host");
    }
    char host[256];
    memcpy(host, host_start, host_len);
    host[host_len] = '\0';
    int port = 80;
    char* colon = strchr(host, ':');
    if (colon) {
        port = atoi(colon + 1);
        *colon = '\0';
        if (port <= 0 || port > 65535) {
            return datara_http_error("invalid port");
        }
    }
    if (!host[0]) {
        return datara_http_error("invalid host");
    }

    int64_t sock = datara_rt_socket_create(1);
    if (sock < 0) {
        return datara_http_error("socket creation failed");
    }
    if (datara_rt_socket_connect(sock, host, (int64_t)port) != 0) {
        datara_rt_socket_close(sock);
        return datara_http_error("connection failed");
    }

    char req[2048];
    int rl = snprintf(req, sizeof(req),
                      "GET %s HTTP/1.1\r\nHost: %s\r\nConnection: close\r\n\r\n",
                      path, host);
    if (rl <= 0 || (size_t)rl >= sizeof(req)) {
        datara_rt_socket_close(sock);
        return datara_http_error("request too long");
    }
    size_t total_sent = 0;
    while (total_sent < (size_t)rl) {
#ifdef _WIN32
        int n = send((SOCKET)sock, req + total_sent, (int)((size_t)rl - total_sent), 0);
        if (n == SOCKET_ERROR) {
#else
        ssize_t n = send((int)sock, req + total_sent, (size_t)rl - total_sent, 0);
        if (n <= 0) {
#endif
            datara_rt_socket_close(sock);
            return datara_http_error("send failed");
        }
        total_sent += (size_t)n;
    }

    // Read the full response until EOF.
    size_t cap = 64 * 1024;
    size_t len = 0;
    char* raw = (char*)malloc(cap);
    if (!raw) {
        datara_rt_socket_close(sock);
        return datara_http_error("out of memory");
    }
    for (;;) {
        if (len + 8192 + 1 > cap) {
            if (cap >= DATARA_HTTP_MAX_BODY) {
                free(raw);
                datara_rt_socket_close(sock);
                return datara_http_error("response too large");
            }
            cap *= 2;
            char* grown = (char*)realloc(raw, cap);
            if (!grown) {
                free(raw);
                datara_rt_socket_close(sock);
                return datara_http_error("out of memory");
            }
            raw = grown;
        }
#ifdef _WIN32
        int n = recv((SOCKET)sock, raw + len, 8192, 0);
#else
        ssize_t n = recv((int)sock, raw + len, 8192, 0);
#endif
        if (n == 0) break;
        if (n < 0) {
            free(raw);
            datara_rt_socket_close(sock);
            return datara_http_error("recv failed");
        }
        len += (size_t)n;
    }
    datara_rt_socket_close(sock);

    // Skip headers up to \r\n\r\n (tolerate bare \n\n for lenient servers).
    char* body = NULL;
    for (size_t i = 0; i + 3 < len; i++) {
        if (raw[i] == '\r' && raw[i + 1] == '\n' && raw[i + 2] == '\r' && raw[i + 3] == '\n') {
            body = raw + i + 4;
            break;
        }
    }
    if (!body) {
        for (size_t i = 0; i + 1 < len; i++) {
            if (raw[i] == '\n' && raw[i + 1] == '\n') {
                body = raw + i + 2;
                break;
            }
        }
    }
    if (!body) {
        free(raw);
        return datara_http_error("malformed response");
    }

    // Header block spans [raw, body); it includes the separator, which the
    // per-line header scanner tolerates.
    const char* hdr = raw;
    size_t hdr_len = (size_t)(body - raw);
    size_t body_len = len - hdr_len;

    size_t te_vlen = 0;
    const char* te = datara_http_header_value(hdr, hdr_len, "transfer-encoding", &te_vlen);
    if (te && datara_http_ci_find(te, te_vlen, "chunked")) {
        size_t decoded = datara_http_decode_chunked((char*)body, body_len);
        if (decoded == (size_t)-1) {
            free(raw);
            return datara_http_error("malformed chunked body");
        }
        body_len = decoded;
    } else {
        // Honor Content-Length when present: return exactly that many bytes
        // of the body. If the server sent fewer bytes than advertised
        // (early close), fall back to what actually arrived.
        size_t cl_vlen = 0;
        const char* cl = datara_http_header_value(hdr, hdr_len, "content-length", &cl_vlen);
        if (cl && cl_vlen > 0) {
            size_t declared = 0;
            size_t k = 0;
            for (; k < cl_vlen; k++) {
                char c = cl[k];
                if (c < '0' || c > '9') break;
                declared = declared * 10 + (size_t)(c - '0');
                if (declared > DATARA_HTTP_MAX_BODY) break;
            }
            if (k == cl_vlen && declared <= body_len && declared <= DATARA_HTTP_MAX_BODY) {
                body_len = declared;
            }
        }
    }

    char* out = datara_scratch_alloc(body_len);
    if (!out) {
        free(raw);
        return datara_http_error("out of memory");
    }
    memcpy(out, body, body_len);
    out[body_len] = '\0';
    free(raw);
    return out;
}

// ---------------------------------------------------------------------------
// High-Performance Fast Math
// ---------------------------------------------------------------------------
double datara_rt_math_sqrt(double x) { return sqrt(x); }
double datara_rt_math_pow(double base, double exp) { return pow(base, exp); }
double datara_rt_math_abs(double x) { return fabs(x); }
double datara_rt_math_sin(double x) { return sin(x); }
double datara_rt_math_cos(double x) { return cos(x); }
double datara_rt_math_tan(double x) { return tan(x); }
double datara_rt_math_floor(double x) { return floor(x); }
double datara_rt_math_ceil(double x) { return ceil(x); }
double datara_rt_math_round(double x) { return round(x); }
double datara_rt_math_min(double a, double b) { return fmin(a, b); }
double datara_rt_math_max(double a, double b) { return fmax(a, b); }
double datara_rt_math_clamp(double val, double min_val, double max_val) {
    if (val < min_val) return min_val;
    if (val > max_val) return max_val;
    return val;
}
double datara_rt_math_hypot(double a, double b) { return hypot(a, b); }
double datara_rt_math_log(double x) { return log(x); }
double datara_rt_math_exp(double x) { return exp(x); }
int64_t datara_rt_math_min_int(int64_t a, int64_t b) { return a < b ? a : b; }
int64_t datara_rt_math_max_int(int64_t a, int64_t b) { return a > b ? a : b; }
int64_t datara_rt_math_clamp_int(int64_t val, int64_t min_val, int64_t max_val) {
    if (val < min_val) return min_val;
    if (val > max_val) return max_val;
    return val;
}
int64_t datara_rt_math_abs_int(int64_t x) { return x < 0 ? -x : x; }
int64_t datara_rt_math_ctz(int64_t v) {
    if (v == 0) return 64;
#if defined(_MSC_VER) && !defined(__clang__)
    unsigned long idx;
    _BitScanForward64(&idx, (unsigned __int64)v);
    return (int64_t)idx;
#else
    return (int64_t)__builtin_ctzll((unsigned long long)v);
#endif
}
int64_t datara_rt_math_shr(int64_t v, int64_t s) { return v >> (s & 63); }
int64_t datara_rt_math_shl(int64_t v, int64_t s) { return (int64_t)((uint64_t)v << (s & 63)); }
int64_t datara_rt_math_xor(int64_t a, int64_t b) { return a ^ b; }
int64_t datara_rt_math_and(int64_t a, int64_t b) { return a & b; }
int64_t datara_rt_math_or(int64_t a, int64_t b) { return a | b; }

// ---------------------------------------------------------------------------
// Cryptography: SHA-256 & Base64
// ---------------------------------------------------------------------------

typedef struct {
    uint8_t data[64];
    uint32_t datalen;
    uint64_t bitlen;
    uint32_t state[8];
} DATARA_SHA256_CTX;

#define D_ROTR(a,b) (((a) >> (b)) | ((a) << (32-(b))))
#define D_SIG0(x) (D_ROTR(x,2) ^ D_ROTR(x,13) ^ D_ROTR(x,22))
#define D_SIG1(x) (D_ROTR(x,6) ^ D_ROTR(x,11) ^ D_ROTR(x,25))
#define D_sig0(x) (D_ROTR(x,7) ^ D_ROTR(x,18) ^ ((x) >> 3))
#define D_sig1(x) (D_ROTR(x,17) ^ D_ROTR(x,19) ^ ((x) >> 10))
#define D_CH(x,y,z) (((x) & (y)) ^ (~(x) & (z)))
#define D_MAJ(x,y,z) (((x) & (y)) ^ ((x) & (z)) ^ ((y) & (z)))

static const uint32_t K_SHA256[64] = {
    0x428a2f98,0x71374491,0xb5c0fbcf,0xe9b5dba5,0x3956c25b,0x59f111f1,0x923f82a4,0xab1c5ed5,
    0xd807aa98,0x12835b01,0x243185be,0x550c7dc3,0x72be5d74,0x80deb1fe,0x9bdc06a7,0xc19bf174,
    0xe49b69c1,0xefbe4786,0x0fc19dc6,0x240ca1cc,0x2de92c6f,0x4a7484aa,0x5cb0a9dc,0x76f988da,
    0x983e5152,0xa831c66d,0xb00327c8,0xbf597fc7,0xc6e00bf3,0xd5a79147,0x06ca6351,0x14292967,
    0x27b70a85,0x2e1b2138,0x4d2c6dfc,0x53380d13,0x650a7354,0x766a0abb,0x81c2c92e,0x92722c85,
    0xa2bfe8a1,0xa81a664b,0xc24b8b70,0xc76c51a3,0xd192e819,0xd6990624,0xf40e3585,0x106aa070,
    0x19a4c116,0x1e376c08,0x2748774c,0x34b0bcb5,0x391c0cb3,0x4ed8aa4a,0x5b9cca4f,0x682e6ff3,
    0x748f82ee,0x78a5636f,0x84c87814,0x8cc70208,0x90befffa,0xa4506ceb,0xbef9a3f7,0xc67178f2
};

static void datara_sha256_transform(DATARA_SHA256_CTX *ctx, const uint8_t data[]) {
    uint32_t a, b, c, d, e, f, g, h, i, j, t1, t2, m[64];
    for (i = 0, j = 0; i < 16; ++i, j += 4)
        m[i] = ((uint32_t)data[j] << 24) | ((uint32_t)data[j + 1] << 16) | ((uint32_t)data[j + 2] << 8) | ((uint32_t)data[j + 3]);
    for ( ; i < 64; ++i)
        m[i] = D_sig1(m[i - 2]) + m[i - 7] + D_sig0(m[i - 15]) + m[i - 16];
    a = ctx->state[0]; b = ctx->state[1]; c = ctx->state[2]; d = ctx->state[3];
    e = ctx->state[4]; f = ctx->state[5]; g = ctx->state[6]; h = ctx->state[7];
    for (i = 0; i < 64; ++i) {
        t1 = h + D_SIG1(e) + D_CH(e,f,g) + K_SHA256[i] + m[i];
        t2 = D_SIG0(a) + D_MAJ(a,b,c);
        h = g; g = f; f = e; e = d + t1;
        d = c; c = b; b = a; a = t1 + t2;
    }
    ctx->state[0] += a; ctx->state[1] += b; ctx->state[2] += c; ctx->state[3] += d;
    ctx->state[4] += e; ctx->state[5] += f; ctx->state[6] += g; ctx->state[7] += h;
}

static void datara_sha256_init(DATARA_SHA256_CTX *ctx) {
    ctx->datalen = 0;
    ctx->bitlen = 0;
    ctx->state[0] = 0x6a09e667; ctx->state[1] = 0xbb67ae85;
    ctx->state[2] = 0x3c6ef372; ctx->state[3] = 0xa54ff53a;
    ctx->state[4] = 0x510e527f; ctx->state[5] = 0x9b05688c;
    ctx->state[6] = 0x1f83d9ab; ctx->state[7] = 0x5be0cd19;
}

static void datara_sha256_update(DATARA_SHA256_CTX *ctx, const uint8_t data[], size_t len) {
    size_t i;
    for (i = 0; i < len; ++i) {
        ctx->data[ctx->datalen] = data[i];
        ctx->datalen++;
        if (ctx->datalen == 64) {
            datara_sha256_transform(ctx, ctx->data);
            ctx->bitlen += 512;
            ctx->datalen = 0;
        }
    }
}

static void datara_sha256_final(DATARA_SHA256_CTX *ctx, uint8_t hash[]) {
    uint32_t i = ctx->datalen;
    if (ctx->datalen < 56) {
        ctx->data[i++] = 0x80;
        while (i < 56) ctx->data[i++] = 0x00;
    } else {
        ctx->data[i++] = 0x80;
        while (i < 64) ctx->data[i++] = 0x00;
        datara_sha256_transform(ctx, ctx->data);
        memset(ctx->data, 0, 56);
    }
    ctx->bitlen += (uint64_t)ctx->datalen * 8;
    ctx->data[63] = (uint8_t)(ctx->bitlen);
    ctx->data[62] = (uint8_t)(ctx->bitlen >> 8);
    ctx->data[61] = (uint8_t)(ctx->bitlen >> 16);
    ctx->data[60] = (uint8_t)(ctx->bitlen >> 24);
    ctx->data[59] = (uint8_t)(ctx->bitlen >> 32);
    ctx->data[58] = (uint8_t)(ctx->bitlen >> 40);
    ctx->data[57] = (uint8_t)(ctx->bitlen >> 48);
    ctx->data[56] = (uint8_t)(ctx->bitlen >> 56);
    datara_sha256_transform(ctx, ctx->data);
    for (i = 0; i < 4; ++i) {
        hash[i]      = (uint8_t)((ctx->state[0] >> (24 - i * 8)) & 0x000000ff);
        hash[i + 4]  = (uint8_t)((ctx->state[1] >> (24 - i * 8)) & 0x000000ff);
        hash[i + 8]  = (uint8_t)((ctx->state[2] >> (24 - i * 8)) & 0x000000ff);
        hash[i + 12] = (uint8_t)((ctx->state[3] >> (24 - i * 8)) & 0x000000ff);
        hash[i + 16] = (uint8_t)((ctx->state[4] >> (24 - i * 8)) & 0x000000ff);
        hash[i + 20] = (uint8_t)((ctx->state[5] >> (24 - i * 8)) & 0x000000ff);
        hash[i + 24] = (uint8_t)((ctx->state[6] >> (24 - i * 8)) & 0x000000ff);
        hash[i + 28] = (uint8_t)((ctx->state[7] >> (24 - i * 8)) & 0x000000ff);
    }
}

const char* datara_rt_sha256(const char* input) {
    if (!input) return "";
    DATARA_SHA256_CTX ctx;
    datara_sha256_init(&ctx);
    datara_sha256_update(&ctx, (const uint8_t*)input, strlen(input));
    uint8_t hash[32];
    datara_sha256_final(&ctx, hash);

    char* hex = (char*)malloc(65);
    if (!hex) return "";
    for (int i = 0; i < 32; i++) {
        snprintf(hex + (i * 2), 3, "%02x", hash[i]);
    }
    hex[64] = '\0';
    return hex;
}

static const char B64_CHARS[] = "ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";

const char* datara_rt_base64_encode(const char* input) {
    if (!input) return "";
    size_t in_len = strlen(input);
    size_t out_len = 4 * ((in_len + 2) / 3);
    char* encoded = (char*)malloc(out_len + 1);
    if (!encoded) return "";

    size_t i, j = 0;
    for (i = 0; i < in_len; i += 3) {
        uint32_t octet_a = (uint8_t)input[i];
        uint32_t octet_b = (i + 1 < in_len) ? (uint8_t)input[i + 1] : 0;
        uint32_t octet_c = (i + 2 < in_len) ? (uint8_t)input[i + 2] : 0;

        uint32_t triple = (octet_a << 16) + (octet_b << 8) + octet_c;

        encoded[j++] = B64_CHARS[(triple >> 18) & 0x3F];
        encoded[j++] = B64_CHARS[(triple >> 12) & 0x3F];
        encoded[j++] = (i + 1 < in_len) ? B64_CHARS[(triple >> 6) & 0x3F] : '=';
        encoded[j++] = (i + 2 < in_len) ? B64_CHARS[triple & 0x3F] : '=';
    }
    encoded[out_len] = '\0';
    return encoded;
}

const char* datara_rt_base64_decode(const char* input) {
    if (!input) return "";
    size_t in_len = strlen(input);
    if (in_len % 4 != 0) return "";

    size_t out_len = in_len / 4 * 3;
    if (in_len > 0 && input[in_len - 1] == '=') out_len--;
    if (in_len > 1 && input[in_len - 2] == '=') out_len--;

    char* decoded = (char*)malloc(out_len + 1);
    if (!decoded) return "";

    static int b64_rev[256];
    static int rev_init = 0;
    if (!rev_init) {
        memset(b64_rev, -1, sizeof(b64_rev));
        for (int k = 0; k < 64; k++) b64_rev[(uint8_t)B64_CHARS[k]] = k;
        rev_init = 1;
    }

    size_t i, j = 0;
    for (i = 0; i < in_len; i += 4) {
        int a = b64_rev[(uint8_t)input[i]];
        int b = b64_rev[(uint8_t)input[i + 1]];
        int c = input[i + 2] == '=' ? 0 : b64_rev[(uint8_t)input[i + 2]];
        int d = input[i + 3] == '=' ? 0 : b64_rev[(uint8_t)input[i + 3]];

        if (a < 0 || b < 0 || c < 0 || d < 0) { free(decoded); return ""; }

        uint32_t triple = (a << 18) | (b << 12) | (c << 6) | d;
        if (j < out_len) decoded[j++] = (triple >> 16) & 0xFF;
        if (j < out_len) decoded[j++] = (triple >> 8) & 0xFF;
        if (j < out_len) decoded[j++] = triple & 0xFF;
    }
    decoded[out_len] = '\0';
    return decoded;
}

int64_t datara_rt_random_bytes(uint8_t* buf, int64_t len) {
    if (!buf || len <= 0) return 0;
#ifdef _WIN32
    typedef unsigned char (__stdcall *DataraRtlGenRandomFn)(void*, unsigned long);
    static DataraRtlGenRandomFn s_pfnRtlGenRandom = NULL;
    static int s_rng_init = 0;
    if (!s_rng_init) {
        HMODULE hAdvApi = LoadLibraryA("advapi32.dll");
        if (hAdvApi) {
            s_pfnRtlGenRandom = (DataraRtlGenRandomFn)GetProcAddress(hAdvApi, "SystemFunction036");
        }
        s_rng_init = 1;
    }
    if (s_pfnRtlGenRandom && s_pfnRtlGenRandom((void*)buf, (unsigned long)len)) {
        return len;
    }
#else
    int fd = open("/dev/urandom", O_RDONLY);
    if (fd >= 0) {
        ssize_t r = read(fd, buf, (size_t)len);
        close(fd);
        if (r > 0) return (int64_t)r;
    }
#endif
    // Insecure clock-seeded LCG fallback: mark it so callers can detect that
    // the bytes are not cryptographically random (datara_rt_rng_is_insecure).
    g_datara_rng_insecure = 1;
    // High-entropy fallback using splitmix64 / xorshift with clock
    int64_t seed = datara_rt_now_precise_ms();
    for (int64_t i = 0; i < len; i++) {
        seed = seed * 6364136223846793005ULL + 1442695040888963407ULL;
        buf[i] = (uint8_t)((seed >> 32) ^ (seed & 0xFF));
    }
    return len;
}

int datara_rt_rng_is_insecure(void) {
    return g_datara_rng_insecure;
}

const char* datara_rt_uuid_v4(void) {
    uint8_t b[16];
    datara_rt_random_bytes(b, 16);
    // RFC 4122 v4 variant & version bits
    b[6] = (b[6] & 0x0F) | 0x40; // Version 4
    b[8] = (b[8] & 0x3F) | 0x80; // Variant 1 (RFC 4122)

    char* str = (char*)malloc(37);
    if (!str) return "";
    snprintf(str, 37, "%02x%02x%02x%02x-%02x%02x-%02x%02x-%02x%02x-%02x%02x%02x%02x%02x%02x",
        b[0], b[1], b[2], b[3],
        b[4], b[5],
        b[6], b[7],
        b[8], b[9],
        b[10], b[11], b[12], b[13], b[14], b[15]
    );
    return str;
}

// ---------------------------------------------------------------------------
// Native UI Dialogs (Cross-platform)
// ---------------------------------------------------------------------------

// Escape a string for use inside a double-quoted shell argument. Escapes
// backslash, double quote, dollar and backtick so untrusted message/title
// text cannot terminate the quoting, expand shell variables or inject
// commands into `system()` invocations built from osascript/zenity.
static void datara_shell_escape_double(const char* src, char* dst, size_t dst_size) {
    size_t j = 0;
    if (dst_size == 0) return;
    dst[0] = '\0';
    if (!src) return;
    for (size_t i = 0; src[i] != '\0' && j + 2 < dst_size; i++) {
        char c = src[i];
        if (c == '\\' || c == '"' || c == '$' || c == '`') {
            dst[j++] = '\\';
        }
        dst[j++] = c;
    }
    dst[j] = '\0';
}

int64_t datara_rt_dialog_info(const char* title, const char* msg) {
#ifdef _WIN32
    HMODULE hUser = LoadLibraryA("user32.dll");
    if (hUser) {
        typedef int (WINAPI *MsgBoxFn)(HWND, LPCSTR, LPCSTR, UINT);
        MsgBoxFn pfn = (MsgBoxFn)GetProcAddress(hUser, "MessageBoxA");
        if (pfn) {
            pfn(NULL, msg ? msg : "", title ? title : "Information", 0x00000000L | 0x00000040L /* MB_OK | MB_ICONINFORMATION */);
            return 1;
        }
    }
#elif defined(__APPLE__)
    if (msg && title) {
        char cmd[2048];
        char emsg[512], etitle[512];
        datara_shell_escape_double(msg, emsg, sizeof(emsg));
        datara_shell_escape_double(title, etitle, sizeof(etitle));
        snprintf(cmd, sizeof(cmd), "osascript -e 'display dialog \"%s\" with title \"%s\" buttons {\"OK\"} default button \"OK\"'", emsg, etitle);
        if (system(cmd) == 0) return 1;
    }
#else
    if (getenv("DISPLAY") || getenv("WAYLAND_DISPLAY")) {
        char cmd[2048];
        char emsg[512], etitle[512];
        datara_shell_escape_double(msg ? msg : "", emsg, sizeof(emsg));
        datara_shell_escape_double(title ? title : "Info", etitle, sizeof(etitle));
        snprintf(cmd, sizeof(cmd), "zenity --info --title=\"%s\" --text=\"%s\" 2>/dev/null", etitle, emsg);
        if (system(cmd) == 0) return 1;
    }
#endif
    printf("[%s] %s\n", title ? title : "Info", msg ? msg : "");
    fflush(stdout);
    return 1;
}

int64_t datara_rt_dialog_alert(const char* title, const char* msg) {
#ifdef _WIN32
    HMODULE hUser = LoadLibraryA("user32.dll");
    if (hUser) {
        typedef int (WINAPI *MsgBoxFn)(HWND, LPCSTR, LPCSTR, UINT);
        MsgBoxFn pfn = (MsgBoxFn)GetProcAddress(hUser, "MessageBoxA");
        if (pfn) {
            pfn(NULL, msg ? msg : "", title ? title : "Alert", 0x00000000L | 0x00000030L /* MB_OK | MB_ICONWARNING */);
            return 1;
        }
    }
#elif defined(__APPLE__)
    if (msg && title) {
        char cmd[2048];
        char emsg[512], etitle[512];
        datara_shell_escape_double(msg, emsg, sizeof(emsg));
        datara_shell_escape_double(title, etitle, sizeof(etitle));
        snprintf(cmd, sizeof(cmd), "osascript -e 'display alert \"%s\" message \"%s\" as warning'", etitle, emsg);
        if (system(cmd) == 0) return 1;
    }
#else
    if (getenv("DISPLAY") || getenv("WAYLAND_DISPLAY")) {
        char cmd[2048];
        char emsg[512], etitle[512];
        datara_shell_escape_double(msg ? msg : "", emsg, sizeof(emsg));
        datara_shell_escape_double(title ? title : "Warning", etitle, sizeof(etitle));
        snprintf(cmd, sizeof(cmd), "zenity --warning --title=\"%s\" --text=\"%s\" 2>/dev/null", etitle, emsg);
        if (system(cmd) == 0) return 1;
    }
#endif
    fprintf(stderr, "[%s] %s\n", title ? title : "Warning", msg ? msg : "");
    fflush(stderr);
    return 1;
}

int64_t datara_rt_dialog_confirm(const char* title, const char* msg) {
#ifdef _WIN32
    HMODULE hUser = LoadLibraryA("user32.dll");
    if (hUser) {
        typedef int (WINAPI *MsgBoxFn)(HWND, LPCSTR, LPCSTR, UINT);
        MsgBoxFn pfn = (MsgBoxFn)GetProcAddress(hUser, "MessageBoxA");
        if (pfn) {
            int res = pfn(NULL, msg ? msg : "", title ? title : "Confirm", 0x00000004L | 0x00000020L /* MB_YESNO | MB_ICONQUESTION */);
            return (res == 6 /* IDYES */) ? 1 : 0;
        }
    }
#elif defined(__APPLE__)
    if (msg && title) {
        char cmd[2048];
        char emsg[512], etitle[512];
        datara_shell_escape_double(msg, emsg, sizeof(emsg));
        datara_shell_escape_double(title, etitle, sizeof(etitle));
        snprintf(cmd, sizeof(cmd), "osascript -e 'display dialog \"%s\" with title \"%s\" buttons {\"Cancel\", \"OK\"} default button \"OK\"'", emsg, etitle);
        int res = system(cmd);
        return (res == 0) ? 1 : 0;
    }
#else
    if (getenv("DISPLAY") || getenv("WAYLAND_DISPLAY")) {
        char cmd[2048];
        char emsg[512], etitle[512];
        datara_shell_escape_double(msg ? msg : "", emsg, sizeof(emsg));
        datara_shell_escape_double(title ? title : "Confirm", etitle, sizeof(etitle));
        snprintf(cmd, sizeof(cmd), "zenity --question --title=\"%s\" --text=\"%s\" 2>/dev/null", etitle, emsg);
        int res = system(cmd);
        return (res == 0) ? 1 : 0;
    }
#endif
    printf("[%s] %s (y/n): ", title ? title : "Confirm", msg ? msg : "");
    fflush(stdout);
    int c = getchar();
    return (c == 'y' || c == 'Y') ? 1 : 0;
}

int64_t datara_rt_system(const char* cmd) {
    if (!cmd) return -1;
    return (int64_t)system(cmd);
}

const char* datara_rt_exec(const char* cmd) {
    if (!cmd) return "";
#ifdef _WIN32
    FILE* pipe = _popen(cmd, "rt");
#else
    FILE* pipe = popen(cmd, "r");
#endif
    if (!pipe) return "";
    size_t cap = 4096;
    size_t len = 0;
    char* buf = (char*)malloc(cap);
    if (!buf) {
#ifdef _WIN32
        _pclose(pipe);
#else
        pclose(pipe);
#endif
        return "";
    }
    char temp[512];
    while (fgets(temp, sizeof(temp), pipe)) {
        size_t tlen = strlen(temp);
        if (len + tlen + 1 > cap) {
            cap *= 2;
            char* new_buf = (char*)realloc(buf, cap);
            if (!new_buf) break;
            buf = new_buf;
        }
        memcpy(buf + len, temp, tlen);
        len += tlen;
    }
    buf[len] = '\0';
#ifdef _WIN32
    _pclose(pipe);
#else
    pclose(pipe);
#endif
    return buf;
}

// ---------------------------------------------------------------------------
// Memory Management & RAII
// ---------------------------------------------------------------------------

void datara_rt_free(void* ptr) {
    if (ptr) {
        free(ptr);
    }
}

// Strings produced by the runtime live in TLS static buffers (tls_int_bufs),
// inside the shared tls_scratch_ring block, or are heap blocks that the
// runtime intentionally never frees (see the note near datara_rt_bool_to_str).
// Generated code calls str_free speculatively, so there is no ownership
// tracking that would let us prove a pointer is a plain heap allocation.
// Freeing any of the above would corrupt the heap, so this is a documented
// no-op: runtime strings leak (by design), but nothing is ever corrupted.
// Lists and maps carry tracked headers and are freed by their own *_free.
void datara_rt_str_free(const char* s) {
    (void)s;
}

void datara_rt_list_free(void* list) {
    if (!list) return;
    DataraListHeader* hdr = ((DataraListHeader*)list) - 1;
    if (hdr->magic == DATARA_LIST_MAGIC) {
        free(hdr);
    } else {
        free(list);
    }
}

// ---------------------------------------------------------------------------
// High-Performance Multithreading Engine: Hardware-scaling Thread Pool
// ---------------------------------------------------------------------------

#define DATARA_MAX_WORKERS 64

typedef struct {
    void (*fn)(int64_t, void*);
    void* ctx;
    int64_t start;
    int64_t end;
} DataraParallelChunk;

typedef struct {
    void (*task_fn)(void*);
    void* task_ctx;
    volatile long is_done;
} DataraTask;

static int g_workers_count = 0;
static int g_workers_initialized = 0;

#ifdef _WIN32
static HANDLE g_worker_threads[DATARA_MAX_WORKERS];
static HANDLE g_start_events[DATARA_MAX_WORKERS];
static HANDLE g_done_events[DATARA_MAX_WORKERS];
static volatile int g_shutdown = 0;
static DataraParallelChunk g_worker_chunks[DATARA_MAX_WORKERS];
static DataraTask g_worker_tasks[DATARA_MAX_WORKERS];
static volatile int g_worker_mode[DATARA_MAX_WORKERS]; // 0 = none, 1 = parallel_for, 2 = invoke

static DWORD WINAPI datara_worker_proc(LPVOID arg) {
    int worker_idx = (int)(intptr_t)arg;
    while (1) {
        WaitForSingleObject(g_start_events[worker_idx], INFINITE);
        if (g_shutdown) break;

        if (g_worker_mode[worker_idx] == 1) {
            DataraParallelChunk* c = &g_worker_chunks[worker_idx];
            if (c->fn) {
                for (int64_t i = c->start; i < c->end; i++) {
                    c->fn(i, c->ctx);
                }
            }
        } else if (g_worker_mode[worker_idx] == 2) {
            DataraTask* t = &g_worker_tasks[worker_idx];
            if (t->task_fn) {
                t->task_fn(t->task_ctx);
            }
            t->is_done = 1;
        }

        SetEvent(g_done_events[worker_idx]);
    }
    return 0;
}
#else
#include <pthread.h>
#include <unistd.h>
static pthread_t g_worker_threads[DATARA_MAX_WORKERS];
static pthread_mutex_t g_worker_mutexes[DATARA_MAX_WORKERS];
static pthread_cond_t g_worker_conds[DATARA_MAX_WORKERS];
static volatile int g_worker_ready[DATARA_MAX_WORKERS];
static volatile int g_worker_done[DATARA_MAX_WORKERS];
static volatile int g_shutdown = 0;
static DataraParallelChunk g_worker_chunks[DATARA_MAX_WORKERS];
static DataraTask g_worker_tasks[DATARA_MAX_WORKERS];
static volatile int g_worker_mode[DATARA_MAX_WORKERS];

static void* datara_worker_proc(void* arg) {
    int worker_idx = (int)(intptr_t)arg;
    while (1) {
        pthread_mutex_lock(&g_worker_mutexes[worker_idx]);
        while (!g_worker_ready[worker_idx] && !g_shutdown) {
            pthread_cond_wait(&g_worker_conds[worker_idx], &g_worker_mutexes[worker_idx]);
        }
        if (g_shutdown) {
            pthread_mutex_unlock(&g_worker_mutexes[worker_idx]);
            break;
        }
        g_worker_ready[worker_idx] = 0;
        pthread_mutex_unlock(&g_worker_mutexes[worker_idx]);

        if (g_worker_mode[worker_idx] == 1) {
            DataraParallelChunk* c = &g_worker_chunks[worker_idx];
            if (c->fn) {
                for (int64_t i = c->start; i < c->end; i++) {
                    c->fn(i, c->ctx);
                }
            }
        } else if (g_worker_mode[worker_idx] == 2) {
            DataraTask* t = &g_worker_tasks[worker_idx];
            if (t->task_fn) {
                t->task_fn(t->task_ctx);
            }
            t->is_done = 1;
        }

        pthread_mutex_lock(&g_worker_mutexes[worker_idx]);
        g_worker_done[worker_idx] = 1;
        pthread_cond_signal(&g_worker_conds[worker_idx]);
        pthread_mutex_unlock(&g_worker_mutexes[worker_idx]);
    }
    return NULL;
}
#endif

void datara_rt_thread_pool_init(int64_t workers) {
    if (g_workers_initialized) return;
    if (workers <= 0) {
#ifdef _WIN32
        SYSTEM_INFO sys;
        GetSystemInfo(&sys);
        workers = (int64_t)sys.dwNumberOfProcessors;
#elif defined(__APPLE__)
        int count = 0;
        size_t count_len = sizeof(count);
        sysctlbyname("hw.logicalcpu", &count, &count_len, NULL, 0);
        workers = count > 0 ? (int64_t)count : 4;
#else
        workers = (int64_t)sysconf(_SC_NPROCESSORS_ONLN);
#endif
    }
    if (workers > DATARA_MAX_WORKERS) workers = DATARA_MAX_WORKERS;
    if (workers < 1) workers = 1;
    g_workers_count = (int)workers;

#ifdef _WIN32
    for (int i = 1; i < g_workers_count; i++) {
        g_start_events[i] = CreateEvent(NULL, FALSE, FALSE, NULL);
        g_done_events[i] = CreateEvent(NULL, FALSE, FALSE, NULL);
        g_worker_threads[i] = CreateThread(NULL, 0, datara_worker_proc, (LPVOID)(intptr_t)i, 0, NULL);
    }
#else
    for (int i = 1; i < g_workers_count; i++) {
        pthread_mutex_init(&g_worker_mutexes[i], NULL);
        pthread_cond_init(&g_worker_conds[i], NULL);
        g_worker_ready[i] = 0;
        g_worker_done[i] = 0;
        pthread_create(&g_worker_threads[i], NULL, datara_worker_proc, (void*)(intptr_t)i);
    }
#endif
    g_workers_initialized = 1;
}

static inline void datara_rt_ensure_threads(void) {
    if (!g_workers_initialized) {
        datara_rt_thread_pool_init(0);
    }
}

int64_t datara_rt_num_workers(void) {
    datara_rt_ensure_threads();
    return (int64_t)g_workers_count;
}

static DATARA_TLS int tls_in_parallel_region = 0;

void datara_rt_parallel_for(int64_t start, int64_t end, void (*fn)(int64_t idx, void* ctx), void* ctx) {
    if (start >= end || !fn) return;
    datara_rt_ensure_threads();

    int64_t total = end - start;
    int num_w = g_workers_count;
    if (tls_in_parallel_region != 0 || num_w <= 1 || total <= 1) {
        for (int64_t i = start; i < end; i++) {
            fn(i, ctx);
        }
        return;
    }

    tls_in_parallel_region = 1;

    if (num_w > total) num_w = (int)total;
    int64_t chunk_size = total / num_w;
    int64_t rem = total % num_w;

    int64_t cur_start = start + chunk_size + (0 < rem ? 1 : 0);
    for (int w = 1; w < num_w; w++) {
        int64_t w_chunk = chunk_size + (w < rem ? 1 : 0);
        int64_t w_end = cur_start + w_chunk;
        g_worker_chunks[w].fn = fn;
        g_worker_chunks[w].ctx = ctx;
        g_worker_chunks[w].start = cur_start;
        g_worker_chunks[w].end = w_end;
        g_worker_mode[w] = 1;
#ifdef _WIN32
        ResetEvent(g_done_events[w]);
        SetEvent(g_start_events[w]);
#else
        pthread_mutex_lock(&g_worker_mutexes[w]);
        g_worker_ready[w] = 1;
        g_worker_done[w] = 0;
        pthread_cond_signal(&g_worker_conds[w]);
        pthread_mutex_unlock(&g_worker_mutexes[w]);
#endif
        cur_start = w_end;
    }

    // Current thread immediately executes the first chunk (zero overhead)
    int64_t main_end = start + chunk_size + (0 < rem ? 1 : 0);
    for (int64_t i = start; i < main_end; i++) {
        fn(i, ctx);
    }

    // Wait for all worker threads to complete
#ifdef _WIN32
    for (int w = 1; w < num_w; w++) {
        WaitForSingleObject(g_done_events[w], INFINITE);
    }
#else
    for (int w = 1; w < num_w; w++) {
        pthread_mutex_lock(&g_worker_mutexes[w]);
        while (!g_worker_done[w]) {
            pthread_cond_wait(&g_worker_conds[w], &g_worker_mutexes[w]);
        }
        pthread_mutex_unlock(&g_worker_mutexes[w]);
    }
#endif

    tls_in_parallel_region = 0;
}

void datara_rt_parallel_invoke(void (*fn1)(void* ctx1), void* ctx1, void (*fn2)(void* ctx2), void* ctx2) {
    datara_rt_ensure_threads();
    if (tls_in_parallel_region != 0 || g_workers_count <= 1) {
        if (fn1) fn1(ctx1);
        if (fn2) fn2(ctx2);
        return;
    }

    tls_in_parallel_region = 1;

    // Dispatch fn1 to worker 1
    g_worker_tasks[1].task_fn = fn1;
    g_worker_tasks[1].task_ctx = ctx1;
    g_worker_tasks[1].is_done = 0;
    g_worker_mode[1] = 2;

#ifdef _WIN32
    ResetEvent(g_done_events[1]);
    SetEvent(g_start_events[1]);
#else
    pthread_mutex_lock(&g_worker_mutexes[1]);
    g_worker_ready[1] = 1;
    g_worker_done[1] = 0;
    pthread_cond_signal(&g_worker_conds[1]);
    pthread_mutex_unlock(&g_worker_mutexes[1]);
#endif

    // Run fn2 on current thread concurrently
    if (fn2) fn2(ctx2);

    // Wait for worker 1
#ifdef _WIN32
    WaitForSingleObject(g_done_events[1], INFINITE);
#else
    pthread_mutex_lock(&g_worker_mutexes[1]);
    while (!g_worker_done[1]) {
        pthread_cond_wait(&g_worker_conds[1], &g_worker_mutexes[1]);
    }
    pthread_mutex_unlock(&g_worker_mutexes[1]);
#endif

    tls_in_parallel_region = 0;
}

// ---------------------------------------------------------------------------
// Effect-Driven Ephemeral Frame Arena
// ---------------------------------------------------------------------------
#define DATARA_ARENA_SIZE (2 * 1024 * 1024) // 2MB thread-local ephemeral bump arena
#if defined(_MSC_VER)
static __declspec(thread) char* g_datara_arena = NULL;
static __declspec(thread) int64_t g_datara_arena_top = 0;
#else
static __thread char* g_datara_arena = NULL;
static __thread int64_t g_datara_arena_top = 0;
#endif

void* datara_rt_arena_alloc(int64_t bytes) {
    if (bytes <= 0) return NULL;
    int64_t aligned = (bytes + 7) & ~7;
    if (!g_datara_arena) {
        g_datara_arena = (char*)malloc(DATARA_ARENA_SIZE);
        if (!g_datara_arena) return malloc((size_t)bytes);
    }
    if (g_datara_arena_top + aligned > DATARA_ARENA_SIZE) {
        return malloc((size_t)bytes);
    }
    void* ptr = (void*)&g_datara_arena[g_datara_arena_top];
    g_datara_arena_top += aligned;
    return ptr;
}

int64_t datara_rt_arena_checkpoint(void) {
    return g_datara_arena_top;
}

void datara_rt_arena_reset(int64_t saved_top) {
    if (saved_top >= 0 && saved_top <= g_datara_arena_top) {
        g_datara_arena_top = saved_top;
    }
}

// ---------------------------------------------------------------------------
// First-Class SIMD 4D Vector Math
// ---------------------------------------------------------------------------
// SSE2 intrinsics are used for the hot vector operations when the target is
// x86-64 / SSE2-capable. Everything falls back to a portable scalar path
// otherwise, and datara_rt_simd_enabled() reports which path is compiled in.
#if defined(__x86_64__) || defined(_M_X64) || defined(__SSE2__) || (defined(_MSC_VER) && !defined(_M_ARM64))
#define DATARA_SIMD_SSE2 1
#include <emmintrin.h>
#endif

int datara_rt_simd_enabled(void) {
#if defined(DATARA_SIMD_SSE2)
    return 1;
#else
    return 0;
#endif
}

DataraFloat4 datara_rt_float4(double x, double y, double z, double w) {
    DataraFloat4 v;
    v.x = (float)x; v.y = (float)y; v.z = (float)z; v.w = (float)w;
    return v;
}

DataraInt4 datara_rt_int4(int64_t x, int64_t y, int64_t z, int64_t w) {
    DataraInt4 v;
    v.x = (int32_t)x; v.y = (int32_t)y; v.z = (int32_t)z; v.w = (int32_t)w;
    return v;
}

double datara_rt_float4_dot(DataraFloat4 a, DataraFloat4 b) {
#if defined(DATARA_SIMD_SSE2)
    /* One packed multiply + SSE2 shuffle/add horizontal reduction. */
    __m128 va = _mm_loadu_ps(&a.x);
    __m128 vb = _mm_loadu_ps(&b.x);
    __m128 prod = _mm_mul_ps(va, vb);
    /* Round 1: add lanes (1,0,3,2) -> partial sums in both low lanes. */
    __m128 shuf = _mm_shuffle_ps(prod, prod, _MM_SHUFFLE(2, 3, 0, 1));
    __m128 sums = _mm_add_ps(prod, shuf);
    /* Round 2: add lanes (2,3,0,1) -> full sum broadcast to all lanes. */
    shuf = _mm_shuffle_ps(sums, sums, _MM_SHUFFLE(1, 0, 3, 2));
    sums = _mm_add_ps(sums, shuf);
    return (double)_mm_cvtss_f32(sums);
#else
    return (double)(a.x * b.x + a.y * b.y + a.z * b.z + a.w * b.w);
#endif
}

DataraFloat4 datara_rt_float4_min4(DataraFloat4 a, DataraFloat4 b) {
    DataraFloat4 out;
#if defined(DATARA_SIMD_SSE2)
    __m128 va = _mm_loadu_ps(&a.x);
    __m128 vb = _mm_loadu_ps(&b.x);
    _mm_storeu_ps(&out.x, _mm_min_ps(va, vb));
#else
    out.x = a.x < b.x ? a.x : b.x;
    out.y = a.y < b.y ? a.y : b.y;
    out.z = a.z < b.z ? a.z : b.z;
    out.w = a.w < b.w ? a.w : b.w;
#endif
    return out;
}

DataraFloat4 datara_rt_float4_max4(DataraFloat4 a, DataraFloat4 b) {
    DataraFloat4 out;
#if defined(DATARA_SIMD_SSE2)
    __m128 va = _mm_loadu_ps(&a.x);
    __m128 vb = _mm_loadu_ps(&b.x);
    _mm_storeu_ps(&out.x, _mm_max_ps(va, vb));
#else
    out.x = a.x > b.x ? a.x : b.x;
    out.y = a.y > b.y ? a.y : b.y;
    out.z = a.z > b.z ? a.z : b.z;
    out.w = a.w > b.w ? a.w : b.w;
#endif
    return out;
}

DataraInt4 datara_rt_int4_min4(DataraInt4 a, DataraInt4 b) {
    DataraInt4 out;
#if defined(DATARA_SIMD_SSE2)
    /* SSE2 has no integer min/max for 32-bit lanes: compare, then blend
       with and/andnot (select a where a > b for min, else b). */
    __m128i va = _mm_loadu_si128((const __m128i*)&a.x);
    __m128i vb = _mm_loadu_si128((const __m128i*)&b.x);
    __m128i gt = _mm_cmpgt_epi32(va, vb);
    _mm_storeu_si128((__m128i*)&out.x,
                     _mm_or_si128(_mm_and_si128(gt, vb), _mm_andnot_si128(gt, va)));
#else
    out.x = a.x < b.x ? a.x : b.x;
    out.y = a.y < b.y ? a.y : b.y;
    out.z = a.z < b.z ? a.z : b.z;
    out.w = a.w < b.w ? a.w : b.w;
#endif
    return out;
}

DataraInt4 datara_rt_int4_max4(DataraInt4 a, DataraInt4 b) {
    DataraInt4 out;
#if defined(DATARA_SIMD_SSE2)
    __m128i va = _mm_loadu_si128((const __m128i*)&a.x);
    __m128i vb = _mm_loadu_si128((const __m128i*)&b.x);
    __m128i gt = _mm_cmpgt_epi32(va, vb);
    _mm_storeu_si128((__m128i*)&out.x,
                     _mm_or_si128(_mm_and_si128(gt, va), _mm_andnot_si128(gt, vb)));
#else
    out.x = a.x > b.x ? a.x : b.x;
    out.y = a.y > b.y ? a.y : b.y;
    out.z = a.z > b.z ? a.z : b.z;
    out.w = a.w > b.w ? a.w : b.w;
#endif
    return out;
}

// ============================================================================
// Graduated Ownership Runtime Guards
// Thread-unsafe by default — guards are per-thread
// ============================================================================
typedef struct {
    int64_t val;
    uint32_t refcount;
    uint32_t active;
} DataraOwnGuardSlot;

#define DATARA_OWN_MAX_GUARDS 8192
static DATARA_TLS DataraOwnGuardSlot g_datara_own_slots[DATARA_OWN_MAX_GUARDS];
static DATARA_TLS uint32_t g_datara_own_next_slot = 1;

int64_t datara_rt_own_acquire(int64_t val) {
    for (uint32_t i = 1; i < g_datara_own_next_slot && i < DATARA_OWN_MAX_GUARDS; i++) {
        if (g_datara_own_slots[i].active && g_datara_own_slots[i].val == val) {
            g_datara_own_slots[i].refcount++;
            return val;
        }
    }
    uint32_t slot_idx = 0;
    for (uint32_t i = 1; i < g_datara_own_next_slot && i < DATARA_OWN_MAX_GUARDS; i++) {
        if (!g_datara_own_slots[i].active) {
            slot_idx = i;
            break;
        }
    }
    if (slot_idx == 0 && g_datara_own_next_slot < DATARA_OWN_MAX_GUARDS) {
        slot_idx = g_datara_own_next_slot++;
    }
    if (slot_idx > 0 && slot_idx < DATARA_OWN_MAX_GUARDS) {
        g_datara_own_slots[slot_idx].val = val;
        g_datara_own_slots[slot_idx].refcount = 1;
        g_datara_own_slots[slot_idx].active = 1;
    }
    return val;
}

void datara_rt_own_release(int64_t val) {
    for (uint32_t i = 1; i < g_datara_own_next_slot && i < DATARA_OWN_MAX_GUARDS; i++) {
        if (g_datara_own_slots[i].active && g_datara_own_slots[i].val == val) {
            if (g_datara_own_slots[i].refcount > 0) {
                g_datara_own_slots[i].refcount--;
            }
            if (g_datara_own_slots[i].refcount == 0) {
                g_datara_own_slots[i].active = 0;
                g_datara_own_slots[i].val = 0;
            }
            return;
        }
    }
}


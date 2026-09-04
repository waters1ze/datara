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
#endif

#ifndef INADDR_NONE
#define INADDR_NONE ((unsigned long)0xffffffff)
#endif

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
        tls_scratch_offset = 0;
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
#define QNAN_PREFIX 0x7FF8000000000000ULL
#define TAG_INT     0x0001000000000000ULL
#define TAG_BOOL    0x0002000000000000ULL
#define TAG_STR     0x0003000000000000ULL
#define TAG_OBJ     0x0004000000000000ULL
#define TAG_NULL    0x0005000000000000ULL

uint64_t datara_rt_nanbox_int(int64_t val) {
    return QNAN_PREFIX | TAG_INT | (uint64_t)(uint32_t)val;
}

int64_t datara_rt_nanunbox_int(uint64_t box) {
    return (int64_t)(int32_t)(box & 0xFFFFFFFFULL);
}

uint64_t datara_rt_nanbox_bool(int64_t b) {
    return QNAN_PREFIX | TAG_BOOL | (b ? 1ULL : 0ULL);
}

int64_t datara_rt_nanunbox_bool(uint64_t box) {
    return (box & 1ULL) ? 1 : 0;
}

uint64_t datara_rt_nanbox_str(const char* s) {
    return QNAN_PREFIX | TAG_STR | (((uint64_t)s) & 0x0000FFFFFFFFFFFFULL);
}

const char* datara_rt_nanunbox_str(uint64_t box) {
    return (const char*)(box & 0x0000FFFFFFFFFFFFULL);
}

void datara_rt_out_val(uint64_t box) {
    if ((box & 0xFFF8000000000000ULL) != QNAN_PREFIX) {
        double d;
        memcpy(&d, &box, 8);
        datara_rt_out_float(d);
    } else {
        uint64_t tag = box & 0x0007000000000000ULL;
        if (tag == TAG_INT) {
            datara_rt_out_int(datara_rt_nanunbox_int(box));
        } else if (tag == TAG_BOOL) {
            datara_rt_out_bool(datara_rt_nanunbox_bool(box));
        } else if (tag == TAG_STR) {
            datara_rt_out_str(datara_rt_nanunbox_str(box));
        } else if (tag == TAG_NULL) {
            printf("None\n");
        } else {
            datara_rt_out_int((int64_t)box);
        }
    }
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

void datara_rt_panic(const char* s) {
    datara_rt_flush();
    fprintf(stderr, "panic: %s\n", s ? s : "explicit panic");
    fflush(stderr);
    exit(1);
}

void datara_rt_assert(int64_t cond, const char* msg) {
    if (!cond) {
        datara_rt_panic(msg ? msg : "assertion failed");
    }
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

int64_t datara_rt_list_get(int64_t* list, int64_t idx) {
    if (!list) return 0;
    int64_t count = list[0];
    if (idx < 0 || idx >= count) return 0;
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

typedef struct {
    int64_t capacity;
} DataraListHeader;

int64_t* datara_rt_list_create_capacity(int64_t cap) {
    if (cap < 8) cap = 8;
    DataraListHeader* hdr = (DataraListHeader*)malloc(sizeof(DataraListHeader) + (size_t)(cap + 1) * sizeof(int64_t));
    if (!hdr) return NULL;
    hdr->capacity = cap;
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
    DataraListHeader* hdr = ((DataraListHeader*)list) - 1;
    if (hdr->capacity >= count && hdr->capacity < 1000000000LL) {
        if (count + 1 > hdr->capacity) {
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
    if (start < 0) start = 0;
    if (end > len) end = len;
    if (start >= end) {
        int64_t* res = (int64_t*)malloc(sizeof(int64_t));
        if (res) res[0] = 0;
        return res;
    }
    int64_t count = end - start;
    int64_t* res = (int64_t*)malloc((size_t)(count + 1) * sizeof(int64_t));
    if (!res) return NULL;
    res[0] = count;
    for (int64_t i = 0; i < count; i++) {
        res[i + 1] = list[start + i + 1];
    }
    return res;
}

int64_t* datara_rt_list_create_repeat(int64_t elem, int64_t count) {
    if (count < 0) count = 0;
    int64_t* arr = (int64_t*)malloc((size_t)(count + 1) * sizeof(int64_t));
    if (!arr) return NULL;
    arr[0] = count;
    for (int64_t i = 0; i < count; i++) {
        arr[i + 1] = elem;
    }
    return arr;
}

void* datara_rt_map_create(void) {
    int64_t* map = (int64_t*)malloc(sizeof(int64_t));
    if (!map) return NULL;
    map[0] = 0;
    return map;
}

int64_t* datara_rt_map_create_2(const char* k0, int64_t v0, const char* k1, int64_t v1) {
    int64_t* map = (int64_t*)malloc(5 * sizeof(int64_t));
    if (!map) return NULL;
    map[0] = 2;
    map[1] = (int64_t)k0;
    map[2] = v0;
    map[3] = (int64_t)k1;
    map[4] = v1;
    return map;
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

typedef struct {
    int64_t capacity;
    int64_t magic;
} DataraMapHeader;

#define DATARA_MAP_MAGIC 0x4441544D41503130ULL

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
    if (sz < 0) { fclose(f); return ""; }
    fseek(f, 0, SEEK_SET);
    char* buf = (char*)malloc(sz + 1);
    if (!buf) { fclose(f); return ""; }
    size_t read_bytes = fread(buf, 1, sz, f);
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

static int g_argc = 0;
static char** g_argv = NULL;

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

const char* datara_rt_str_char_at(const char* s, int64_t idx) {
    if (!s || idx < 0) return "";
    int64_t total_len = (int64_t)strlen(s);
    if (idx >= total_len) return "";
    char* buf = datara_scratch_alloc(1);
    if (!buf) return "";
    buf[0] = s[idx];
    buf[1] = '\0';
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

const char* datara_rt_http_get(void) {
    return "HTTP/1.1 200 OK\r\nContent-Type: text/plain\r\nContent-Length: 2\r\n\r\nOK";
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
double datara_rt_math_hypot(double a, double b) { return hypot(a, b); }
int64_t datara_rt_math_min_int(int64_t a, int64_t b) { return a < b ? a : b; }
int64_t datara_rt_math_max_int(int64_t a, int64_t b) { return a > b ? a : b; }
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
int64_t datara_rt_math_shr(int64_t v, int64_t s) { return v >> s; }
int64_t datara_rt_math_shl(int64_t v, int64_t s) { return v << s; }
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

        if (a < 0 || b < 0) { free(decoded); return ""; }

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
    // High-entropy fallback using splitmix64 / xorshift with clock
    int64_t seed = datara_rt_now_precise_ms();
    for (int64_t i = 0; i < len; i++) {
        seed = seed * 6364136223846793005ULL + 1442695040888963407ULL;
        buf[i] = (uint8_t)((seed >> 32) ^ (seed & 0xFF));
    }
    return len;
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
        char cmd[1024];
        snprintf(cmd, sizeof(cmd), "osascript -e 'display dialog \"%s\" with title \"%s\" buttons {\"OK\"} default button \"OK\"'", msg, title);
        if (system(cmd) == 0) return 1;
    }
#else
    if (getenv("DISPLAY") || getenv("WAYLAND_DISPLAY")) {
        char cmd[1024];
        snprintf(cmd, sizeof(cmd), "zenity --info --title=\"%s\" --text=\"%s\" 2>/dev/null", title ? title : "Info", msg ? msg : "");
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
        char cmd[1024];
        snprintf(cmd, sizeof(cmd), "osascript -e 'display alert \"%s\" message \"%s\" as warning'", title, msg);
        if (system(cmd) == 0) return 1;
    }
#else
    if (getenv("DISPLAY") || getenv("WAYLAND_DISPLAY")) {
        char cmd[1024];
        snprintf(cmd, sizeof(cmd), "zenity --warning --title=\"%s\" --text=\"%s\" 2>/dev/null", title ? title : "Warning", msg ? msg : "");
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
        char cmd[1024];
        snprintf(cmd, sizeof(cmd), "osascript -e 'display dialog \"%s\" with title \"%s\" buttons {\"Cancel\", \"OK\"} default button \"OK\"'", msg, title);
        int res = system(cmd);
        return (res == 0) ? 1 : 0;
    }
#else
    if (getenv("DISPLAY") || getenv("WAYLAND_DISPLAY")) {
        char cmd[1024];
        snprintf(cmd, sizeof(cmd), "zenity --question --title=\"%s\" --text=\"%s\" 2>/dev/null", title ? title : "Confirm", msg ? msg : "");
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

void datara_rt_str_free(const char* s) {
    if (s && s[0] != '\0' && s != "true" && s != "false" && s != "None") {
        free((void*)s);
    }
}

void datara_rt_list_free(void* list) {
    if (!list) return;
    DataraListHeader* hdr = ((DataraListHeader*)list) - 1;
    if (hdr->capacity > 0 && hdr->capacity < 1000000000LL) {
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
    return (double)(a.x * b.x + a.y * b.y + a.z * b.z + a.w * b.w);
}


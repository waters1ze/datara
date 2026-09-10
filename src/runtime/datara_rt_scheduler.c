#include "datara_rt_scheduler.h"
#include "datara_runtime.h"

#include <stdio.h>
#include <stdlib.h>
#include <string.h>

#ifdef _WIN32
#include <windows.h>
#else
#include <pthread.h>
#include <unistd.h>
#endif

// Counters for verification and Phase 4 determinism contract
static uint64_t g_sched_mutex_queue_pushes = 0;
static uint64_t g_sched_wave_executions = 0;

uint64_t datara_rt_scheduler_mutex_queue_pushes(void) {
    return g_sched_mutex_queue_pushes;
}

uint64_t datara_rt_scheduler_wave_executions(void) {
    return g_sched_wave_executions;
}

void datara_rt_scheduler_reset_stats(void) {
    g_sched_mutex_queue_pushes = 0;
    g_sched_wave_executions = 0;
}

// Region Cancellation Registry
#define MAX_CANCELLED_REGIONS 512
static int64_t g_cancelled_regions[MAX_CANCELLED_REGIONS];
static int g_cancelled_count = 0;

#ifdef _WIN32
static CRITICAL_SECTION g_cancel_cs;
static INIT_ONCE g_cancel_init_once = INIT_ONCE_STATIC_INIT;

// Callback for InitOnceExecuteOnce to initialize CRITICAL_SECTION exactly once safely
static BOOL CALLBACK init_cancel_cs_callback(PINIT_ONCE InitOnce, PVOID Parameter, PVOID *Context) {
    (void)InitOnce;
    (void)Parameter;
    (void)Context;
    InitializeCriticalSection(&g_cancel_cs);
    return TRUE;
}

// Wave A4: Safe initialization gate avoiding DLL attach / static init race conditions
static void ensure_cancel_cs(void) {
    InitOnceExecuteOnce(&g_cancel_init_once, init_cancel_cs_callback, NULL, NULL);
}
#else
static pthread_mutex_t g_cancel_mutex;
static pthread_once_t g_cancel_once = PTHREAD_ONCE_INIT;

static void init_cancel_mutex(void) {
    pthread_mutex_init(&g_cancel_mutex, NULL);
}

// Wave A4: Safe initialization gate avoiding static initialization ordering races on POSIX
static void ensure_cancel_cs(void) {
    pthread_once(&g_cancel_once, init_cancel_mutex);
}
#endif

void datara_rt_schedule_cancel(int64_t region_id) {
    if (region_id == 0) return;
    ensure_cancel_cs();
#ifdef _WIN32
    EnterCriticalSection(&g_cancel_cs);
    if (g_cancelled_count < MAX_CANCELLED_REGIONS) {
        g_cancelled_regions[g_cancelled_count++] = region_id;
    }
    LeaveCriticalSection(&g_cancel_cs);
#else
    pthread_mutex_lock(&g_cancel_mutex);
    if (g_cancelled_count < MAX_CANCELLED_REGIONS) {
        g_cancelled_regions[g_cancelled_count++] = region_id;
    }
    pthread_mutex_unlock(&g_cancel_mutex);
#endif
}

static int is_region_cancelled(int64_t region_id) {
    if (region_id == 0) return 0;
    ensure_cancel_cs();
    int cancelled = 0;
#ifdef _WIN32
    EnterCriticalSection(&g_cancel_cs);
    for (int i = 0; i < g_cancelled_count; i++) {
        if (g_cancelled_regions[i] == region_id) {
            cancelled = 1;
            break;
        }
    }
    LeaveCriticalSection(&g_cancel_cs);
#else
    pthread_mutex_lock(&g_cancel_mutex);
    for (int i = 0; i < g_cancelled_count; i++) {
        if (g_cancelled_regions[i] == region_id) {
            cancelled = 1;
            break;
        }
    }
    pthread_mutex_unlock(&g_cancel_mutex);
#endif
    return cancelled;
}

// --- Dynamic Ready Queue (for dynamic/non-deterministic tasks) ---
typedef struct {
    uint32_t* queue;
    int head;
    int tail;
    int count;
    int capacity;
#ifdef _WIN32
    CRITICAL_SECTION cs;
    CONDITION_VARIABLE cv;
#else
    pthread_mutex_t mutex;
    pthread_cond_t cond;
#endif
} DynamicReadyQueue;

static void queue_init(DynamicReadyQueue* q, int capacity) {
    q->queue = (uint32_t*)malloc(sizeof(uint32_t) * (size_t)capacity);
    q->head = 0;
    q->tail = 0;
    q->count = 0;
    q->capacity = capacity;
#ifdef _WIN32
    InitializeCriticalSection(&q->cs);
    InitializeConditionVariable(&q->cv);
#else
    pthread_mutex_init(&q->mutex, NULL);
    pthread_cond_init(&q->cond, NULL);
#endif
}

static void queue_free(DynamicReadyQueue* q) {
#ifdef _WIN32
    DeleteCriticalSection(&q->cs);
#else
    pthread_mutex_destroy(&q->mutex);
    pthread_cond_destroy(&q->cond);
#endif
    if (q->queue) {
        free(q->queue);
        q->queue = NULL;
    }
}

static void queue_push(DynamicReadyQueue* q, uint32_t task_id) {
#ifdef _WIN32
    EnterCriticalSection(&q->cs);
    g_sched_mutex_queue_pushes++;
    if (q->count < q->capacity) {
        q->queue[q->tail] = task_id;
        q->tail = (q->tail + 1) % q->capacity;
        q->count++;
        WakeConditionVariable(&q->cv);
    }
    LeaveCriticalSection(&q->cs);
#else
    pthread_mutex_lock(&q->mutex);
    g_sched_mutex_queue_pushes++;
    if (q->count < q->capacity) {
        q->queue[q->tail] = task_id;
        q->tail = (q->tail + 1) % q->capacity;
        q->count++;
        pthread_cond_signal(&q->cond);
    }
    pthread_mutex_unlock(&q->mutex);
#endif
}

static int queue_pop(DynamicReadyQueue* q, uint32_t* out_task_id) {
#ifdef _WIN32
    EnterCriticalSection(&q->cs);
    if (q->count == 0) {
        LeaveCriticalSection(&q->cs);
        return 0;
    }
    *out_task_id = q->queue[q->head];
    q->head = (q->head + 1) % q->capacity;
    q->count--;
    LeaveCriticalSection(&q->cs);
    return 1;
#else
    pthread_mutex_lock(&q->mutex);
    if (q->count == 0) {
        pthread_mutex_unlock(&q->mutex);
        return 0;
    }
    *out_task_id = q->queue[q->head];
    q->head = (q->head + 1) % q->capacity;
    q->count--;
    pthread_mutex_unlock(&q->mutex);
    return 1;
#endif
}

// Comparison function for sorting task IDs (ensures 100% deterministic tie-breaking)
static int cmp_u32(const void* a, const void* b) {
    uint32_t ua = *(const uint32_t*)a;
    uint32_t ub = *(const uint32_t*)b;
    return (ua > ub) - (ua < ub);
}

// Wave Execution Context for flat parallel_for
typedef struct {
    DataraRtTaskNode* nodes;
    uint32_t* wave_task_indices;
} WaveRunContext;

static void wave_task_runner(int64_t idx, void* ctx) {
    WaveRunContext* w = (WaveRunContext*)ctx;
    uint32_t task_idx = w->wave_task_indices[idx];
    DataraRtTaskNode* node = &w->nodes[task_idx];
    node->status = 1; // running
    if (node->fn) {
        node->fn(node->ctx);
    }
    node->status = 2; // completed
}

// Primary Scheduler API implementation
int64_t datara_rt_schedule_run(int64_t node_count, DataraRtTaskNode* nodes, int64_t region_id) {
    if (node_count <= 0 || !nodes) {
        return 0;
    }

    if (is_region_cancelled(region_id)) {
        for (int64_t i = 0; i < node_count; i++) {
            nodes[i].status = 3; // cancelled
        }
        return -1;
    }

    // Check if the entire subgraph is deterministic
    int all_deterministic = 1;
    for (int64_t i = 0; i < node_count; i++) {
        if (!nodes[i].deterministic) {
            all_deterministic = 0;
            break;
        }
    }

    // 1. FAST PATH: Deterministic Subgraph
    // Uses flat parallel_for over Kahn topological wavefronts:
    // NO atomics on the critical path, NO work stealing, output order guaranteed,
    // and NEVER touches the mutex queue.
    if (all_deterministic) {
        // Map from task.id to index in nodes array
        // (if nodes[i].id == i, this is an identity map)
        int32_t* id_to_idx = (int32_t*)malloc(sizeof(int32_t) * (size_t)node_count);
        for (int64_t i = 0; i < node_count; i++) {
            id_to_idx[i] = (int32_t)i;
        }

        // Compute in-degrees and build adjacency lists
        uint32_t* in_degree = (uint32_t*)calloc((size_t)node_count, sizeof(uint32_t));
        uint32_t* succ_count = (uint32_t*)calloc((size_t)node_count, sizeof(uint32_t));
        uint32_t* succ_cap = (uint32_t*)malloc(sizeof(uint32_t) * (size_t)node_count);
        uint32_t** succ_lists = (uint32_t**)malloc(sizeof(uint32_t*) * (size_t)node_count);

        for (int64_t i = 0; i < node_count; i++) {
            in_degree[i] = nodes[i].dep_count;
            succ_cap[i] = 4;
            succ_count[i] = 0;
            succ_lists[i] = (uint32_t*)malloc(sizeof(uint32_t) * succ_cap[i]);
        }

        for (int64_t i = 0; i < node_count; i++) {
            for (uint32_t d = 0; d < nodes[i].dep_count; d++) {
                uint32_t dep_id = nodes[i].deps[d];
                // Find node index corresponding to dep_id
                int64_t dep_idx = -1;
                if (dep_id < (uint32_t)node_count && nodes[dep_id].id == dep_id) {
                    dep_idx = (int64_t)dep_id;
                } else {
                    for (int64_t k = 0; k < node_count; k++) {
                        if (nodes[k].id == dep_id) {
                            dep_idx = k;
                            break;
                        }
                    }
                }

                if (dep_idx >= 0 && dep_idx < node_count) {
                    if (succ_count[dep_idx] >= succ_cap[dep_idx]) {
                        succ_cap[dep_idx] *= 2;
                        succ_lists[dep_idx] = (uint32_t*)realloc(succ_lists[dep_idx], sizeof(uint32_t) * succ_cap[dep_idx]);
                    }
                    succ_lists[dep_idx][succ_count[dep_idx]++] = (uint32_t)i;
                }
            }
        }

        // Wave buffers
        uint32_t* cur_wave = (uint32_t*)malloc(sizeof(uint32_t) * (size_t)node_count);
        uint32_t* next_wave = (uint32_t*)malloc(sizeof(uint32_t) * (size_t)node_count);
        int cur_wave_len = 0;

        // Wave 0: all nodes with in_degree == 0
        for (int64_t i = 0; i < node_count; i++) {
            if (in_degree[i] == 0) {
                cur_wave[cur_wave_len++] = (uint32_t)i;
            }
        }
        qsort(cur_wave, (size_t)cur_wave_len, sizeof(uint32_t), cmp_u32);

        int64_t completed_count = 0;

        while (cur_wave_len > 0) {
            if (is_region_cancelled(region_id)) {
                for (int64_t i = 0; i < node_count; i++) {
                    if (nodes[i].status == 0) {
                        nodes[i].status = 3; // cancelled
                    }
                }
                break;
            }

            g_sched_wave_executions++;

            // Execute wave concurrently using flat parallel_for
            WaveRunContext wctx;
            wctx.nodes = nodes;
            wctx.wave_task_indices = cur_wave;
            datara_rt_parallel_for(0, (int64_t)cur_wave_len, wave_task_runner, &wctx);

            completed_count += cur_wave_len;

            // Resolve next wave
            int next_wave_len = 0;
            for (int i = 0; i < cur_wave_len; i++) {
                uint32_t completed_idx = cur_wave[i];
                for (uint32_t s = 0; s < succ_count[completed_idx]; s++) {
                    uint32_t succ_idx = succ_lists[completed_idx][s];
                    if (in_degree[succ_idx] > 0) {
                        in_degree[succ_idx]--;
                        if (in_degree[succ_idx] == 0) {
                            next_wave[next_wave_len++] = succ_idx;
                        }
                    }
                }
            }

            qsort(next_wave, (size_t)next_wave_len, sizeof(uint32_t), cmp_u32);

            // Swap waves
            uint32_t* tmp = cur_wave;
            cur_wave = next_wave;
            next_wave = tmp;
            cur_wave_len = next_wave_len;
        }

        // Cleanup
        for (int64_t i = 0; i < node_count; i++) {
            free(succ_lists[i]);
        }
        free(succ_lists);
        free(succ_cap);
        free(succ_count);
        free(in_degree);
        free(id_to_idx);
        free(cur_wave);
        free(next_wave);

        return completed_count;
    }

    // 2. DYNAMIC PATH: Non-deterministic / Dynamic Tasks
    // Dispatched through the mutex ready-queue.
    DynamicReadyQueue ready_q;
    queue_init(&ready_q, (int)node_count + 16);

    uint32_t* in_degree = (uint32_t*)calloc((size_t)node_count, sizeof(uint32_t));
    for (int64_t i = 0; i < node_count; i++) {
        in_degree[i] = nodes[i].dep_count;
        if (in_degree[i] == 0) {
            queue_push(&ready_q, (uint32_t)i);
        }
    }

    int64_t completed = 0;
    while (completed < node_count) {
        if (is_region_cancelled(region_id)) {
            for (int64_t i = 0; i < node_count; i++) {
                if (nodes[i].status == 0) nodes[i].status = 3;
            }
            break;
        }

        uint32_t task_idx = 0;
        if (queue_pop(&ready_q, &task_idx)) {
            DataraRtTaskNode* node = &nodes[task_idx];
            node->status = 1; // running
            if (node->fn) {
                node->fn(node->ctx);
            }
            node->status = 2; // completed
            completed++;

            // Check if any dependent tasks are now ready
            for (int64_t i = 0; i < node_count; i++) {
                if (in_degree[i] > 0) {
                    for (uint32_t d = 0; d < nodes[i].dep_count; d++) {
                        if (nodes[i].deps[d] == node->id) {
                            in_degree[i]--;
                            if (in_degree[i] == 0) {
                                queue_push(&ready_q, (uint32_t)i);
                            }
                            break;
                        }
                    }
                }
            }
        } else {
            // Wave A4: If the ready queue is empty while completed < node_count,
            // uncompleted tasks either form a dependency cycle or await external/async triggers.
            // Rather than hanging indefinitely or deadlocking, the scheduler cleanly breaks and
            // returns `completed` (number of completed tasks), keeping remaining tasks in status 0 (pending).
            break;
        }
    }

    free(in_degree);
    queue_free(&ready_q);

    return completed;
}

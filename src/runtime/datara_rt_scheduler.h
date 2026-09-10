#ifndef DATARA_RT_SCHEDULER_H
#define DATARA_RT_SCHEDULER_H

#include <stdint.h>
#include <stddef.h>

#ifdef __cplusplus
extern "C" {
#endif

// Effect classes matching the compiler's ScheduleProof
typedef enum {
    DATARA_SCHED_EFFECT_PURE = 0,
    DATARA_SCHED_EFFECT_IO = 1,
    DATARA_SCHED_EFFECT_NETWORK = 2,
    DATARA_SCHED_EFFECT_PARALLEL = 3
} DataraSchedEffectClass;

// Priority classes from cost model
typedef enum {
    DATARA_SCHED_PRIORITY_COLD = 0,
    DATARA_SCHED_PRIORITY_HOT = 1
} DataraSchedPriority;

// Worker pool kind
typedef enum {
    DATARA_SCHED_POOL_CPU = 0,
    DATARA_SCHED_POOL_IO = 1
} DataraSchedPool;

// Task function pointer: receives void* ctx (user context / arguments)
typedef void (*DataraSchedTaskFn)(void* ctx);

// Task Node representing a task in the runtime DAG
typedef struct {
    uint32_t id;
    uint32_t effect_class;    // DataraSchedEffectClass
    uint32_t priority;        // DataraSchedPriority
    uint32_t pool;            // DataraSchedPool
    uint32_t deterministic;   // 1 = deterministic, 0 = dynamic / non-deterministic
    uint32_t dep_count;       // Number of prerequisite task IDs
    uint32_t* deps;           // Array of prerequisite task IDs
    DataraSchedTaskFn fn;     // Task function pointer
    void* ctx;                // Context / arg pointer
    void* result;             // Result pointer (if any)
    int32_t status;           // 0 = pending, 1 = running, 2 = completed, 3 = cancelled
} DataraRtTaskNode;

// Primary scheduler API:
// datara_rt_schedule_run executes the given tasks respecting their dependencies.
// For deterministic tasks, it executes Kahn topological waves via flat parallel_for.
// For dynamic tasks, it queues them through a mutex-guarded ready-queue.
int64_t datara_rt_schedule_run(int64_t node_count, DataraRtTaskNode* nodes, int64_t region_id);

// Cancel all pending tasks belonging to the given region
void datara_rt_schedule_cancel(int64_t region_id);

// Inspection and determinism verification counters (used in Phase 4 determinism tests)
uint64_t datara_rt_scheduler_mutex_queue_pushes(void);
uint64_t datara_rt_scheduler_wave_executions(void);
void     datara_rt_scheduler_reset_stats(void);

#ifdef __cplusplus
}
#endif

#endif // DATARA_RT_SCHEDULER_H

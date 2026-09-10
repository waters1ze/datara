#ifndef TEST_CIMPORT_H
#define TEST_CIMPORT_H

#define TEST_OK 0
#define TEST_ERROR 1
#define BUFFER_SIZE 1024

enum Status {
    STATUS_IDLE = 100,
    STATUS_RUNNING = 200,
    STATUS_DONE = 300
};

struct OpaqueSession;
typedef struct OpaqueSession OpaqueSession;
typedef int CustomInt;

int GetCurrentProcessId(void);

#endif

#ifndef MYLIB_H
#define MYLIB_H

#include <stdio.h>

#define MAX_BUFFER 256
#define MIN(a, b) ((a) < (b) ? (a) : (b))
#define MAX(a, b) ((a) > (b) ? (a) : (b))

#define LOG(fmt, ...) fprintf(stderr, fmt, __VA_ARGS__)

#define SWAP(type, a, b) \
    do {                 \
        type tmp = (a);  \
        (a) = (b);       \
        (b) = tmp;       \
    } while (0)

#ifdef DEBUG
#define TRACE(msg) fprintf(stderr, "trace: %s\n", msg)
#else
#define TRACE(msg)
#endif

#if defined(__linux__)
#define PLATFORM "linux"
#elif defined(_WIN32)
#define PLATFORM "windows"
#else
#define PLATFORM "unknown"
#endif

#pragma pack(push, 1)
struct packed_header {
    int id;
    char kind;
};
#pragma pack(pop)

int clampi(int x);

#endif /* MYLIB_H */

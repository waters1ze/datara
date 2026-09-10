#ifndef TEST_ABI_LIB_H
#define TEST_ABI_LIB_H

typedef struct {
    long long x;
    long long y;
} Point;

typedef struct {
    int a;
    int b;
} Pair32;

typedef long long (*math_cb)(long long);

Point make_point(long long x, long long y);
long long sum_point(Point p);
long long sum_two_points(Point a, Point b);
Point add_points(Point a, Point b);

Pair32 make_pair(int a, int b);
long long sum_pair(Pair32 p);

long long apply_callback(long long val, math_cb cb);
long long apply_callback_inline(long long val, long long (*cb)(long long));

#endif

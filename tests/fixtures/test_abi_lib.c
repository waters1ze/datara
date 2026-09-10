#include "test_abi_lib.h"

Point make_point(long long x, long long y) {
    Point p;
    p.x = x;
    p.y = y;
    return p;
}

long long sum_point(Point p) {
    return p.x + p.y;
}

long long sum_two_points(Point a, Point b) {
    return a.x + a.y + b.x + b.y;
}

Point add_points(Point a, Point b) {
    Point res;
    res.x = a.x + b.x;
    res.y = a.y + b.y;
    return res;
}

Pair32 make_pair(int a, int b) {
    Pair32 p;
    p.a = a;
    p.b = b;
    return p;
}

long long sum_pair(Pair32 p) {
    return (long long)p.a + (long long)p.b;
}

long long apply_callback(long long val, math_cb cb) {
    if (cb) {
        return cb(val);
    }
    return 0;
}

long long apply_callback_inline(long long val, long long (*cb)(long long)) {
    if (cb) {
        return cb(val);
    }
    return 0;
}

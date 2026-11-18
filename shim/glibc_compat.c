/* Let a binary built against glibc 2.38+ headers run on older glibc (e.g. Ubuntu 22.04, glibc 2.35).
   C++ code compiled with _GNU_SOURCE calls the C23 names below; they behave the same for our inputs. */
#undef _GNU_SOURCE
#include <stdarg.h>
#include <stdint.h>
#include <stdio.h>
#include <stdlib.h>
#include <sys/random.h>

long __isoc23_strtol(const char *s, char **e, int b) { return strtol(s, e, b); }
unsigned long __isoc23_strtoul(const char *s, char **e, int b) { return strtoul(s, e, b); }
long long __isoc23_strtoll(const char *s, char **e, int b) { return strtoll(s, e, b); }
unsigned long long __isoc23_strtoull(const char *s, char **e, int b) { return strtoull(s, e, b); }
int __isoc23_sscanf(const char *s, const char *fmt, ...) {
    va_list ap;
    va_start(ap, fmt);
    int r = vsscanf(s, fmt, ap);
    va_end(ap);
    return r;
}
uint32_t arc4random(void) {
    uint32_t x = 0;
    while (getrandom(&x, sizeof x, 0) != (ssize_t)sizeof x) {}
    return x;
}

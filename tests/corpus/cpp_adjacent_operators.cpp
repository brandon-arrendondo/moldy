// Two tokens written apart stay apart where together they would read as
// one; a literal and its suffix, written together, stay together.
#include <cstddef>
#include <string>

struct A {
    void *operator new(std::size_t n);
    void operator delete(void *p);
    A &operator+=(const A &o);
    bool operator<=(const A &o) const;
    int operator->*(int m);
};

std::string operator""_s(const char *p, std::size_t n);
std::string operator"" _t(const char *p, std::size_t n);
unsigned long long operator""_km(unsigned long long v);

int adjacent(int a, int b, int *q)
{
    int r = a / *q;
    r = a - -b;
    r = a + +b;
    r = - -a;
    auto s = "a"_s;
    auto k = 12_km;
    auto w = u8"a" "b";
    return r + static_cast<int>(s.size() + k + sizeof w);
}

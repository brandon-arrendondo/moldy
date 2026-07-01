#include <vector>

enum class Color {
    Red,
    Green,
    Blue,
};

int sum_vector(const std::vector<int> &values)
{
    int total = 0;
    for (const auto &value : values) {
        total += value;
    }
    return total;
}

auto make_adder(int base)
{
    return [base](int x) {
        return base + x;
    };
}

void apply_all(std::vector<int> &values)
{
    auto doubler = [](int &x) {
        x *= 2;
    };
    for (auto &v : values) {
        doubler(v);
    }
}

int *make_null()
{
    return nullptr;
}

Color parse_color(const std::string &name)
{
    if (name == "red") {
        return Color::Red;
    }
    return Color::Blue;
}

class Resource {
public:
    Resource() = default;
    Resource(const Resource &) = delete;
    Resource &operator=(const Resource &) = delete;
    ~Resource() = default;
};

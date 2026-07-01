class Vector2 {
public:
    Vector2(double x, double y) : x_(x), y_(y) {}

    Vector2 operator+(const Vector2 &other) const
    {
        return Vector2(x_ + other.x_, y_ + other.y_);
    }

    Vector2 &operator+=(const Vector2 &other)
    {
        x_ += other.x_;
        y_ += other.y_;
        return *this;
    }

    bool operator==(const Vector2 &other) const
    {
        return x_ == other.x_ && y_ == other.y_;
    }

    double &operator[](int index)
    {
        return index == 0 ? x_ : y_;
    }

    friend std::ostream &operator<<(std::ostream &os, const Vector2 &v);

private:
    double x_;
    double y_;
};

std::ostream &operator<<(std::ostream &os, const Vector2 &v)
{
    os << "(" << v.x_ << ", " << v.y_ << ")";
    return os;
}

class Counter {
public:
    Counter &operator++()
    {
        ++value_;
        return *this;
    }

    Counter operator++(int)
    {
        Counter copy = *this;
        ++value_;
        return copy;
    }

private:
    int value_ = 0;
};

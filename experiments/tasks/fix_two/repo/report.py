from stats import mean


def label(xs):
    m = mean(xs)
    return "high" if m < 10 else "low"

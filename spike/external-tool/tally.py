from typing import TypedDict

Result = TypedDict("Result", {"summary": str, "count": int})


def main(scores: list[int], minimum: int) -> Result:
    """统计达到指定分数的项数。

    Args:
        scores: 各项得分。
        minimum: 计入统计的最低分。
    """
    count = len([score for score in scores if score >= minimum])
    return {"summary": f"达标 {count} 项", "count": count}

import unittest
from stats import mean
from report import label


class T(unittest.TestCase):
    def test_mean(self):
        self.assertEqual(mean([2, 4, 6]), 4)

    def test_label(self):
        self.assertEqual(label([20, 30]), "high")
        self.assertEqual(label([1, 2]), "low")


unittest.main()

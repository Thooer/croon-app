import pathlib, subprocess, sys, tempfile
sys.path.insert(0, str(pathlib.Path(__file__).resolve().parents[3]))
from experiments.checking import verdict

attempt = pathlib.Path(sys.argv[1])
out = attempt / "out"
hidden = """import unittest
import a


class T(unittest.TestCase):
    def test_add(self):
        self.assertEqual(a.add(2, 3), 5)
        self.assertEqual(a.add(-1, 1), 0)
        self.assertEqual(a.add(0, 0), 0)
        self.assertEqual(a.add(10, -4), 6)


unittest.main()
"""
with tempfile.TemporaryDirectory() as t:
    t = pathlib.Path(t)
    for f in out.glob("*.py"):
        (t / f.name).write_text(f.read_text())
    (t / "hidden_test.py").write_text(hidden)
    r = subprocess.run([sys.executable, "hidden_test.py"], capture_output=True, text=True, timeout=120, cwd=t)
verdict(r.returncode == 0, "隐藏测试通过" if r.returncode == 0 else "隐藏测试失败", tail=(r.stdout + r.stderr)[-400:])

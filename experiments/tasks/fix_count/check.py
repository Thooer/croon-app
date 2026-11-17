import pathlib, subprocess, sys, tempfile
sys.path.insert(0, str(pathlib.Path(__file__).resolve().parents[3]))
from experiments.checking import load, made, verdict

attempt = pathlib.Path(sys.argv[1])
out = attempt / "out"
hidden = 'import unittest\nfrom text import count_lines\n\n\nclass T(unittest.TestCase):\n    def test_lines(self):\n        self.assertEqual(count_lines(""), 0)\n        self.assertEqual(count_lines("a"), 1)\n        self.assertEqual(count_lines("a\\n"), 1)\n        self.assertEqual(count_lines("a\\nb"), 2)\n        self.assertEqual(count_lines("a\\nb\\n"), 2)\n        self.assertEqual(count_lines("\\n"), 1)\n\n\nunittest.main()\n'
with tempfile.TemporaryDirectory() as t:
    t = pathlib.Path(t)
    for f in out.rglob("*"):
        if f.is_file() and ".git" not in f.parts:
            target = t / f.relative_to(out)
            target.parent.mkdir(parents=True, exist_ok=True)
            target.write_bytes(f.read_bytes())
    (t / "hidden_test.py").write_text(hidden)
    r = subprocess.run([sys.executable, "hidden_test.py"], capture_output=True, text=True, timeout=120, cwd=t)
if r.returncode != 0:
    verdict(False, "隐藏测试失败", stage="wrong", tail=(r.stdout + r.stderr)[-400:])
else:
    verdict(True, "隐藏测试通过")

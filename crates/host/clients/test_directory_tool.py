"""目录工具的接入验收。需要已构建的 croon-app、任务镜像与本目录的 Python 依赖。"""

import asyncio
import json
import os
from pathlib import Path
import subprocess
import sys
import tempfile
import unittest

from mcp import ClientSession, StdioServerParameters
from mcp.client.stdio import stdio_client

ROOT = Path(__file__).resolve().parents[3]
CLIENT = Path(__file__).with_name("directory_tool.py").resolve()
BINARY = ROOT / "target/debug/croon-app"
PACKAGE = ROOT / "examples/shipping_labels"


class ShippingLabels(unittest.IsolatedAsyncioTestCase):
    def setUp(self):
        self.work = Path(self.enterContext(tempfile.TemporaryDirectory()))
        self.env = {
            "XDG_STATE_HOME": str(self.work / "state"),
            "XDG_CONFIG_HOME": str(self.work / "config"),
            "PYTHONDONTWRITEBYTECODE": "1",
        }

    def setup_batch(self, batch, label, input_dir, results_dir):
        root = self.work / batch
        source = root / input_dir
        source.mkdir(parents=True)
        (source / "label.txt").write_bytes(label)
        (source / "keep.txt").write_bytes(b"keep\n")
        binding = root / "binding.json"
        binding.write_text(json.dumps({"input_dir": input_dir, "results_dir": results_dir}))
        args = ["-B", str(CLIENT), "--binary", str(BINARY), "--package", str(PACKAGE), "--binding", str(binding)]
        return args, source, root / results_dir

    def verify(self, receipt, source, label, results, count):
        destination = Path(receipt["directory"])
        self.assertEqual(destination.parent, results)
        self.assertEqual(receipt["value"]["copies"], count)
        self.assertTrue(receipt["value"]["snapshot"].startswith("sha256:"))
        self.assertEqual((destination / receipt["value"]["file"]).read_bytes(), label * count)
        self.assertEqual((destination / "keep.txt").read_bytes(), b"keep\n")
        self.assertEqual({p.name for p in source.iterdir()}, {"label.txt", "keep.txt"})
        self.assertEqual((source / "label.txt").read_bytes(), label)
        self.assertEqual((source / "keep.txt").read_bytes(), b"keep\n")
        return destination

    async def test_two_bindings_independent_outputs_and_tool_error(self):
        north, south = b"North Dock\nSKU-1\n", b"South Dock\nSKU-2\n"
        cli, source_b, results_b = self.setup_batch("B", south, "inbox", "jobs")
        proc = await asyncio.to_thread(
            subprocess.run, [sys.executable, *cli, "call", '{"copies":1}'], cwd=self.work,
            env=os.environ | self.env, capture_output=True, text=True, timeout=60,
        )
        self.assertEqual(proc.returncode, 0, proc.stderr)
        self.verify(json.loads(proc.stdout), source_b, south, results_b, 1)

        args, source_a, results_a = self.setup_batch("A", north, "input", "print-runs")
        server = StdioServerParameters(command=sys.executable, args=[*args, "serve"], env=self.env, cwd=self.work)
        async with stdio_client(server) as (reader, writer), ClientSession(reader, writer) as client:
            await client.initialize()
            (spec,) = (await client.list_tools()).tools
            self.assertEqual(spec.name, "shipping_labels")
            self.assertEqual(spec.inputSchema["required"], ["copies"])
            self.assertEqual(set(spec.inputSchema["properties"]), {"copies"})
            delivered = []
            for count in (2, 3):
                reply = await client.call_tool(spec.name, {"copies": count})
                self.assertFalse(reply.isError, reply.content)
                self.assertEqual(json.loads(reply.content[0].text), reply.structuredContent)
                delivered.append(self.verify(reply.structuredContent, source_a, north, results_a, count))
            for invalid in ({"copies": 0}, {"copies": "two"}, {"copies": 2, "snapshot": "other"}):
                reply = await client.call_tool(spec.name, invalid)
                self.assertTrue(reply.isError, reply.content)
                self.assertIsNone(reply.structuredContent)
            self.assertNotEqual(*delivered)
            self.assertEqual(set(results_a.iterdir()), set(delivered))
            self.assertEqual((delivered[0] / "print.txt").read_bytes(), north * 2)

    def test_missing_label_reports_the_command_failure_and_discards_partial_output(self):
        args, source, results = self.setup_batch("missing", b"unused", "input", "results")
        (source / "label.txt").unlink()
        proc = subprocess.run([sys.executable, *args, "call", '{"copies":2}'], cwd=self.work,
                              env=os.environ | self.env, capture_output=True, text=True, timeout=60)
        self.assertNotEqual(proc.returncode, 0)
        self.assertEqual(proc.stdout, "")
        self.assertIn("生成标签失败", proc.stderr)
        self.assertIn("label.txt", proc.stderr)
        self.assertEqual(list(results.iterdir()), [])
        self.assertEqual({p.name for p in source.iterdir()}, {"keep.txt"})

    def test_binding_cannot_put_results_in_the_input(self):
        args, source, _ = self.setup_batch("overlap", b"x", "in", "in/new")
        proc = subprocess.run([sys.executable, *args, "describe"], env=os.environ | self.env,
                              capture_output=True, text=True, timeout=30)
        self.assertNotEqual(proc.returncode, 0)
        self.assertEqual(proc.stdout, "")
        self.assertIn("输入目录之外", proc.stderr)
        self.assertFalse((source / "new").exists())


if __name__ == "__main__":
    unittest.main()

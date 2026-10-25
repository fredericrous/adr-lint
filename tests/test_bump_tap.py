import pathlib
import subprocess
import sys
import tempfile
import unittest

ROOT = pathlib.Path(__file__).resolve().parent.parent
SCRIPT = ROOT / "scripts" / "bump-tap.py"
SEED = ROOT / "packaging" / "homebrew" / "adr-lint.rb"
TARGETS = [
    "aarch64-apple-darwin",
    "x86_64-apple-darwin",
    "aarch64-unknown-linux-gnu",
    "x86_64-unknown-linux-gnu",
]


def run(version, sums, formula):
    return subprocess.run(
        [sys.executable, "-I", str(SCRIPT), version, str(sums), str(formula)],
        capture_output=True,
        text=True,
    )


class BumpTap(unittest.TestCase):
    def setUp(self):
        self._dir = tempfile.TemporaryDirectory()
        self.addCleanup(self._dir.cleanup)
        self.dir = pathlib.Path(self._dir.name)
        self.formula = self.dir / "adr-lint.rb"
        self.formula.write_text(SEED.read_text())
        self.sums = self.dir / "SHA256SUMS"
        self.sums.write_text(
            "".join(
                f"{str(i + 1) * 64}  adr-lint-1.2.3-{t}.tar.gz\n"
                for i, t in enumerate(TARGETS)
            )
        )

    def test_rewrites_the_seed_formula(self):
        done = run("v1.2.3", self.sums, self.formula)
        self.assertEqual(done.returncode, 0, done.stderr)
        text = self.formula.read_text()
        self.assertIn('version "1.2.3"', text)
        for i, t in enumerate(TARGETS):
            self.assertIn(
                f"download/v1.2.3/adr-lint-1.2.3-{t}.tar.gz\"\n"
                f"      sha256 \"{str(i + 1) * 64}\"",
                text,
            )
        self.assertNotIn("0.1.0-", text)

    def test_refuses_a_formula_with_a_missing_pair(self):
        lines = SEED.read_text().splitlines(keepends=True)
        idx = next(i for i, l in enumerate(lines) if "x86_64-apple-darwin" in l)
        del lines[idx : idx + 2]
        self.formula.write_text("".join(lines))
        done = run("1.2.3", self.sums, self.formula)
        self.assertNotEqual(done.returncode, 0)
        self.assertIn("expected 4 url/sha pairs", done.stderr)

    def test_refuses_checksums_of_another_version(self):
        done = run("1.2.4", self.sums, self.formula)
        self.assertNotEqual(done.returncode, 0)
        self.assertIn("wrong release", done.stderr)


if __name__ == "__main__":
    unittest.main()

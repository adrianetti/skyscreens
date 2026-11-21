import sys
import unittest
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent))

from commit_msg import problems


class CommitMessageTests(unittest.TestCase):
    def test_accepts_the_project_style(self):
        for subject in [
            "feat(xr): decode into a surface layer",
            "fix(mac): sign with a stable identity",
            "docs: agent guide",
            "refactor(link)!: split the server from the C interface",
            "ci: run the APK build",
        ]:
            self.assertEqual(problems(subject), [], subject)

    def test_rejects_everything_else(self):
        for subject in [
            "Update stuff",
            "feat:missing space",
            "feature(xr): wrong type",
            "fix(Mac): uppercase scope",
            "docs: ends with a period.",
            "feat: " + "x" * 100,
            "fixup! feat: something",
        ]:
            self.assertNotEqual(problems(subject), [], subject)


if __name__ == "__main__":
    unittest.main()

"""Guards for workflow settings that only misbehave in a real release run."""
import pathlib, re, unittest

ROOT = pathlib.Path(__file__).resolve().parent.parent

class E2eConcurrency(unittest.TestCase):
    def setUp(self):
        self.text = (ROOT / ".github/workflows/e2e.yml").read_text(encoding="utf-8")

    def test_called_run_is_never_cancelled(self):
        # e2e.yml is also called by release.yml and preview.yml; there github.workflow
        # is the caller's name. cancel-in-progress must depend on it, and the group of a
        # called run must be unique (run id), or a second dispatch cancels the release E2E.
        cancel = re.search(r"^\s*cancel-in-progress:\s*(.+)$", self.text, re.M).group(1)
        group = re.search(r"^\s*group:\s*(.+)$", self.text, re.M).group(1)
        name = re.search(r"^name:\s*(.+)$", self.text, re.M).group(1).strip()
        self.assertNotEqual(cancel.strip(), "true")
        self.assertIn("github.workflow", cancel)
        self.assertIn(name, cancel)
        self.assertIn("github.run_id", group)

class DockerWebPublish(unittest.TestCase):
    def test_publish_jobs_are_serialised_and_never_cancelled(self):
        text = (ROOT / ".github/workflows/docker-web.yml").read_text(encoding="utf-8")
        job = text[text.index("\n  publish:"):]
        self.assertRegex(job, r"concurrency:\s*\n\s+group:\s*docker-web-publish\s*\n\s+cancel-in-progress:\s*false")
        # The latest decision sits in the same job as the push, after the serialisation.
        self.assertLess(job.index("HIGHEST="), job.index("docker push"))

if __name__ == "__main__":
    unittest.main()

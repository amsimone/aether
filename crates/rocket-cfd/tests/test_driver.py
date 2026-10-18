"""Pure adapter tests; no solver/Gmsh installation or fabricated CFD success needed."""
import importlib.util
from pathlib import Path
import tempfile
import unittest

SOURCE = Path(__file__).parents[1] / "src" / "driver.py"
spec = importlib.util.spec_from_file_location("aether_cfd_driver", SOURCE)
driver = importlib.util.module_from_spec(spec)
spec.loader.exec_module(driver)

class DriverContractTests(unittest.TestCase):
    def test_environment_setup_receives_no_solver_arguments_and_preserves_quoting(self):
        with tempfile.TemporaryDirectory(prefix="aether cfd ") as folder:
            root = Path(folder)
            bashrc = root / "environment file.sh"
            bashrc.write_text('test "$#" -eq 0 || return 91\n')
            literal = 'literal $HOME; touch should-not-exist'
            result = driver.foam_command(root, bashrc, "printf", "%s", literal)
            self.assertEqual(result, literal)
            self.assertFalse((root / "should-not-exist").exists())

    def test_failed_environment_setup_is_not_ignored(self):
        with tempfile.TemporaryDirectory() as folder:
            root = Path(folder)
            bashrc = root / "broken.sh"
            bashrc.write_text("return 92\n")
            with self.assertRaises(RuntimeError):
                driver.foam_command(root, bashrc, "printf", "must not execute")
            self.assertNotIn("must not execute", (root / "printf.log").read_text())

    def test_force_and_moment_axes_are_converted_to_the_declared_body_frame(self):
        with tempfile.TemporaryDirectory() as folder:
            root = Path(folder)
            output = root / "postProcessing" / "coefficients" / "0"
            output.mkdir(parents=True)
            lines = ["# Time Cd Cs Cl CmRoll CmPitch CmYaw"]
            lines.extend(f"{i} 0.4 0.1 -0.2 0.03 0.04 0.05" for i in range(50))
            (output / "coefficient.dat").write_text("\n".join(lines))
            row, iteration, drift, window = driver.coefficients(root)
            self.assertEqual(row, dict(ca=0.4, cy=0.1, cn=0.2, cl=-0.03, cm=0.04, cn_yaw=-0.05))
            self.assertEqual(iteration, 49)
            self.assertEqual(drift, 0)
            self.assertEqual(window, 40)

if __name__ == "__main__":
    unittest.main()

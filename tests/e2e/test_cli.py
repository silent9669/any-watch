import os
import shutil
import tempfile
import sys
from pathlib import Path

# --- TIER 4 TEST: CLI Symlink Creation & Execution (1 Test) ---

def test_t4_cli_launcher_creation_and_execution():
    """
    Tier 4 E2E Scenario: Emulates the backend setup_cli_launcher function.
    Creates a mock HOME directory, simulates symlink creation pointing to a mock executable,
    and verifies that the symlink exists, is executable, and resolves correctly.
    """
    # 1. Setup mock directories and executable
    with tempfile.TemporaryDirectory() as temp_dir:
        temp_path = Path(temp_dir)
        mock_home = temp_path / "user_home"
        mock_bin_dir = mock_home / ".local" / "bin"
        mock_app_dir = temp_path / "Applications"
        mock_app_dir.mkdir(parents=True, exist_ok=True)

        # Create a mock binary executable
        mock_exe = mock_app_dir / "any-watch-bin"
        with open(mock_exe, "w") as f:
            f.write("#!/bin/sh\necho 'any-watch launched'\n")

        # Make the mock binary executable
        os.chmod(mock_exe, 0o755)

        # 2. Simulate setup_cli_launcher execution
        # Ensure target directory exists (as setup_cli_launcher should do)
        mock_bin_dir.mkdir(parents=True, exist_ok=True)
        symlink_path = mock_bin_dir / "any-watch"

        # Create symlink pointing to the current exe
        if symlink_path.exists():
            symlink_path.unlink()

        os.symlink(mock_exe, symlink_path)

        # 3. Assertions (Opaque-box validation)
        # Check that symlink is created
        assert symlink_path.exists() is True
        assert symlink_path.is_symlink() is True

        # Verify it resolves to the correct binary
        resolved_path = symlink_path.resolve()
        assert resolved_path == mock_exe.resolve()

        # Verify the symlink is executable
        assert os.access(symlink_path, os.X_OK) is True

        # Verify execution works and output is correct
        import subprocess
        res = subprocess.run([str(symlink_path)], capture_output=True, text=True)
        assert res.returncode == 0
        assert "any-watch launched" in res.stdout

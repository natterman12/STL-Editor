"""Keep test runs out of the user's real settings (File > Recent, last folder...)."""
import os
import tempfile

os.environ["STLEDITOR_CONFIG_DIR"] = tempfile.mkdtemp(prefix="stleditor-test-config-")

"""Copy the licence files of every bundled third-party package into one folder."""
import os
import shutil
import sys
from importlib import metadata

PACKAGES = ["manifold3d", "numpy", "scipy", "scikit-image", "trimesh", "fast-simplification",
            "pywebview", "pythonnet", "clr-loader", "shapely", "networkx", "lxml", "pillow",
            "imageio", "lazy-loader", "packaging", "proxy-tools", "bottle", "typing-extensions",
            "cffi", "pycparser", "tifffile", "mini-racer"]
WEB_LICENSES = {"three.js (MIT)": "THREE-LICENSE.txt", "three-mesh-bvh (MIT)": "THREE-MESH-BVH-LICENSE.txt",
                "Support Fins (MIT)": "support-fins/LICENSE.txt"}


def main(dest):
    os.makedirs(dest, exist_ok=True)
    summary = []
    for name in PACKAGES:
        try:
            dist = metadata.distribution(name)
        except metadata.PackageNotFoundError:
            continue
        lic = dist.metadata.get("License-Expression") or dist.metadata.get("License") or ""
        lic = lic.splitlines()[0][:60] if lic else ""
        copied = 0
        for f in dist.files or []:
            base = os.path.basename(str(f)).upper()
            if any(k in base for k in ("LICENSE", "LICENCE", "COPYING", "NOTICE")):
                src = f.locate()
                if os.path.isfile(src):
                    out = os.path.join(dest, f"{dist.metadata['Name']}-{os.path.basename(str(f))}")
                    if not out.lower().endswith(".txt"):
                        out += ".txt"
                    shutil.copyfile(src, out)
                    copied += 1
        summary.append(f"{dist.metadata['Name']} {dist.version}  {lic}  ({copied} file(s))")
    here = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
    for label, fn in WEB_LICENSES.items():
        p = os.path.join(here, "web", "vendor", fn)
        if os.path.isfile(p):
            shutil.copyfile(p, os.path.join(dest, fn.replace("/", "-")))
            summary.append(label)
    with open(os.path.join(dest, "_THIRD-PARTY.txt"), "w", encoding="utf-8") as fh:
        fh.write("STL Editor bundles these open-source components. Their licence texts are in this folder.\n\n")
        fh.write("\n".join(summary) + "\n")
    print(f"licences: {len(summary)} components")


if __name__ == "__main__":
    main(sys.argv[1])

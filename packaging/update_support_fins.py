"""Vendors the Support Fins engine (https://github.com/gittrahan/support-fins, MIT) into
web/vendor/support-fins/, where both editions run it:

  engine.esm.js   the web edition's worker imports it
  engine.iife.js  the desktop app runs it in an embedded V8 (mini-racer)
  VERSION.json    which release it is, plus checksums the desktop app checks when it
                  downloads a newer engine at start-up (see stleditor/supportfins.py)
  LICENSE.txt     Support Fins' MIT licence

    python packaging/update_support_fins.py            # newest release (vX.Y.Z tag)
    python packaging/update_support_fins.py v1.0.2     # a given tag
    python packaging/update_support_fins.py --check    # only report; exit 10 if newer

It builds Support Fins' own plugin entry (plugins/shared/engine/bridge.js, which runs the
website's engine files unmodified) with esbuild, exactly as their plugins do. Needs git and
Node (npx fetches esbuild), or ESBUILD=/path/to/esbuild. Run packaging/build_web.py and the
tests afterwards; the "Support Fins update" GitHub workflow does all of it daily.
"""
import datetime
import hashlib
import json
import os
import re
import shutil
import subprocess
import sys
import tempfile

ROOT = os.path.abspath(os.path.join(os.path.dirname(__file__), ".."))
OUT = os.path.join(ROOT, "web", "vendor", "support-fins")
UPSTREAM = "https://github.com/gittrahan/support-fins.git"
# The integration contract: the bridge functions and options.json format this app uses.
# A release that changes them is refused here, so it can't break either edition.
API = 1
SCHEMA_VERSION = 1
NEEDED = ["computeFins", "drawWall", "optionsFromDialog", "optionVisible", "OPTIONS_SCHEMA", "reportLine",
          "computeFinsB64", "drawWallB64", "optionsSchemaJson", "optionsFromDialogJson"]


def run(cmd, **kw):
    r = subprocess.run(cmd, capture_output=True, text=True, **kw)
    if r.returncode:
        sys.exit(f"{os.path.basename(str(cmd[0]))} failed ({r.returncode}):\n{(r.stderr or r.stdout).strip()}")
    return r.stdout


def semver(tag):
    m = re.fullmatch(r"v(\d+)\.(\d+)\.(\d+)", tag)
    return tuple(int(x) for x in m.groups()) if m else None


def latest_tag():
    out = run(["git", "ls-remote", "--tags", "--refs", UPSTREAM])
    tags = [ln.split("refs/tags/")[1] for ln in out.splitlines() if "refs/tags/" in ln]
    tags = [t for t in tags if semver(t)]
    if not tags:
        sys.exit("No vX.Y.Z release tags found upstream.")
    return max(tags, key=semver)


def current():
    try:
        with open(os.path.join(OUT, "VERSION.json"), encoding="utf-8") as fh:
            return json.load(fh)
    except OSError:
        return None


def esbuild():
    if os.environ.get("ESBUILD"):
        return [os.environ["ESBUILD"]]
    if shutil.which("esbuild"):
        return ["esbuild"]
    return [shutil.which("npx") or "npx", "--yes", "esbuild@0.28"]


# Extras this app adds on top of the plugin entry, generated from Support Fins' own files so
# they follow every release. Optional: when a release changes the code they are made from,
# the extra is left out and the app falls back (a curve is then drawn as short straight walls).
CURVE_SIG = re.compile(r"export function drawWall\(positions, a, b, options = \{\}\) \{")
CURVE_CALL = "drawnWall(seat(a), seat(b), tris, 0, {"
ENTRY = """// written by STL Editor's packaging/update_support_fins.py: Support Fins' plugin entry,
// plus the extras STL Editor uses
export * from './bridge.js';
%s
"""
CURVE_EXPORTS = """import { b64ToBytes, bytesToB64 } from './bridge.js';
import { drawWallThrough } from './stle_draw_through.js';
export { drawWallThrough };
// a curved wall: a and b its ends, m a point it runs through (Support Fins' Curve tool)
export function drawWallThroughB64(soupB64, aJson, bJson, mJson, optionsJson) {
  const options = optionsJson ? JSON.parse(optionsJson) : {};
  const bytes = b64ToBytes(soupB64);
  const soup = bytes.byteOffset % 8 ? new Float64Array(bytes.slice().buffer) : new Float64Array(bytes.buffer, bytes.byteOffset, bytes.byteLength / 8);
  const res = drawWallThrough(soup, JSON.parse(aJson), JSON.parse(bJson), mJson ? JSON.parse(mJson) : null, options);
  if (!res.ok) return JSON.stringify(res);
  const t = res.triangles;
  return JSON.stringify({ ok: true, triangles: bytesToB64(new Uint8Array(t.buffer, t.byteOffset, t.byteLength)), offset: res.offset, stats: res.stats });
}"""


def write_entry(src):
    """The bundle entry: bridge.js plus the extras that apply to this release. Returns the extras."""
    eng = os.path.join(src, "plugins", "shared", "engine")
    extras, parts = [], []
    try:
        with open(os.path.join(eng, "draw_entry.js"), encoding="utf-8") as fh:
            d = fh.read()
        with open(os.path.join(eng, "bridge.js"), encoding="utf-8") as fh:
            b = fh.read()
    except OSError:
        d = b = ""
    # drawWallThrough: draw_entry.js's drawWall with the curve point Support Fins' own
    # drawnWall takes (opts.through, the site's Curve tool) passed on
    if (len(CURVE_SIG.findall(d)) == 1 and d.count(CURVE_CALL) == 1
            and "export function b64ToBytes" in b and "export function bytesToB64" in b):
        d = CURVE_SIG.sub("export function drawWallThrough(positions, a, b, through, options = {}) {", d)
        d = d.replace(CURVE_CALL, "drawnWall(seat(a), seat(b), tris, 0, { through: through ? seat(through) : null,")
        with open(os.path.join(eng, "stle_draw_through.js"), "w", encoding="utf-8") as fh:
            fh.write(d)
        parts.append(CURVE_EXPORTS)
        extras += ["drawWallThrough", "drawWallThroughB64"]
    path = os.path.join(eng, "stle_entry.js")
    with open(path, "w", encoding="utf-8") as fh:
        fh.write(ENTRY.replace("%s", "\n".join(parts)))
    return path, extras


def build(tag):
    tmp = tempfile.mkdtemp(prefix="support-fins-")
    try:
        src = os.path.join(tmp, "src")
        run(["git", "clone", "--quiet", "--depth", "1", "--branch", tag, UPSTREAM, src])
        commit = run(["git", "-C", src, "rev-parse", "HEAD"]).strip()
        bridge = os.path.join(src, "plugins", "shared", "engine", "bridge.js")
        if not os.path.isfile(bridge):
            sys.exit(f"{tag}: plugins/shared/engine/bridge.js is missing; the plugin entry has moved.")
        with open(os.path.join(src, "plugins", "shared", "engine", "options.json"), encoding="utf-8") as fh:
            schema = json.load(fh)
        if schema.get("version") != SCHEMA_VERSION:
            sys.exit(f"{tag}: options.json is version {schema.get('version')}, this app reads {SCHEMA_VERSION}.")
        stage = os.path.join(tmp, "out")
        os.makedirs(stage)
        entry, extras = write_entry(src)
        common = ["--bundle", "--target=es2022", "--minify", "--log-level=warning", "--legal-comments=eof"]
        run(esbuild() + [entry, "--format=esm", f"--outfile={os.path.join(stage, 'engine.esm.js')}"] + common)
        run(esbuild() + [entry, "--format=iife", "--global-name=SupportFinsEngine",
                         f"--outfile={os.path.join(stage, 'engine.iife.js')}"] + common)
        # the bundle must still offer everything this app calls
        probe = ("import('./engine.esm.js').then(m => { const miss = %s.filter(k => !(k in m));"
                 " if (miss.length) { console.error('missing: ' + miss.join(', ')); process.exit(1); }"
                 " const r = m.computeFins(cube(), {});"
                 " if (!(r.triangles instanceof Float32Array)) process.exit(1);"
                 " for (const k of %s) if (typeof m[k] !== 'function') { console.error('extra missing: ' + k); process.exit(1); }"
                 " if (m.drawWallThrough) { const w = m.drawWallThrough(ell(), [22, 3, 27], [22, 17, 27], [25, 10, 27], {});"
                 "   if (!w.ok) { console.error('curve: ' + w.reason); process.exit(1); } }"
                 " console.log(m.reportLine(r.stats)); });"
                 % (json.dumps(NEEDED), json.dumps(extras)))
        cube = ("function cube(){const s=[],P=[[0,0,0],[20,0,0],[20,10,0],[0,10,0],[0,0,10],[20,0,10],[20,10,10],[0,10,10]],"
                "F=[[0,2,1],[0,3,2],[4,5,6],[4,6,7],[0,1,5],[0,5,4],[1,2,6],[1,6,5],[2,3,7],[2,7,6],[3,0,4],[3,4,7]];"
                "for(const f of F)for(const i of f)s.push(...P[i]);return new Float64Array(s)}")
        # an upside-down L: a post and an arm out over the plate, for the curved wall
        ell = ("function box(s,x0,y0,z0,x1,y1,z1){const P=[[x0,y0,z0],[x1,y0,z0],[x1,y1,z0],[x0,y1,z0],[x0,y0,z1],[x1,y0,z1],[x1,y1,z1],[x0,y1,z1]],"
               "F=[[0,2,1],[0,3,2],[4,5,6],[4,6,7],[0,1,5],[0,5,4],[1,2,6],[1,6,5],[2,3,7],[2,7,6],[3,0,4],[3,4,7]];for(const f of F)for(const i of f)s.push(...P[i])}"
               "function ell(){const s=[];box(s,0,0,0,10,20,27);box(s,0,0,27,30,20,35);return new Float64Array(s)}")
        with open(os.path.join(stage, "probe.mjs"), "w", encoding="utf-8") as fh:
            fh.write(cube + "\n" + ell + "\n" + probe)
        report = run(["node", "probe.mjs"], cwd=stage).strip()
        os.remove(os.path.join(stage, "probe.mjs"))
        shutil.copyfile(os.path.join(src, "LICENSE"), os.path.join(stage, "LICENSE.txt"))
        info = {
            "name": "Support Fins", "tag": tag, "version": tag.lstrip("v"), "commit": commit,
            "source": "https://github.com/gittrahan/support-fins", "api": API,
            "built": datetime.datetime.now(datetime.timezone.utc).strftime("%Y-%m-%dT%H:%M:%SZ"),
            "sha256": {f: hashlib.sha256(open(os.path.join(stage, f), "rb").read()).hexdigest()
                       for f in ("engine.esm.js", "engine.iife.js")},
            "options": len(schema.get("options", [])),
            "extras": extras,
        }
        with open(os.path.join(stage, "VERSION.json"), "w", encoding="utf-8", newline="\n") as fh:
            json.dump(info, fh, indent=1)
            fh.write("\n")
        # the same, as a module the web engine imports (JSON modules aren't in every browser)
        with open(os.path.join(stage, "version.js"), "w", encoding="utf-8", newline="\n") as fh:
            fh.write("// written by packaging/update_support_fins.py\nexport default " + json.dumps(info, indent=1) + ";\n")
        if os.path.isdir(OUT):
            shutil.rmtree(OUT)
        shutil.copytree(stage, OUT)
        return info, report
    finally:
        shutil.rmtree(tmp, ignore_errors=True)


def main(argv):
    check = "--check" in argv
    force = "--force" in argv
    args = [a for a in argv if not a.startswith("--")]
    tag = args[0] if args else latest_tag()
    cur = current()
    have = cur["tag"] if cur else None
    if check:
        newer = have is None or semver(tag) > semver(have)
        print(f"Support Fins: installed {have or 'none'}, newest {tag}{' (update available)' if newer else ''}")
        return 10 if newer else 0
    if have == tag and not force:
        print(f"Support Fins {tag} is already vendored.")
        return 0
    info, report = build(tag)
    print(f"Vendored Support Fins {info['tag']} ({info['commit'][:7]}): {info['options']} settings; test cube: {report}")
    return 0


if __name__ == "__main__":
    sys.exit(main(sys.argv[1:]))

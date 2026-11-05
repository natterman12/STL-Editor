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
EXTRAS_DIR = os.path.join(ROOT, "packaging", "support_fins_extras")
CURVE_EXPORTS = """import { drawWallThrough } from './stle_draw_through.js';
import { soupFromB64, trianglesB64 } from './stle_seat.js';
export { drawWallThrough };
// a curved wall: a and b its ends, m a point it runs through (Support Fins' Curve tool)
export function drawWallThroughB64(soupB64, aJson, bJson, mJson, optionsJson) {
  const options = optionsJson ? JSON.parse(optionsJson) : {};
  const res = drawWallThrough(soupFromB64(soupB64), JSON.parse(aJson), JSON.parse(bJson), mJson ? JSON.parse(mJson) : null, options);
  if (!res.ok) return JSON.stringify(res);
  return JSON.stringify({ ok: true, triangles: trianglesB64(res.triangles), offset: res.offset, stats: res.stats });
}"""
# A closed upside-down L (post 10 wide, arm out to x=30 at z 27..35, 20 deep) for the probes.
PROBE_PART = r"""function ell(){const s=[],P=[[0,0],[10,0],[10,27],[30,27],[30,35],[0,35]],n=P.length;
for(let i=0;i<n;i++){const a=P[i],b=P[(i+1)%n];s.push(a[0],0,a[1],b[0],0,b[1],b[0],20,b[1],a[0],0,a[1],b[0],20,b[1],a[0],20,a[1]);}
const cap=(r,y,f)=>{const q=[[r[0],r[1]],[r[2],r[1]],[r[2],r[3]],[r[0],r[3]]];for(const t of [[0,1,2],[0,2,3]]){const o=f?[q[t[0]],q[t[2]],q[t[1]]]:t.map(i=>q[i]);for(const p of o)s.push(p[0],y,p[1]);}};
cap([0,0,10,35],0,false);cap([10,27,30,35],0,false);cap([0,0,10,35],20,true);cap([10,27,30,35],20,true);
let v=0;for(let i=0;i<s.length;i+=9){const[ax,ay,az,bx,by,bz,cx,cy,cz]=s.slice(i,i+9);v+=ax*(by*cz-bz*cy)-ay*(bx*cz-bz*cx)+az*(bx*cy-by*cx);}
if(v<0)for(let i=0;i<s.length;i+=9)for(let k=0;k<3;k++){const t=s[i+3+k];s[i+3+k]=s[i+6+k];s[i+6+k]=t;}
return new Float64Array(s)}
function sideFace(L){for(let f=0;f<L.length/9;f++){const xs=[L[f*9],L[f*9+3],L[f*9+6]];if(xs.every(x=>Math.abs(x-10)<1e-9)&&Math.max(L[f*9+2],L[f*9+5],L[f*9+8])<=27)return f;}return -1}
function must(c,msg){if(!c){console.error(msg);process.exit(1);}}"""


def extras_for(src):
    """The extras that can be made for this release: [(name, files {name: text}, entry code, exports, probe)]."""
    eng = os.path.join(src, "plugins", "shared", "engine")
    out = []
    read = lambda *p: open(os.path.join(*p), encoding="utf-8").read() if os.path.isfile(os.path.join(*p)) else ""  # noqa: E731
    seat = read(EXTRAS_DIR, "stle_seat.js")
    # drawWallThrough: draw_entry.js's drawWall with the curve point Support Fins' own
    # drawnWall takes (opts.through, the site's Curve tool) passed on
    d = read(eng, "draw_entry.js")
    if len(CURVE_SIG.findall(d)) == 1 and d.count(CURVE_CALL) == 1:
        d = CURVE_SIG.sub("export function drawWallThrough(positions, a, b, through, options = {}) {", d)
        d = d.replace(CURVE_CALL, "drawnWall(seat(a), seat(b), tris, 0, { through: through ? seat(through) : null,")
        out.append(("curves", {"stle_draw_through.js": d, "stle_seat.js": seat}, CURVE_EXPORTS,
                    ["drawWallThrough", "drawWallThroughB64"],
                    "const w = m.drawWallThrough(ell(), [22, 3, 27], [22, 17, 27], [25, 10, 27], {}); must(w.ok, 'curve: ' + w.reason);"))
    # gussets, anchor fins, Suggest and Strength: our modules calling the site's own
    for name, fname, exports, probe in (
        ("gussets", "stle_gusset.js", ["gussetWall", "gussetWallB64"],
         "const g = m.gussetWall(ell(), [10, 10, 8], [18, 10, 27], [1, 0, 0], {}); must(g.ok && g.triangles.length, 'gusset: ' + g.reason);"),
        ("anchors", "stle_anchor.js", ["anchorFin", "anchorFinB64"],
         "const L = ell(); const a = m.anchorFin(L, [10, 10, 15], sideFace(L), {}, m.optionsFromDialog({'sway.on': true}).sway);"
         " must(a.ok && a.triangles.length, 'anchor: ' + a.reason);"),
        ("orient", "stle_orient.js", ["suggestPoses", "strengthOf", "strongestPose", "suggestPosesJson", "strengthOfJson", "strongestPoseJson"],
         "const s = m.suggestPoses(ell(), {}, 3); must(s.candidates.length && s.candidates[0].rot.length === 9, 'suggest');"
         " must(['strong', 'mixed', 'weak'].includes(m.strengthOf(ell(), {}, [0, 0, 1]).posture), 'strength');"
         " must(m.strengthOf(ell(), {}, [0, 0, 1]).load.quality, 'load'); must(m.strongestPose(ell(), {}, [0, 0, 1]), 'strongest');"),
    ):
        code = read(EXTRAS_DIR, fname)
        if code and seat:
            out.append((name, {fname: code, "stle_seat.js": seat}, f"export * from './{fname[:-3]}.js';", exports, probe))
    return out


def write_entry(eng, extras, name="stle_entry.js"):
    for _, files, _, _, _ in extras:
        for fn, text in files.items():
            with open(os.path.join(eng, fn), "w", encoding="utf-8") as fh:
                fh.write(text)
    path = os.path.join(eng, name)
    with open(path, "w", encoding="utf-8") as fh:
        fh.write(ENTRY.replace("%s", "\n".join(e[2] for e in extras)))
    return path


def try_extras(src, stage, common):
    """Build and run each extra on its own; the ones that work, and why the others didn't."""
    eng = os.path.join(src, "plugins", "shared", "engine")
    ok, skipped = [], {}
    for ex in extras_for(src):
        name = ex[0]
        entry = write_entry(eng, [ex], f"stle_try_{name}.js")
        out = os.path.join(stage, f"try_{name}.mjs")
        r = subprocess.run(esbuild() + [entry, "--format=esm", f"--outfile={out}"] + common, capture_output=True, text=True)
        if r.returncode:
            lines = (r.stderr or r.stdout).strip().splitlines()
            skipped[name] = [ln.strip() for ln in lines if "ERROR" in ln or "error" in ln][:1] or lines[-1:] or ["build failed"]
            continue
        probe = os.path.join(stage, f"try_{name}_probe.mjs")
        with open(probe, "w", encoding="utf-8") as fh:
            fh.write(PROBE_PART + f"\nconst m = await import('./try_{name}.mjs');\n" + ex[4] + "\nconsole.log('ok');\n")
        r = subprocess.run(["node", probe], cwd=stage, capture_output=True, text=True, timeout=300)
        os.remove(out); os.remove(probe)
        if r.returncode:
            skipped[name] = (r.stderr or r.stdout).strip().splitlines()[-1:] or ["probe failed"]
            continue
        ok.append(ex)
    return ok, skipped


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
        common = ["--bundle", "--target=es2022", "--minify", "--log-level=warning", "--legal-comments=eof"]
        good, skipped = try_extras(src, stage, common)
        for name, why in skipped.items():
            print(f"  extra '{name}' left out for {tag}: {' '.join(why)}")
        extras = [f for ex in good for f in ex[3]]
        entry = write_entry(os.path.join(src, "plugins", "shared", "engine"), good)
        run(esbuild() + [entry, "--format=esm", f"--outfile={os.path.join(stage, 'engine.esm.js')}"] + common)
        run(esbuild() + [entry, "--format=iife", "--global-name=SupportFinsEngine",
                         f"--outfile={os.path.join(stage, 'engine.iife.js')}"] + common)
        # the bundle must still offer everything this app calls
        probe = ("import('./engine.esm.js').then(m => { const miss = %s.filter(k => !(k in m));"
                 " if (miss.length) { console.error('missing: ' + miss.join(', ')); process.exit(1); }"
                 " const r = m.computeFins(cube(), {});"
                 " if (!(r.triangles instanceof Float32Array)) process.exit(1);"
                 " for (const k of %s) if (typeof m[k] !== 'function') { console.error('extra missing: ' + k); process.exit(1); }"
                 " console.log(m.reportLine(r.stats)); });"
                 % (json.dumps(NEEDED), json.dumps(extras)))
        cube = ("function cube(){const s=[],P=[[0,0,0],[20,0,0],[20,10,0],[0,10,0],[0,0,10],[20,0,10],[20,10,10],[0,10,10]],"
                "F=[[0,2,1],[0,3,2],[4,5,6],[4,6,7],[0,1,5],[0,5,4],[1,2,6],[1,6,5],[2,3,7],[2,7,6],[3,0,4],[3,4,7]];"
                "for(const f of F)for(const i of f)s.push(...P[i]);return new Float64Array(s)}")
        with open(os.path.join(stage, "probe.mjs"), "w", encoding="utf-8") as fh:
            fh.write(cube + "\n" + probe)
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
            "features": [ex[0] for ex in good],
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

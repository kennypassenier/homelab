#!/usr/bin/env python3
"""gate-carry-meta.py — workspace facts for gate-carry.sh (fix-187).

Everything it knows comes from `cargo metadata`, never from a hand-kept
list of crate names — a fifth workspace member must not need a matching
edit here (feedback_no_hand_maintained_files).

Subcommands (each prints tab-separated lines to stdout, nothing on a
workspace with no matches):

  stem-map
      <binary-stem>\t<package-name>
      One row per lib/test/bin target. `cargo test`'s "Running ... (…
      /deps/<stem>-<hash>)" line names a binary by this stem (hyphens in
      the target name become underscores), never by its package, so
      parsing a failing-test log back to a package goes through this.

  dir-map
      <crate-dir>\t<package-name>
      One row per workspace member, `<crate-dir>` relative to the
      workspace root. Maps a changed file path to the package that owns
      it.

  foundational PKG [PKG ...]
      <package-name>\t<dependents>\t<other-members>
      For each PKG that is itself a workspace member: how many OTHER
      workspace members depend on it, directly or transitively, out of
      how many other members exist. Only PKGs at or above half of the
      other members are printed — those are "foundational": changing one
      is treated as "changed everywhere" (fix-187 fallback rule), same
      idea as the gate already singles out for proto/core in its own
      comments. A PKG below the line, or not a workspace member, prints
      nothing.
"""
import json
import subprocess
import sys


def metadata():
    out = subprocess.run(
        ["cargo", "metadata", "--format-version", "1"],
        capture_output=True, check=True, text=True,
    ).stdout
    return json.loads(out)


def workspace_packages(md):
    members = set(md["workspace_members"])
    return [p for p in md["packages"] if p["id"] in members]


def stem_map(md):
    rows = []
    for pkg in workspace_packages(md):
        for tgt in pkg["targets"]:
            if set(tgt["kind"]) & {"lib", "test", "bin"}:
                stem = tgt["name"].replace("-", "_")
                rows.append((stem, pkg["name"]))
    return rows


def dir_map(md):
    rows = []
    root = md["workspace_root"]
    for pkg in workspace_packages(md):
        manifest = pkg["manifest_path"]
        crate_dir = manifest[len(root):].lstrip("/")
        crate_dir = crate_dir.rsplit("/", 1)[0] if "/" in crate_dir else "."
        rows.append((crate_dir, pkg["name"]))
    return rows


def forward_deps(md):
    """pkg name -> set(pkg names it depends on), restricted to workspace members."""
    members = set(md["workspace_members"])
    id_to_name = {p["id"]: p["name"] for p in md["packages"]}
    graph = {id_to_name[m]: set() for m in members}
    resolve = md.get("resolve") or {}
    for node in resolve.get("nodes", []):
        if node["id"] not in members:
            continue
        name = id_to_name[node["id"]]
        for dep in node.get("deps", []):
            if dep["pkg"] in members and dep["pkg"] != node["id"]:
                graph[name].add(id_to_name[dep["pkg"]])
    return graph


def foundational(md, queried):
    fwd = forward_deps(md)
    names = list(fwd.keys())
    rev = {n: set() for n in names}
    for n, deps in fwd.items():
        for d in deps:
            rev[d].add(n)

    def reachable(start):
        seen, stack = set(), [start]
        while stack:
            cur = stack.pop()
            for n in rev.get(cur, ()):
                if n not in seen:
                    seen.add(n)
                    stack.append(n)
        return seen

    total_others = max(len(names) - 1, 0)
    rows = []
    for pkg in queried:
        if pkg not in fwd or total_others == 0:
            continue
        dependents = len(reachable(pkg))
        if dependents * 2 >= total_others:
            rows.append((pkg, dependents, total_others))
    return rows


def main(argv):
    if not argv:
        print(__doc__, file=sys.stderr)
        return 2
    cmd, rest = argv[0], argv[1:]
    md = metadata()
    if cmd == "stem-map":
        rows = stem_map(md)
    elif cmd == "dir-map":
        rows = dir_map(md)
    elif cmd == "foundational":
        rows = foundational(md, rest)
    else:
        print(f"gate-carry-meta.py: unknown subcommand {cmd!r}", file=sys.stderr)
        return 2
    for row in rows:
        print("\t".join(str(c) for c in row))
    return 0


if __name__ == "__main__":
    sys.exit(main(sys.argv[1:]))

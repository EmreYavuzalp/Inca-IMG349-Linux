#!/usr/bin/env python3
"""Collect the licenses of every crate linked into the release binaries.

usage: contrib/third-party-licenses.py > THIRD-PARTY-LICENSES

Takes the crate list from `cargo tree` (x86_64 Linux, GUI enabled, normal
dependencies only), which reflects the features actually enabled; `cargo
metadata` is only used to find each crate's sources. For each crate it
copies the license files it ships; crates that ship none get the standard text
of their license, with the crate's authors as the copyright holders.
"""
import json
import os
import re
import subprocess
import sys

TARGET = 'x86_64-unknown-linux-gnu'
LICENSE_FILE = re.compile(r'^(LICEN[CS]E|COPYING|COPYRIGHT|NOTICE|UNLICENSE|OFL|UFL)([-_.].*)?$'
                          r'|licen[cs]e.*\.(txt|md)$', re.I)
SKIP_DIRS = {'src', 'tests', 'benches', 'examples', 'target', '.git'}

MIT = """Permission is hereby granted, free of charge, to any person obtaining a copy
of this software and associated documentation files (the "Software"), to deal
in the Software without restriction, including without limitation the rights
to use, copy, modify, merge, publish, distribute, sublicense, and/or sell
copies of the Software, and to permit persons to whom the Software is
furnished to do so, subject to the following conditions:

The above copyright notice and this permission notice shall be included in all
copies or substantial portions of the Software.

THE SOFTWARE IS PROVIDED "AS IS", WITHOUT WARRANTY OF ANY KIND, EXPRESS OR
IMPLIED, INCLUDING BUT NOT LIMITED TO THE WARRANTIES OF MERCHANTABILITY,
FITNESS FOR A PARTICULAR PURPOSE AND NONINFRINGEMENT. IN NO EVENT SHALL THE
AUTHORS OR COPYRIGHT HOLDERS BE LIABLE FOR ANY CLAIM, DAMAGES OR OTHER
LIABILITY, WHETHER IN AN ACTION OF CONTRACT, TORT OR OTHERWISE, ARISING FROM,
OUT OF OR IN CONNECTION WITH THE SOFTWARE OR THE USE OR OTHER DEALINGS IN THE
SOFTWARE.
"""


def metadata():
    cmd = ['cargo', 'metadata', '--format-version', '1', '--locked',
           '--features', 'gui', '--filter-platform', TARGET]
    return json.loads(subprocess.run(cmd, check=True, capture_output=True, text=True).stdout)


def linked_packages(meta):
    cmd = ['cargo', 'tree', '-q', '--locked', '-e', 'normal', '--features', 'gui',
           '--target', TARGET, '--prefix', 'none', '--format', '{p}']
    tree = subprocess.run(cmd, check=True, capture_output=True, text=True).stdout
    wanted = set()
    for line in tree.splitlines():
        parts = line.split()  # "name vX.Y.Z [(proc-macro)] [(*)]"
        if len(parts) >= 2:
            wanted.add((parts[0], parts[1].lstrip('v')))
    by_key = {(p['name'], p['version']): p for p in meta['packages']}
    pkgs = []
    for key in wanted:
        if key[0] == 'img349ctl':
            continue
        if key not in by_key:
            sys.exit(f'{key[0]} {key[1]} from cargo tree not found in cargo metadata')
        pkgs.append(by_key[key])
    return sorted(pkgs, key=lambda p: (p['name'].lower(), p['version']))


def license_files(pkg):
    """License texts shipped in the crate root and (e.g. for bundled fonts) one level below."""
    root = os.path.dirname(pkg['manifest_path'])
    found = []
    for entry in sorted(os.listdir(root)):
        path = os.path.join(root, entry)
        if os.path.isfile(path) and LICENSE_FILE.search(entry):
            found.append(entry)
        elif os.path.isdir(path) and entry not in SKIP_DIRS and not entry.startswith('.'):
            for sub in sorted(os.listdir(path)):
                # Font folders keep each font's license in a .txt next to it.
                if os.path.isfile(os.path.join(path, sub)) and (
                        LICENSE_FILE.search(sub) or (entry == 'fonts' and sub.endswith('.txt'))):
                    found.append(f'{entry}/{sub}')
    if pkg.get('license_file'):
        extra = os.path.relpath(os.path.normpath(os.path.join(root, pkg['license_file'])), root)
        if os.path.isfile(os.path.join(root, extra)) and extra not in found:
            found.append(extra)
    return [(f, open(os.path.join(root, f), encoding='utf-8', errors='replace').read()) for f in found]


def normalized(text):
    return re.sub(r'\s+', ' ', text).strip()


def main():
    pkgs = linked_packages(metadata())
    # Canonical texts borrowed from crates that do ship them, for crates that don't.
    canonical = {}
    found = {}
    for p in pkgs:
        found[p['id']] = license_files(p)
        for name, text in found[p['id']]:
            if 'APACHE' in name.upper() and 'Apache License' in text and 'Version 2.0' in text:
                canonical.setdefault('Apache-2.0', text)

    out = sys.stdout
    out.write('Third-party software in img349ctl and img349ctl-gui\n')
    out.write('====================================================\n\n')
    out.write('The release binaries include the following Rust crates. Their licenses\n')
    out.write('are reproduced below.\n\n')
    for p in pkgs:
        out.write(f"  {p['name']} {p['version']}  ({p.get('license') or 'see below'})\n")
    missing = []
    printed = {}  # normalized text -> where it was first printed
    for p in pkgs:
        out.write('\n' + '=' * 78 + '\n')
        out.write(f"{p['name']} {p['version']}\n")
        out.write(f"License: {p.get('license') or 'see below'}\n")
        if p.get('repository'):
            out.write(f"Source: {p['repository']}\n")
        out.write('=' * 78 + '\n\n')
        files = found[p['id']]
        if files:
            for name, text in files:
                if len(files) > 1:
                    out.write(f'--- {name} ---\n\n')
                key = normalized(text)
                if key in printed:
                    out.write(f'(Same text as {printed[key]} above.)\n\n')
                else:
                    printed[key] = f"{p['name']} {p['version']}" + (f' {name}' if len(files) > 1 else '')
                    out.write(text.rstrip() + '\n\n')
            continue
        missing.append(p['name'])
        lic = p.get('license') or ''
        authors = ', '.join(re.sub(r'\s*<[^>]*>', '', a) for a in p.get('authors', [])) or f"the {p['name']} authors"
        out.write('(No license file is shipped with this crate; standard text follows.)\n\n')
        if re.search(r'\b(CC0-1\.0|Unlicense|0BSD)\b', lic):
            out.write(f'{lic}: public-domain style license, no notice required.\n\n')
        elif 'MIT' in lic:
            out.write(f'Copyright (c) {authors}\n\n{MIT}\n')
        elif 'Apache-2.0' in lic and 'Apache-2.0' in canonical:
            out.write(canonical['Apache-2.0'].rstrip() + '\n\n')
        else:
            sys.exit(f"{p['name']}: no license file and no standard text for {lic!r}")
    print(f'{len(pkgs)} crates, {len(missing)} without a license file: {", ".join(missing) or "-"}', file=sys.stderr)


if __name__ == '__main__':
    main()

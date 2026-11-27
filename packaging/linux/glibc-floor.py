#!/usr/bin/env python3
"""Hold Linux programs to the glibc of an older system.

  packaging/linux/glibc-floor.py [--check-only] FLOOR PROGRAM...

Rust's standard library refers to a few newer glibc functions as weak symbols and checks at run
time whether they exist: pidfd_spawnp and pidfd_getpid, from glibc 2.39, which it does without
when they are missing. Linked against glibc 2.39, a program still records their version as a
requirement, so the loader of an older glibc refuses to start it ("version `GLIBC_2.39' not
found"), although it would run there. Ubuntu 22.04 has glibc 2.35.

For each PROGRAM, every GLIBC_x.y requirement newer than FLOOR that only undefined weak symbols
use is dropped, in place, and those symbols lose their version, as if the program had been linked
against the older glibc, which has no such symbols to version. The loader then binds them to the
newer glibc's functions where they exist and leaves them null where they don't. A requirement any
other symbol uses stays. The script then fails when a requirement newer than FLOOR remains, so a
build that really needs a newer glibc stops instead of shipping a program older systems refuse.
--check-only changes nothing: it fails when the program would not start on FLOOR as it is.

Python's standard library only; 64-bit little-endian ELF files (x86_64, aarch64, riscv64).
"""

import collections
import re
import struct
import sys

SHT_DYNSYM = 11
SHT_GNU_VERNEED = 0x6FFFFFFE
SHT_GNU_VERSYM = 0x6FFFFFFF
VER_NDX_GLOBAL = 1
STB_WEAK = 2
SHN_UNDEF = 0
GLIBC = re.compile(r"GLIBC_(\d+(?:\.\d+)+)")

# One GLIBC_x.y requirement: its verneed entry's offset, its own (vernaux) offset, the offset of
# the vernaux before it in that entry (None for the first), the version and its version index.
Requirement = collections.namedtuple("Requirement", "need aux previous version index")


def version(text):
    """`2.35` as (2, 35), which compares as versions do."""
    return tuple(int(part) for part in text.split("."))


def dotted(v):
    return ".".join(map(str, v))


class Program:
    """A program's dynamic symbols and the glibc versions they require, read from and changed in its bytes."""

    def __init__(self, data):
        self.data = data
        if data[:4] != b"\x7fELF" or data[4] != 2 or data[5] != 1:
            raise ValueError("not a 64-bit little-endian ELF file")
        (shoff,) = struct.unpack_from("<Q", data, 0x28)
        shentsize, shnum = struct.unpack_from("<HH", data, 0x3A)
        self.sections = []
        for i in range(shnum):
            _name, kind, _flags, _addr, offset, size, link, info, _align, entsize = struct.unpack_from("<IIQQQQIIQQ", data, shoff + i * shentsize)
            self.sections.append({"type": kind, "offset": offset, "size": size, "link": link, "info": info, "entsize": entsize})
        by_type = {s["type"]: s for s in self.sections}
        self.verneed = by_type.get(SHT_GNU_VERNEED)
        self.versym = by_type.get(SHT_GNU_VERSYM)
        self.dynsym = by_type.get(SHT_DYNSYM)
        self.changed = set()

    def string(self, table, offset):
        """The text at `offset` in the string table `table` (a section index)."""
        start = self.sections[table]["offset"] + offset
        return self.data[start : self.data.index(b"\0", start)].decode("utf-8", "replace")

    def put(self, fmt, at, value):
        struct.pack_into(fmt, self.data, at, value)
        self.changed.add((at, struct.calcsize(fmt)))

    def users(self):
        """Version index -> (versym offsets of the symbols that use it, their names, whether all are undefined and weak)."""
        users = {}
        if not (self.versym and self.dynsym):
            return users
        for i in range(self.dynsym["size"] // self.dynsym["entsize"]):
            st_name, st_info, _other, st_shndx = struct.unpack_from("<IBBH", self.data, self.dynsym["offset"] + i * self.dynsym["entsize"])
            at = self.versym["offset"] + 2 * i
            (index,) = struct.unpack_from("<H", self.data, at)
            index &= 0x7FFF
            if index <= VER_NDX_GLOBAL:
                continue
            at_list, names, weak = users.get(index, ([], set(), True))
            at_list.append(at)
            names.add(self.string(self.dynsym["link"], st_name))
            users[index] = (at_list, names, weak and st_info >> 4 == STB_WEAK and st_shndx == SHN_UNDEF)
        return users

    def requirements(self):
        """Every GLIBC_x.y requirement, in file order."""
        found = []
        if not self.verneed:
            return found
        table, need = self.verneed["link"], self.verneed["offset"]
        for _ in range(self.verneed["info"]):
            _vn_version, vn_cnt, _vn_file, vn_aux, vn_next = struct.unpack_from("<HHIII", self.data, need)
            aux, previous = need + vn_aux, None
            for _ in range(vn_cnt):
                _hash, _flags, other, name, vna_next = struct.unpack_from("<IHHII", self.data, aux)
                m = GLIBC.fullmatch(self.string(table, name))
                if m:
                    found.append(Requirement(need, aux, previous, version(m.group(1)), other & 0x7FFF))
                if vna_next == 0:
                    break
                previous, aux = aux, aux + vna_next
            if vn_next == 0:
                break
            need += vn_next
        return found

    def drop(self, r):
        """Unlink requirement `r` from its verneed entry; False when it is the entry's only one, which stays."""
        (vn_cnt,) = struct.unpack_from("<H", self.data, r.need + 2)
        if vn_cnt < 2:
            return False
        (vna_next,) = struct.unpack_from("<I", self.data, r.aux + 12)
        if r.previous is None:
            (vn_aux,) = struct.unpack_from("<I", self.data, r.need + 8)
            self.put("<I", r.need + 8, vn_aux + vna_next)
        else:
            (previous_next,) = struct.unpack_from("<I", self.data, r.previous + 12)
            self.put("<I", r.previous + 12, 0 if vna_next == 0 else previous_next + vna_next)
        self.put("<H", r.need + 2, vn_cnt - 1)
        return True


def hold(path, floor, check_only):
    """Drop from `path` what only weak symbols require beyond `floor` (unless `check_only`) and say what it needs; True when that is `floor` or older."""
    with open(path, "rb") as f:
        program = Program(bytearray(f.read()))
    users = program.users()

    def weak_only(r):
        at_list, _, weak = users.get(r.index, ([], set(), False))
        return r.version > floor and bool(at_list) and weak

    def describe(r):
        return f"GLIBC_{dotted(r.version)} ({', '.join(sorted(users[r.index][1]))})"

    dropped, kept = [], set()
    while not check_only:
        r = next((r for r in program.requirements() if weak_only(r) and r.index not in kept), None)
        if r is None:
            break
        if not program.drop(r):
            kept.add(r.index)
            continue
        for at in users[r.index][0]:
            program.put("<H", at, VER_NDX_GLOBAL)
        dropped.append(describe(r))
    if program.changed:
        with open(path, "r+b") as f:
            for at, size in sorted(program.changed):
                f.seek(at)
                f.write(program.data[at : at + size])
        print(f"{path}: no longer requires {'; '.join(dropped)}")
    remaining = program.requirements()
    newest = max((r.version for r in remaining), default=None)
    print(f"{path}: needs glibc {dotted(newest) + ' or newer' if newest else 'with no versioned symbols'}")
    if newest is None or newest <= floor:
        return True
    droppable = [describe(r) for r in remaining if weak_only(r)]
    if check_only and droppable:
        print(f"error: {path}: glibc {dotted(floor)} refuses it while it requires {'; '.join(droppable)} (run without --check-only)", file=sys.stderr)
    else:
        print(f"error: {path}: needs glibc {dotted(newest)}, newer than {dotted(floor)}", file=sys.stderr)
    return False


def main(argv):
    check_only = "--check-only" in argv
    args = [a for a in argv if a != "--check-only"]
    if len(args) < 2 or not re.fullmatch(r"\d+(\.\d+)+", args[0]):
        sys.stderr.write(__doc__)
        return 2
    ok = True
    for path in args[1:]:
        try:
            ok &= hold(path, version(args[0]), check_only)
        except (OSError, ValueError, IndexError, struct.error) as e:
            print(f"error: {path}: {e}", file=sys.stderr)
            ok = False
    return 0 if ok else 1


if __name__ == "__main__":
    sys.exit(main(sys.argv[1:]))

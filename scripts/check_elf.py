"""Checks that the UI binary loads on Gizlet runtime 1: an aarch64 ELF that
needs no newer glibc symbol version than the runtime's glibc provides.
Usage: check_elf.py BINARY RUNTIME_LIBC"""

import re
import struct
import sys
from pathlib import Path

binary, libc = (Path(arg) for arg in sys.argv[1:])
head = binary.read_bytes()[:20]
if head[:4] != b"\x7fELF" or struct.unpack_from("<H", head, 18)[0] != 183:
    sys.exit(f"{binary} is not an aarch64 ELF executable")


def versions(data: bytes) -> set[tuple[int, ...]]:
    return {tuple(int(p) for p in m.group(1).split(b".")) for m in re.finditer(rb"GLIBC_([0-9]+(?:\.[0-9]+)+)", data)}


needed = max(versions(binary.read_bytes()), default=(0,))
provided = max(versions(libc.read_bytes()))
if needed > provided:
    sys.exit(f"{binary} needs GLIBC_{'.'.join(map(str, needed))}; runtime 1 has GLIBC_{'.'.join(map(str, provided))}")
print(f"{binary.name}: needs GLIBC_{'.'.join(map(str, needed))}, runtime 1 has GLIBC_{'.'.join(map(str, provided))}")

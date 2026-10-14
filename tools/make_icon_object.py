"""Convert app.ico into a PE resource object for the Windows GNU linker."""

import struct
from pathlib import Path


root = Path(__file__).resolve().parents[1]
source = (root / "assets" / "app.ico").read_bytes()
reserved, kind, count = struct.unpack_from("<HHH", source)
if (reserved, kind) != (0, 1) or not 1 <= count <= 64:
    raise ValueError("Invalid icon directory")

icons = []
group = bytearray(struct.pack("<HHH", 0, 1, count))
for index in range(count):
    entry = source[6 + 16 * index:22 + 16 * index]
    if len(entry) != 16:
        raise ValueError("Truncated icon directory")
    size, offset = struct.unpack_from("<II", entry, 8)
    payload = source[offset:offset + size]
    if not payload or len(payload) != size:
        raise ValueError("Invalid icon payload")
    icons.append(payload)
    group.extend(entry[:12] + struct.pack("<H", index + 1))

section = bytearray()


def reserve(size):
    section.extend(b"\0" * (-len(section) % 4))
    offset = len(section)
    section.extend(b"\0" * size)
    return offset


def directory(offset, entries):
    struct.pack_into("<IIHHHH", section, offset, 0, 0, 0, 0, 0, len(entries))
    for index, (resource_id, child, subdirectory) in enumerate(entries):
        struct.pack_into("<II", section, offset + 16 + 8 * index,
                         resource_id, child | (0x80000000 if subdirectory else 0))


root_dir = reserve(16 + 2 * 8)
icon_dir = reserve(16 + count * 8)
group_dir = reserve(16 + 8)
icon_languages = [reserve(16 + 8) for _ in icons]
group_language = reserve(16 + 8)
icon_data = [reserve(16) for _ in icons]
group_data = reserve(16)
payload_offsets = []
for payload in [*icons, bytes(group)]:
    offset = reserve(len(payload))
    section[offset:offset + len(payload)] = payload
    payload_offsets.append(offset)

directory(root_dir, [(3, icon_dir, True), (14, group_dir, True)])
directory(icon_dir, [(index + 1, offset, True)
                     for index, offset in enumerate(icon_languages)])
directory(group_dir, [(1, group_language, True)])
for language, data in zip(icon_languages, icon_data):
    directory(language, [(1033, data, False)])
directory(group_language, [(1033, group_data, False)])

relocations = bytearray()
for data, offset, payload in zip([*icon_data, group_data], payload_offsets,
                                 [*icons, bytes(group)]):
    struct.pack_into("<IIII", section, data, offset, len(payload), 0, 0)
    relocations.extend(struct.pack("<IIH", data, 0, 3))

raw_offset = 20 + 40
relocation_offset = raw_offset + len(section)
symbol_offset = relocation_offset + len(relocations)
header = struct.pack("<HHIIIHH", 0x8664, 1, 0, symbol_offset, 1, 0, 0)
section_header = struct.pack("<8sIIIIIIHHI", b".rsrc", 0, 0, len(section),
                             raw_offset, relocation_offset, 0, len(relocations) // 10, 0, 0x40000040)
symbol = struct.pack("<8sIhHBB", b".rsrc", 0, 1, 0, 3, 0)
(root / "assets" / "app_icon.o").write_bytes(
    header + section_header + section + relocations + symbol + struct.pack("<I", 4))

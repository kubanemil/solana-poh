"""Writes vectors/poh.json from plain hashlib loops, without the Poh state machine.

Each sequence lists entries whose num_hashes already follow Agave's tick rules,
so the tests can both verify them and replay them through Poh.
"""

import json
from hashlib import sha256
from pathlib import Path

OUT = Path(__file__).resolve().parent.parent / "vectors" / "poh.json"
ZERO = bytes(32)


def h(data: bytes) -> bytes:
    return sha256(data).digest()


def advance(start: bytes, n: int) -> bytes:
    for _ in range(n):
        start = h(start)
    return start


def chain(start: bytes, layout: list[tuple[int, bytes | None]]) -> list[dict]:
    entries, prev = [], start
    for num_hashes, mixin in layout:
        before = advance(prev, num_hashes - 1)
        prev = h(before + mixin) if mixin else h(before)
        entries.append(
            {"num_hashes": num_hashes, "mixin": mixin.hex() if mixin else None, "hash": prev.hex()}
        )
    return entries


def mixin(i: int) -> bytes:
    return h(f"mixin-{i}".encode())


def merkle_root(leaves: list[bytes]) -> bytes:
    level = [h(b"\x00" + leaf) for leaf in leaves]
    while len(level) > 1:
        level = [
            h(b"\x01" + level[i] + level[min(i + 1, len(level) - 1)])
            for i in range(0, len(level), 2)
        ]
    return level[0] if level else ZERO


def slot(
    hashes_per_tick: int, ticks: int, records: dict[int, list[int]]
) -> list[tuple[int, bytes | None]]:
    layout, n = [], 0
    for tick in range(ticks):
        used = 0
        for offset in records.get(tick, []):
            layout.append((offset - used, mixin(n)))
            used, n = offset, n + 1
        layout.append((hashes_per_tick - used, None))
    return layout


def main() -> None:
    start = h(b"solana-poh")
    sequences = [
        {
            "name": "agave_test_poh_verify",
            "start": ZERO.hex(),
            "hashes_per_tick": None,
            "entries": chain(ZERO, [(1, None), (1, ZERO), (1, ZERO), (1, None)]),
        },
        {
            "name": "ticks_and_records",
            "start": start.hex(),
            "hashes_per_tick": 10,
            "entries": chain(
                start,
                [
                    (4, mixin(0)),
                    (6, None),
                    (1, mixin(1)),
                    (1, mixin(2)),
                    (8, None),
                    (10, None),
                    (9, mixin(3)),
                    (1, None),
                ],
            ),
        },
        {
            "name": "mainnet_slot_200ms",
            "start": start.hex(),
            "hashes_per_tick": 31_250,
            "entries": chain(
                start, slot(31_250, 64, {0: [1, 100, 5_000], 31: [31_249], 63: [15_625, 31_000]})
            ),
        },
    ]
    words = [
        b"my",
        b"very",
        b"eager",
        b"mother",
        b"just",
        b"served",
        b"us",
        b"nine",
        b"pizzas",
        b"make",
        b"prime",
    ]
    vectors = {
        "chains": [
            {"start": s.hex(), "num_hashes": n, "hash": advance(s, n).hex()}
            for s, n in [
                (ZERO, 0),
                (ZERO, 1),
                (ZERO, 2),
                (ZERO, 1_000),
                (start, 31_250),
                (start, 2_000_000),
            ]
        ],
        "sequences": sequences,
        "hash_signatures": [
            {"leaves": [w.hex() for w in leaves], "root": merkle_root(leaves).hex()}
            for leaves in [[], [b"test"], words[:2], words[:3], words]
        ],
    }
    assert (
        vectors["hash_signatures"][-1]["root"]
        == "b40c847546fdceea166f927fc46c5ca33c3638236a36275c1346d3dffb84e1bc"
    )
    OUT.write_text(json.dumps(vectors, indent=1) + "\n")


if __name__ == "__main__":
    main()

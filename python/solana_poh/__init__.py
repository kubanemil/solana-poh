"""Solana Proof of History: a Rust generator and verifier with Python bindings.

`solana_poh.reference` holds the same algorithm in plain Python, for reading.
"""

from solana_poh._core import (
    Poh,
    PohEntry,
    hash_signatures,
    verify,
    verify_parallel,
    verify_tick_hash_counts,
)
from solana_poh._errors import VerifyError

__all__ = [
    "Poh",
    "PohEntry",
    "VerifyError",
    "hash_signatures",
    "verify",
    "verify_parallel",
    "verify_tick_hash_counts",
]

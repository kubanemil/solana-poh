import json
from enum import Enum

from pymerkle import InmemoryTree as MerkleTree


class EntryType(Enum):
    TRANSACTIONS = "transactions"
    TICK = "tick"


class Transaction:
    def __init__(self, signatures: list[str], message: str):
        self.message = message
        self.signatures = signatures

    def to_json(self) -> str:
        return json.dumps(self.__dict__)

    @staticmethod
    def from_json(json_str):
        data = json.loads(json_str)
        return Transaction(**data)

    def __repr__(self):
        return f"Transaction(signatures={self.signatures}, message={self.message})"


class Entry:
    def __init__(self, num_hashes: int, hash_: bytes, transactions: list[Transaction]):
        self.num_hashes = num_hashes
        self.hash = hash_
        self.transactions = transactions


def hash_transactions(transactions: list[Transaction]) -> bytes:
    signatures = [sig for tx in transactions for sig in tx.signatures]
    merkle_tree = MerkleTree(algorithm="sha256")
    for sig in signatures:
        merkle_tree.append_entry(sig.encode())

    root_hash = merkle_tree.get_state()
    return root_hash
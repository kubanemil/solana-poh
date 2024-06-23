from enum import Enum
from poh import Poh
from pymerkle import InmemoryTree as MerkleTree


class EntryType(Enum):
    TRANSACTIONS = "transactions"
    TICK = "tick"


class Transaction:
    def __init__(self, signatures: list[str], message: str):
        self.message = message
        self.signatures = signatures


class Entry:
    def __init__(self, num_hashes: int, hash_: bytes, transactions: list[Transaction]):
        self.num_hashes = num_hashes
        self.hash = hash_
        self.transactions = transactions

    @classmethod
    def new(cls, prev_hash: bytes, num_hashes: int, transactions: list[Transaction]):
        if num_hashes == 0 and transactions:
            num_hashes = 1
        
        hash_value = next_hash(prev_hash, num_hashes, transactions)
            
        return cls(num_hashes, hash_value, transactions)

    
    def verify(self, start_hash: bytes) -> bool:
        ref_hash = next_hash(start_hash, self.num_hashes, self.transactions)
        if self.hash != ref_hash:
            print(f"next_hash is invalid expected: {self.hash.hexdigest()} actual: {ref_hash.hexdigest()}")
            return False
        return True
    
    def is_tick(self) -> bool:
        return len(self.transactions) == 0
    

def hash_transactions(transactions: list[Transaction]) -> bytes:
    signatures = [sig for tx in transactions for sig in tx.signatures]
    merkle_tree = MerkleTree(algorithm="sha256")
    for sig in signatures:
        merkle_tree.append_entry(sig.encode())

    root_hash = merkle_tree.get_state()
    return root_hash


def next_hash(start_hash: bytes, num_hashes: int, transactions: list[Transaction]) -> bytes:
    if num_hashes == 0 and not transactions:
        return start_hash

    poh = Poh(start_hash)
    poh.hash(num_hashes - 1)
    if not transactions:
        return poh.tick().hash
    return poh.record(hash_transactions(transactions)).hash

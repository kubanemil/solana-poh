import json
import time

from entry import Entry, Transaction
from poh import Poh


class MaxHeightReached(Exception):
    pass


class WorkingBank:
    def __init__(self):
        self.bank = []
        self.transaction_index: int | None = 0


class Record:
    def __init__(self, mixin: bytes, transactions: list[Transaction], slot: int):
        self.mixin: bytes = mixin
        self.transactions: list[Transaction] = transactions
        self.slot: int = slot

    def to_json(self):
        transactions_json = [tx.to_json() for tx in self.transactions]
        record_dict = {
            "mixin": self.mixin.hex(),  # Convert bytes to hex string
            "transactions": transactions_json,
            "slot": self.slot,
        }
        return json.dumps(record_dict)

    @staticmethod
    def from_json(json_str):
        data = json.loads(json_str)
        mixin = bytes.fromhex(data["mixin"])  # Convert hex string back to bytes
        transactions = [Transaction.from_json(tx) for tx in data["transactions"]]
        return Record(mixin, transactions, data["slot"])

    def __repr__(self):
        return f"Record(mixin={self.mixin.hex()}, transactions={self.transactions}, slot={self.slot})"


class PohRecorder:
    def __init__(self, tick_height: int, last_entry_hash: bytes, hashes_per_tick: int):
        self.poh = Poh(
            hash_=last_entry_hash, hashes_per_tick=hashes_per_tick, tick_number=0
        )
        self.working_bank: WorkingBank = WorkingBank()
        self.ticks_from_record = 0
        self.tick_height = tick_height
        self.tick_cache: list[(Entry, int)] = []

    # lot of other additional methods are ommited for brevity (like banking, reseting, etc.)

    def record(self, mixin: bytes, transactions: list[Transaction]):
        assert len(transactions) > 0, "No transactions provided"

        while True:
            poh_entry = self.poh.record(mixin)

            if poh_entry:
                entry = Entry(poh_entry.num_hashes, poh_entry.hash, transactions)
                self.working_bank.bank.append(
                    entry
                )  # sending to bank, via thread send()
                return
            self.ticks_from_record += 1
            self.tick()

    def tick(self):
        poh_entry = self.poh.tick()

        if poh_entry:
            self.tick_height += 1
            print(f"tick_height {self.tick_height}")
            print(time.time())

            entry = Entry(poh_entry.num_hashes, poh_entry.hash, [])
            self.tick_cache.append((entry, self.tick_height))

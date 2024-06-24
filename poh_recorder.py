import json
import logging

from entry import Entry, Transaction
from poh import Poh


class Bank:
    def __init__(self):
        self.bank = []


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
        self.poh = Poh(hash_=last_entry_hash, hashes_per_tick=hashes_per_tick)
        self.bank: Bank = Bank()
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
                self.bank.bank.append(
                    entry
                )  # in original code sends to bank, via thread .sender
                return
            self.ticks_from_record += 1
            self.tick()

    def tick(self):
        poh_entry = self.poh.tick()

        if poh_entry:
            self.tick_height += 1
            logging.info(f"tick height: {self.tick_height}")
            logging.info(f"tick hash: {poh_entry.hash.hex()}")
            logging.info(f"num hashes between ticks: {poh_entry.num_hashes} \n")

            entry = Entry(poh_entry.num_hashes, poh_entry.hash, [])
            self.tick_cache.append((entry, self.tick_height))

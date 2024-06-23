from entry import Transaction, Entry
from poh import Poh

class MaxHeightReached(Exception):
    pass

class WorkingBank:
    def __init__(self):
        self.bank = []
        self.transaction_index: int | None = 0

class Record:
    def __init__(self):
        self.mixin: bytes
        self.transactions: list[Transaction]
        self.slot: int
        self.sender
        

class PohRecorder:

    def __init__(self, poh: Poh, working_bank):
        self.poh = poh
        self.working_bank: WorkingBank = WorkingBank()
        self.sender = "!!!"
        self.ticks_from_record = "???"
        self.tick_height: int = 0
        self.tick_cache: list[(Entry, int)] = []
        self.leader_first_tick_height_including_grace_ticks: int | None = None

    # lot of other additional methods are ommited for brevity

    def record(self, bank_slot: int, mixin: bytes, transactions: list[Transaction]):
        assert len(transactions) > 0, "No transactions provided"

        while True:
            working_bank = self.working_bank
            if not working_bank:
                raise MaxHeightReached
            
            poh_entry = self.poh.record(mixin)

            if poh_entry:
                num_transactions = len(transactions)

                entry = Entry(poh_entry.num_hashes, poh_entry.hash, transactions)
                working_bank.bank.append(entry) # sending to bank, via thread send()

                if working_bank.transaction_index is not None:
                    transaction_index = working_bank.transaction_index
                    working_bank.transaction_index += num_transactions
                    return transaction_index
                return None
        
            self.ticks_from_record += 1
            self.tick()
    

    def tick(self):
        poh_entry = self.poh.tick()

        if poh_entry:
            self.tick_height += 1
            print(f"tick_height {self.tick_height}")

            entry = Entry(poh_entry.num_hashes, poh_entry.hash, [])
            self.tick_cache.append((entry, self.tick_height))
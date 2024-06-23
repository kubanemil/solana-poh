from poh_recorder import PohRecorder, Record
import time


class PohService:
    def __init__(self) -> None:
        self.poh_exit = True


    def tick_producer(
        self,
        poh_recorder: PohRecorder,
        hashes_per_batch: int,
        record_receiver,
        target_ns_per_tick: int,
    ):
        next_record = None

        while True:
            should_tick = self.record_or_hash(
                next_record, 
                poh_recorder, 
                record_receiver, 
                hashes_per_batch,
                target_ns_per_tick,
            )
            if should_tick:
                poh_recorder.tick()

                if self.poh_exit:
                    break
    
    def record_or_hash(
            next_record: Record | None,
            poh_recorder: PohRecorder,
            record_receiver,
            hashes_per_batch: int,
            target_ns_per_tick: int,
    ) -> bool:
        if next_record is None:
            poh = poh_recorder.poh

            while True:
                should_tick = poh.hash(hashes_per_batch)
                ideal_time = poh.target_poh_time(target_ns_per_tick)

                if should_tick:
                    return True
                
                record = record_receiver.get()
                if record:
                    next_record = record
                    break

                if ideal_time <= time.time():
                    continue

                while ideal_time > time.time():
                    record = record_receiver.get()
                    if record:
                        next_record = record
                        break
                break
        else:
            while True:
                poh_recorder.record(next_record.slot, next_record.mixin, next_record.transactions)

                new_record = record_receiver.get()
                if not new_record:
                    break

                next_record = new_record
                    
        return False
                